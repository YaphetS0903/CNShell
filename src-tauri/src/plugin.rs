use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{PluginAuditEvent, PluginInstallRecord, PluginManifest, PluginPermissionReport},
};
use chrono::Utc;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, io::Write, path::Path};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PERMISSIONS: usize = 16;
const MAX_INSTALLED_PLUGINS: usize = 256;
const KNOWN_PERMISSIONS: &[&str] = &[
    "ui",
    "network",
    "directory",
    "terminalRead",
    "terminalInput",
    "connectionMetadata",
    "credentialProxy",
];
const REGISTRY_KEY: &str = "cnshell.plugins.registry";
const AUDIT_KEY: &str = "cnshell.plugins.audit";

pub fn inspect_file(path: &str) -> AppResult<PluginPermissionReport> {
    let path = Path::new(path);
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err(AppError::Validation(
            "插件 manifest 必须是绝对路径 JSON 文件".into(),
        ));
    }
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() || metadata.len() > MAX_MANIFEST_BYTES {
        return Err(AppError::Validation(
            "插件 manifest 必须是 256 KB 以内的普通文件".into(),
        ));
    }
    let bytes = std::fs::read(path)?;
    let value: Value = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Validation(format!("插件 manifest JSON 无效：{error}")))?;
    let manifest: PluginManifest = serde_json::from_value(value)
        .map_err(|error| AppError::Validation(format!("插件 manifest 字段无效：{error}")))?;
    validate(&manifest)
}

pub fn validate(manifest: &PluginManifest) -> AppResult<PluginPermissionReport> {
    let mut warnings = Vec::new();
    if !valid_id(&manifest.id)
        || manifest.name.trim().is_empty()
        || manifest.name.len() > 256
        || manifest.version.trim().is_empty()
        || manifest.version.len() > 128
        || manifest.api_version != 1
        || !valid_entrypoint(&manifest.entrypoint)
    {
        return Err(AppError::Validation(
            "插件 manifest 的 ID、名称、版本、API 版本或 WASM 入口无效".into(),
        ));
    }
    if manifest.permissions.len() > MAX_PERMISSIONS {
        return Err(AppError::Validation("插件请求权限数量超限".into()));
    }
    let mut seen: HashSet<&str> = HashSet::new();
    for permission in &manifest.permissions {
        if !KNOWN_PERMISSIONS.contains(&permission.as_str()) || !seen.insert(permission.as_str()) {
            return Err(AppError::Validation(format!(
                "插件权限未知或重复：{permission}"
            )));
        }
    }
    let mut domains = HashSet::new();
    for domain in &manifest.network_domains {
        let domain = domain.trim().to_ascii_lowercase();
        if domain.is_empty()
            || domain.len() > 253
            || domain.contains(['/', ':', '*', '?', '#', '@'])
            || !domains.insert(domain)
        {
            return Err(AppError::Validation(
                "插件网络域名必须是唯一的主机名，不能使用通配符".into(),
            ));
        }
    }
    if !manifest.network_domains.is_empty() && !seen.contains("network") {
        return Err(AppError::Validation(
            "声明网络域名前必须请求 network 权限".into(),
        ));
    }
    if manifest
        .permissions
        .iter()
        .any(|permission| permission == "credentialProxy")
    {
        warnings.push("credentialProxy 默认拒绝，只有短期凭据代理且每次调用需确认".into());
    }
    if manifest
        .permissions
        .iter()
        .any(|permission| permission == "terminalInput")
    {
        warnings.push("terminalInput 默认拒绝，插件不能直接获得终端输入控制权".into());
    }
    let signature_status = match manifest.signature.as_deref() {
        None => {
            warnings.push("manifest 未提供签名，不能安装或执行插件".into());
            "unsigned"
        }
        Some(_) => {
            warnings.push("签名存在但当前没有受信任发布者密钥，不能安装或执行插件".into());
            "present-unverified"
        }
    };
    let default_granted_permissions = manifest
        .permissions
        .iter()
        .filter(|permission| {
            matches!(
                permission.as_str(),
                "ui" | "connectionMetadata" | "terminalRead"
            )
        })
        .cloned()
        .collect::<Vec<_>>();
    let denied_permissions = manifest
        .permissions
        .iter()
        .filter(|permission| !default_granted_permissions.contains(permission))
        .cloned()
        .collect::<Vec<_>>();
    Ok(PluginPermissionReport {
        manifest: manifest.clone(),
        valid: true,
        signature_status: signature_status.into(),
        requested_permissions: manifest.permissions.clone(),
        default_granted_permissions,
        denied_permissions,
        warnings,
    })
}

fn valid_id(value: &str) -> bool {
    (3..=128).contains(&value.len())
        && value.bytes().all(|value| {
            value.is_ascii_lowercase() || value.is_ascii_digit() || value == b'.' || value == b'-'
        })
        && value.contains('.')
}

fn valid_entrypoint(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 256
        && value.ends_with(".wasm")
        && !Path::new(value).is_absolute()
        && !value.contains(['\\', ':'])
        && !Path::new(value)
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
}

#[allow(dead_code)]
fn manifest_digest(manifest: &PluginManifest) -> String {
    let bytes = serde_json::to_vec(manifest).unwrap_or_default();
    format!("sha256:{:x}", Sha256::digest(bytes))
}

fn file_digest(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

pub async fn list_installed(db: &Database) -> AppResult<Vec<PluginInstallRecord>> {
    Ok(db.load_named_state(REGISTRY_KEY).await?.unwrap_or_default())
}

pub async fn list_audit(db: &Database) -> AppResult<Vec<PluginAuditEvent>> {
    Ok(db.load_named_state(AUDIT_KEY).await?.unwrap_or_default())
}

async fn save_registry_and_audit(
    db: &Database,
    records: &[PluginInstallRecord],
    event: PluginAuditEvent,
) -> AppResult<()> {
    let mut events = list_audit(db).await?;
    events.push(event);
    if events.len() > 256 {
        let drop_count = events.len() - 256;
        events.drain(..drop_count);
    }
    let registry_json =
        serde_json::to_string(records).map_err(|error| AppError::Internal(error.to_string()))?;
    let audit_json =
        serde_json::to_string(&events).map_err(|error| AppError::Internal(error.to_string()))?;
    if registry_json.len() > 1024 * 1024 || audit_json.len() > 1024 * 1024 {
        return Err(AppError::Validation("插件登记或审计记录超过 1 MB".into()));
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES(?,?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(REGISTRY_KEY).bind(registry_json).bind(&now).execute(&mut *transaction).await?;
    sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES(?,?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(AUDIT_KEY).bind(audit_json).bind(&now).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(())
}

pub async fn register(db: &Database, path: &str) -> AppResult<PluginInstallRecord> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("插件 manifest 路径超过 16 KB".into()));
    }
    let canonical = std::fs::canonicalize(path)
        .map_err(|error| AppError::Unavailable(format!("解析插件 manifest 路径失败：{error}")))?;
    let canonical_string = canonical.to_string_lossy().into_owned();
    let report = inspect_file(&canonical_string)?;
    let digest = file_digest(&canonical)?;
    let now = Utc::now().to_rfc3339();
    let mut records = list_installed(db).await?;
    let previous = records
        .iter()
        .find(|item| item.id == report.manifest.id)
        .cloned();
    if previous.is_none() && records.len() >= MAX_INSTALLED_PLUGINS {
        return Err(AppError::Validation("最多登记 256 个插件".into()));
    }
    let installed_at = previous
        .as_ref()
        .map(|item| item.installed_at.clone())
        .unwrap_or_else(|| now.clone());
    let record = PluginInstallRecord {
        id: report.manifest.id.clone(),
        name: report.manifest.name.clone(),
        version: report.manifest.version.clone(),
        manifest_path: canonical_string,
        digest: digest.clone(),
        signature_status: report.signature_status.clone(),
        requested_permissions: report.requested_permissions.clone(),
        denied_permissions: report.denied_permissions.clone(),
        enabled: false,
        executable: false,
        installed_at,
        updated_at: now.clone(),
    };
    let permissions_changed = previous.as_ref().is_some_and(|item| {
        item.requested_permissions != record.requested_permissions
            || item.denied_permissions != record.denied_permissions
    });
    let action = if permissions_changed {
        "permissions-changed-blocked"
    } else if previous.is_some() {
        "updated-blocked"
    } else {
        "registered-blocked"
    };
    let detail = if permissions_changed {
        "插件权限声明已变化；新版本保持不可执行，所有权限需重新评审"
    } else {
        "插件已登记但保持不可执行：缺少受信任发布者密钥或运行时沙箱"
    };
    records.retain(|item| item.id != record.id);
    records.push(record.clone());
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: record.id.clone(),
            action: action.into(),
            detail: detail.into(),
            digest,
            created_at: now,
        },
    )
    .await?;
    Ok(record)
}

pub async fn disable(db: &Database, id: &str) -> AppResult<()> {
    if !valid_id(id) {
        return Err(AppError::Validation("插件 ID 无效".into()));
    }
    let mut records = list_installed(db).await?;
    let record = records
        .iter_mut()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    record.enabled = false;
    record.executable = false;
    record.updated_at = Utc::now().to_rfc3339();
    let digest = record.digest.clone();
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: "disabled".into(),
            detail: "插件已禁用，且仍不可执行".into(),
            digest,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

pub async fn remove(db: &Database, id: &str) -> AppResult<()> {
    if !valid_id(id) {
        return Err(AppError::Validation("插件 ID 无效".into()));
    }
    let mut records = list_installed(db).await?;
    let record = records
        .iter()
        .find(|item| item.id == id)
        .cloned()
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    records.retain(|item| item.id != id);
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: "removed".into(),
            detail: "插件登记已移除；manifest 文件不会被 CNshell 删除".into(),
            digest: record.digest,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

pub async fn export_audit(db: &Database, path: &str) -> AppResult<usize> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("插件审计导出路径超过 16 KB".into()));
    }
    let target = Path::new(path);
    if !target.is_absolute() || target.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(AppError::Validation(
            "插件审计必须导出为绝对路径 JSON 文件".into(),
        ));
    }
    let parent = target
        .parent()
        .filter(|value| value.is_dir())
        .ok_or_else(|| AppError::Validation("插件审计导出目录不存在".into()))?;
    let events = list_audit(db).await?;
    let count = events.len();
    let payload = serde_json::to_vec_pretty(&serde_json::json!({
        "schemaVersion": 1,
        "exportedAt": Utc::now().to_rfc3339(),
        "events": events,
    }))
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let target = target.to_path_buf();
    let temporary = parent.join(format!(
        ".cnshell-plugin-audit-{}.tmp",
        uuid::Uuid::new_v4()
    ));
    tokio::task::spawn_blocking(move || -> AppResult<()> {
        let result = (|| {
            let mut file = std::fs::File::options()
                .create_new(true)
                .write(true)
                .open(&temporary)?;
            file.write_all(&payload)?;
            file.sync_all()?;
            std::fs::rename(&temporary, &target)?;
            Ok(())
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temporary);
        }
        result
    })
    .await
    .map_err(|error| AppError::Internal(format!("插件审计导出任务失败：{error}")))??;
    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::Database;
    use tempfile::tempdir;

    fn manifest() -> PluginManifest {
        PluginManifest {
            id: "com.example.status".into(),
            name: "Status".into(),
            version: "1.0.0".into(),
            api_version: 1,
            entrypoint: "plugin.wasm".into(),
            permissions: vec!["ui".into(), "terminalRead".into(), "network".into()],
            network_domains: vec!["status.example.com".into()],
            publisher: Some("Example".into()),
            signature: None,
        }
    }

    #[test]
    fn grants_only_low_risk_permissions_by_default() {
        let report = validate(&manifest()).unwrap();
        assert_eq!(
            report.default_granted_permissions,
            vec!["ui", "terminalRead"]
        );
        assert_eq!(report.denied_permissions, vec!["network"]);
        assert_eq!(report.signature_status, "unsigned");
    }

    #[test]
    fn rejects_unknown_permissions_wildcards_and_untrusted_shape() {
        let mut value = manifest();
        value.permissions.push("filesystemAll".into());
        assert!(validate(&value).is_err());
        value = manifest();
        value.network_domains = vec!["*.example.com".into()];
        assert!(validate(&value).is_err());
        value = manifest();
        value.entrypoint = "../plugin.wasm".into();
        assert!(validate(&value).is_err());
    }

    #[tokio::test]
    async fn registry_keeps_untrusted_plugins_blocked_and_audited() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let manifest_path = directory.path().join("manifest.json");
        std::fs::write(&manifest_path, serde_json::to_vec(&manifest()).unwrap()).unwrap();

        let record = register(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(record.id, "com.example.status");
        assert!(!record.enabled);
        assert!(!record.executable);
        assert!(record.digest.starts_with("sha256:"));

        let mut changed = manifest();
        changed.permissions.push("terminalInput".into());
        std::fs::write(&manifest_path, serde_json::to_vec(&changed).unwrap()).unwrap();
        let updated = register(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(updated.denied_permissions.contains(&"terminalInput".into()));
        assert_eq!(updated.installed_at, record.installed_at);

        disable(&database, &record.id).await.unwrap();
        let installed = list_installed(&database).await.unwrap();
        assert_eq!(installed.len(), 1);
        assert!(!installed[0].enabled);

        remove(&database, &record.id).await.unwrap();
        assert!(list_installed(&database).await.unwrap().is_empty());
        let audit = list_audit(&database).await.unwrap();
        assert_eq!(audit.len(), 4);
        assert_eq!(audit[0].action, "registered-blocked");
        assert_eq!(audit[1].action, "permissions-changed-blocked");
        assert_eq!(audit[3].action, "removed");
        assert!(manifest_path.exists());

        let export_path = directory.path().join("plugin-audit.json");
        assert_eq!(
            export_audit(&database, export_path.to_str().unwrap())
                .await
                .unwrap(),
            4
        );
        let exported = std::fs::read_to_string(export_path).unwrap();
        assert!(exported.contains("registered-blocked"));
        assert!(!exported.contains("manifestPath"));
        assert!(!exported.contains(manifest_path.to_str().unwrap()));
    }
}
