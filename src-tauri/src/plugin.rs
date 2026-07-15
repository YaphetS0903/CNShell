use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        PluginAuditEvent, PluginInstallRecord, PluginManifest, PluginPermissionReport,
        PluginPublisherRoot, PluginRunResult,
    },
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::Utc;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::HashSet,
    io::Write,
    path::{Path, PathBuf},
    time::Instant,
};
use wasmi::{Config, Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PUBLISHER_KEY_BYTES: u64 = 64 * 1024;
const MAX_WASM_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PERMISSIONS: usize = 16;
const MAX_INSTALLED_PLUGINS: usize = 256;
const MAX_PUBLISHER_ROOTS: usize = 128;
const SANDBOX_FUEL: u64 = 10_000_000;
const SANDBOX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
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
const PUBLISHER_ROOTS_KEY: &str = "cnshell.plugins.publisher-roots";

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PublisherKeyFile {
    schema_version: u32,
    publisher_id: String,
    name: String,
    public_key: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SignatureEnvelope<'a> {
    schema_version: u32,
    manifest: &'a PluginManifest,
    entrypoint_sha256: &'a str,
}

struct VerifiedPackage {
    report: PluginPermissionReport,
    manifest_digest: String,
    entrypoint_path: PathBuf,
    entrypoint_digest: String,
}

struct SandboxState {
    limits: StoreLimits,
}

pub fn inspect_file(path: &str) -> AppResult<PluginPermissionReport> {
    let path = Path::new(path);
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err(AppError::Validation(
            "插件 manifest 必须是绝对路径 JSON 文件".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
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
        Some(value) if decode_signature(value).is_ok() => {
            warnings.push("签名存在但当前没有受信任发布者密钥，不能安装或执行插件".into());
            "present-unverified"
        }
        Some(_) => {
            warnings.push("manifest 签名格式无效，必须为 ed25519:<base64url>".into());
            "invalid"
        }
    };
    let default_granted_permissions = manifest
        .permissions
        .iter()
        .filter(|permission| matches!(permission.as_str(), "ui"))
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

fn file_digest(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    Ok(format!("sha256:{:x}", Sha256::digest(bytes)))
}

fn decode_public_key(value: &str) -> AppResult<[u8; 32]> {
    let encoded = value.strip_prefix("ed25519:").ok_or_else(|| {
        AppError::Validation("发布者公钥必须使用 ed25519:<base64url> 格式".into())
    })?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Validation("发布者公钥 Base64URL 无效".into()))?;
    bytes
        .try_into()
        .map_err(|_| AppError::Validation("Ed25519 发布者公钥必须为 32 字节".into()))
}

fn decode_signature(value: &str) -> AppResult<Signature> {
    let encoded = value
        .strip_prefix("ed25519:")
        .ok_or_else(|| AppError::Validation("插件签名必须使用 ed25519:<base64url> 格式".into()))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Validation("插件签名 Base64URL 无效".into()))?;
    Signature::from_slice(&bytes)
        .map_err(|_| AppError::Validation("Ed25519 插件签名必须为 64 字节".into()))
}

fn signature_payload(manifest: &PluginManifest, entrypoint_digest: &str) -> AppResult<Vec<u8>> {
    let mut unsigned = manifest.clone();
    unsigned.signature = None;
    serde_jcs::to_vec(&SignatureEnvelope {
        schema_version: 1,
        manifest: &unsigned,
        entrypoint_sha256: entrypoint_digest,
    })
    .map_err(|error| AppError::Internal(format!("生成插件签名载荷失败：{error}")))
}

fn publisher_fingerprint(key: &[u8; 32]) -> String {
    format!("sha256:{:x}", Sha256::digest(key))
}

fn canonical_manifest_path(path: &str) -> AppResult<PathBuf> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("插件 manifest 路径超过 16 KB".into()));
    }
    let original = Path::new(path);
    if !original.is_absolute()
        || original.extension().and_then(|value| value.to_str()) != Some("json")
    {
        return Err(AppError::Validation(
            "插件 manifest 必须是绝对路径 JSON 文件".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(original)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_MANIFEST_BYTES
    {
        return Err(AppError::Validation(
            "插件 manifest 必须是 256 KB 以内的非符号链接普通文件".into(),
        ));
    }
    std::fs::canonicalize(original)
        .map_err(|error| AppError::Unavailable(format!("解析插件 manifest 路径失败：{error}")))
}

fn entrypoint_path(manifest_path: &Path, entrypoint: &str) -> AppResult<PathBuf> {
    let parent = manifest_path
        .parent()
        .ok_or_else(|| AppError::Validation("插件 manifest 缺少父目录".into()))?;
    let candidate = parent.join(entrypoint);
    let metadata = std::fs::symlink_metadata(&candidate)
        .map_err(|error| AppError::Unavailable(format!("读取插件 WASM 失败：{error}")))?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_WASM_BYTES
    {
        return Err(AppError::Validation(
            "插件 WASM 必须是 1 字节至 16 MB 的非符号链接普通文件".into(),
        ));
    }
    let canonical_parent = std::fs::canonicalize(parent)?;
    let canonical = std::fs::canonicalize(candidate)?;
    if !canonical.starts_with(&canonical_parent) {
        return Err(AppError::Validation(
            "插件 WASM 不能位于 manifest 目录之外".into(),
        ));
    }
    Ok(canonical)
}

pub async fn list_publishers(db: &Database) -> AppResult<Vec<PluginPublisherRoot>> {
    Ok(db
        .load_named_state(PUBLISHER_ROOTS_KEY)
        .await?
        .unwrap_or_default())
}

async fn verify_package(db: &Database, path: &str) -> AppResult<VerifiedPackage> {
    let canonical = canonical_manifest_path(path)?;
    let canonical_string = canonical.to_string_lossy().into_owned();
    let mut report = inspect_file(&canonical_string)?;
    let wasm_path = entrypoint_path(&canonical, &report.manifest.entrypoint)?;
    let entrypoint_digest = file_digest(&wasm_path)?;
    let publisher_id = report
        .manifest
        .publisher
        .as_deref()
        .ok_or_else(|| AppError::Validation("插件 manifest 缺少 publisher".into()))?;
    if !valid_id(publisher_id)
        || !(report.manifest.id == publisher_id
            || report.manifest.id.starts_with(&format!("{publisher_id}.")))
    {
        return Err(AppError::Validation(
            "插件 ID 必须等于发布者 ID 或位于发布者命名空间下".into(),
        ));
    }
    let root = list_publishers(db)
        .await?
        .into_iter()
        .find(|root| root.id == publisher_id && root.enabled)
        .ok_or_else(|| {
            AppError::Unavailable(format!("发布者 {publisher_id} 尚未受信任或已撤销"))
        })?;
    let key_bytes = decode_public_key(&root.public_key)?;
    if publisher_fingerprint(&key_bytes) != root.fingerprint {
        return Err(AppError::Validation("发布者根指纹与公钥不一致".into()));
    }
    let verifying_key = VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| AppError::Validation("发布者 Ed25519 公钥无效".into()))?;
    let signature = decode_signature(
        report
            .manifest
            .signature
            .as_deref()
            .ok_or_else(|| AppError::Validation("插件 manifest 缺少签名".into()))?,
    )?;
    let payload = signature_payload(&report.manifest, &entrypoint_digest)?;
    verifying_key.verify(&payload, &signature).map_err(|_| {
        AppError::Validation("插件签名验证失败，manifest 或 WASM 可能已被修改".into())
    })?;
    report.signature_status = "verified".into();
    report.warnings.retain(|warning| !warning.contains("签名"));
    Ok(VerifiedPackage {
        report,
        manifest_digest: file_digest(&canonical)?,
        entrypoint_path: wasm_path,
        entrypoint_digest,
    })
}

pub async fn inspect_verified(db: &Database, path: &str) -> AppResult<PluginPermissionReport> {
    match verify_package(db, path).await {
        Ok(package) => Ok(package.report),
        Err(error) => {
            let mut report = inspect_file(path)?;
            report.warnings.push(error.to_string());
            Ok(report)
        }
    }
}

pub async fn import_publisher(db: &Database, path: &str) -> AppResult<PluginPublisherRoot> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("发布者密钥路径超过 16 KB".into()));
    }
    let path = Path::new(path);
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()) != Some("json") {
        return Err(AppError::Validation(
            "发布者密钥必须是绝对路径 JSON 文件".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() > MAX_PUBLISHER_KEY_BYTES
    {
        return Err(AppError::Validation(
            "发布者密钥必须是 64 KB 以内的非符号链接普通文件".into(),
        ));
    }
    let key_file: PublisherKeyFile = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|error| AppError::Validation(format!("发布者密钥 JSON 无效：{error}")))?;
    if key_file.schema_version != 1
        || !valid_id(&key_file.publisher_id)
        || key_file.name.trim().is_empty()
        || key_file.name.len() > 256
    {
        return Err(AppError::Validation("发布者密钥版本、ID 或名称无效".into()));
    }
    let key_bytes = decode_public_key(&key_file.public_key)?;
    VerifyingKey::from_bytes(&key_bytes)
        .map_err(|_| AppError::Validation("发布者 Ed25519 公钥无效".into()))?;
    let fingerprint = publisher_fingerprint(&key_bytes);
    let mut roots = list_publishers(db).await?;
    if let Some(existing) = roots.iter().find(|root| root.id == key_file.publisher_id)
        && existing.enabled
        && existing.fingerprint != fingerprint
    {
        return Err(AppError::Validation(
            "同一发布者 ID 已绑定不同公钥；请先撤销旧根，确认密钥轮换后再导入".into(),
        ));
    }
    if roots.iter().all(|root| root.id != key_file.publisher_id)
        && roots.len() >= MAX_PUBLISHER_ROOTS
    {
        return Err(AppError::Validation("最多信任 128 个插件发布者".into()));
    }
    let now = Utc::now().to_rfc3339();
    let installed_at = roots
        .iter()
        .find(|root| root.id == key_file.publisher_id)
        .map(|root| root.installed_at.clone())
        .unwrap_or_else(|| now.clone());
    let root = PluginPublisherRoot {
        id: key_file.publisher_id,
        name: key_file.name.trim().into(),
        public_key: format!("ed25519:{}", URL_SAFE_NO_PAD.encode(key_bytes)),
        fingerprint: fingerprint.clone(),
        enabled: true,
        installed_at,
        updated_at: now.clone(),
    };
    roots.retain(|item| item.id != root.id);
    roots.push(root.clone());
    save_roots_and_audit(
        db,
        &roots,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: format!("publisher:{}", root.id),
            action: "publisher-trusted".into(),
            detail: "用户明确导入并信任 Ed25519 发布者根".into(),
            digest: fingerprint,
            created_at: now,
        },
    )
    .await?;
    Ok(root)
}

pub async fn list_installed(db: &Database) -> AppResult<Vec<PluginInstallRecord>> {
    Ok(db.load_named_state(REGISTRY_KEY).await?.unwrap_or_default())
}

pub async fn list_audit(db: &Database) -> AppResult<Vec<PluginAuditEvent>> {
    Ok(db.load_named_state(AUDIT_KEY).await?.unwrap_or_default())
}

fn bounded_audit_json(
    mut events: Vec<PluginAuditEvent>,
    event: PluginAuditEvent,
) -> AppResult<String> {
    events.push(event);
    if events.len() > 256 {
        let drop_count = events.len() - 256;
        events.drain(..drop_count);
    }
    let json =
        serde_json::to_string(&events).map_err(|error| AppError::Internal(error.to_string()))?;
    if json.len() > 1024 * 1024 {
        return Err(AppError::Validation("插件审计记录超过 1 MB".into()));
    }
    Ok(json)
}

async fn save_roots_and_audit(
    db: &Database,
    roots: &[PluginPublisherRoot],
    event: PluginAuditEvent,
) -> AppResult<()> {
    let roots_json =
        serde_json::to_string(roots).map_err(|error| AppError::Internal(error.to_string()))?;
    if roots_json.len() > 1024 * 1024 {
        return Err(AppError::Validation("插件发布者根超过 1 MB".into()));
    }
    let audit_json = bounded_audit_json(list_audit(db).await?, event)?;
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES(?,?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(PUBLISHER_ROOTS_KEY).bind(roots_json).bind(&now).execute(&mut *transaction).await?;
    sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES(?,?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
        .bind(AUDIT_KEY).bind(audit_json).bind(&now).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok(())
}

async fn save_roots_registry_and_audit(
    db: &Database,
    roots: &[PluginPublisherRoot],
    records: &[PluginInstallRecord],
    event: PluginAuditEvent,
) -> AppResult<()> {
    let roots_json =
        serde_json::to_string(roots).map_err(|error| AppError::Internal(error.to_string()))?;
    let registry_json =
        serde_json::to_string(records).map_err(|error| AppError::Internal(error.to_string()))?;
    let audit_json = bounded_audit_json(list_audit(db).await?, event)?;
    if roots_json.len() > 1024 * 1024 || registry_json.len() > 1024 * 1024 {
        return Err(AppError::Validation("插件信任根或登记记录超过 1 MB".into()));
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    for (key, value) in [
        (PUBLISHER_ROOTS_KEY, roots_json),
        (REGISTRY_KEY, registry_json),
        (AUDIT_KEY, audit_json),
    ] {
        sqlx::query("INSERT INTO workspace_state(key,value,updated_at)VALUES(?,?,?)ON CONFLICT(key)DO UPDATE SET value=excluded.value,updated_at=excluded.updated_at")
            .bind(key).bind(value).bind(&now).execute(&mut *transaction).await?;
    }
    transaction.commit().await?;
    Ok(())
}

async fn append_audit(db: &Database, event: PluginAuditEvent) -> AppResult<()> {
    let audit_json = bounded_audit_json(list_audit(db).await?, event)?;
    db.save_named_state(
        AUDIT_KEY,
        &serde_json::from_str::<Value>(&audit_json)
            .map_err(|error| AppError::Internal(error.to_string()))?,
    )
    .await
}

async fn save_registry_and_audit(
    db: &Database,
    records: &[PluginInstallRecord],
    event: PluginAuditEvent,
) -> AppResult<()> {
    let registry_json =
        serde_json::to_string(records).map_err(|error| AppError::Internal(error.to_string()))?;
    let audit_json = bounded_audit_json(list_audit(db).await?, event)?;
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
    let canonical = canonical_manifest_path(path)?;
    let canonical_string = canonical.to_string_lossy().into_owned();
    let mut report = inspect_file(&canonical_string)?;
    let wasm_path = entrypoint_path(&canonical, &report.manifest.entrypoint)?;
    let entrypoint_digest = file_digest(&wasm_path)?;
    if let Ok(verified) = verify_package(db, &canonical_string).await {
        report = verified.report;
    }
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
        entrypoint_digest,
        publisher_id: report.manifest.publisher.clone(),
        signature_status: report.signature_status.clone(),
        requested_permissions: report.requested_permissions.clone(),
        denied_permissions: report.denied_permissions.clone(),
        granted_permissions: Vec::new(),
        enabled: false,
        executable: report.signature_status == "verified" && report.denied_permissions.is_empty(),
        installed_at,
        updated_at: now.clone(),
    };
    let permissions_changed = previous.as_ref().is_some_and(|item| {
        item.requested_permissions != record.requested_permissions
            || item.denied_permissions != record.denied_permissions
    });
    let action = if permissions_changed {
        "permissions-changed-blocked"
    } else if record.executable && previous.is_some() {
        "updated-verified"
    } else if record.executable {
        "registered-verified"
    } else if previous.is_some() {
        "updated-blocked"
    } else {
        "registered-blocked"
    };
    let detail = if permissions_changed {
        "插件权限声明已变化；新版本保持不可执行，所有权限需重新评审"
    } else if record.executable {
        "插件签名和 WASM 摘要验证通过；仍需用户明确启用"
    } else {
        "插件已登记但保持不可执行：签名不受信任或请求了当前运行时未开放的权限"
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
    record.granted_permissions.clear();
    record.updated_at = Utc::now().to_rfc3339();
    let digest = record.digest.clone();
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: "disabled".into(),
            detail: "插件已禁用，运行权限立即失效".into(),
            digest,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

pub async fn revoke_publisher(db: &Database, id: &str) -> AppResult<()> {
    if !valid_id(id) {
        return Err(AppError::Validation("发布者 ID 无效".into()));
    }
    let mut roots = list_publishers(db).await?;
    let root = roots
        .iter_mut()
        .find(|root| root.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件发布者 {id}")))?;
    root.enabled = false;
    root.updated_at = Utc::now().to_rfc3339();
    let fingerprint = root.fingerprint.clone();
    let mut records = list_installed(db).await?;
    for record in records
        .iter_mut()
        .filter(|record| record.publisher_id.as_deref() == Some(id))
    {
        record.enabled = false;
        record.executable = false;
        record.granted_permissions.clear();
        record.signature_status = "publisher-revoked".into();
        record.updated_at = Utc::now().to_rfc3339();
    }
    save_roots_registry_and_audit(
        db,
        &roots,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: format!("publisher:{id}"),
            action: "publisher-revoked".into(),
            detail: "发布者信任已撤销，其全部插件已立即禁用".into(),
            digest: fingerprint,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

async fn verified_record_package(
    db: &Database,
    record: &PluginInstallRecord,
) -> AppResult<VerifiedPackage> {
    let package = verify_package(db, &record.manifest_path).await?;
    if package.report.manifest.id != record.id
        || package.report.manifest.version != record.version
        || package.manifest_digest != record.digest
        || package.entrypoint_digest != record.entrypoint_digest
        || package.report.requested_permissions != record.requested_permissions
    {
        return Err(AppError::Validation(
            "插件登记后的 manifest、WASM、版本或权限已经变化".into(),
        ));
    }
    Ok(package)
}

async fn invalidate_record(db: &Database, id: &str, detail: String) -> AppResult<()> {
    let mut records = list_installed(db).await?;
    let record = records
        .iter_mut()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    record.enabled = false;
    record.executable = false;
    record.granted_permissions.clear();
    record.signature_status = "invalidated".into();
    record.updated_at = Utc::now().to_rfc3339();
    let digest = record.digest.clone();
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: "invalidated".into(),
            detail,
            digest,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

pub async fn enable(db: &Database, id: &str) -> AppResult<PluginInstallRecord> {
    if !valid_id(id) {
        return Err(AppError::Validation("插件 ID 无效".into()));
    }
    let current = list_installed(db)
        .await?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    let package = match verified_record_package(db, &current).await {
        Ok(package) => package,
        Err(error) => {
            let detail = format!("启用前校验失败：{error}");
            invalidate_record(db, id, detail).await?;
            return Err(error);
        }
    };
    if !package.report.denied_permissions.is_empty() {
        return Err(AppError::Unavailable(format!(
            "当前 WASM SDK 尚未开放这些权限：{}",
            package.report.denied_permissions.join(", ")
        )));
    }
    let mut records = list_installed(db).await?;
    let record = records
        .iter_mut()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    record.signature_status = "verified".into();
    record.executable = true;
    record.enabled = true;
    record.granted_permissions = package.report.requested_permissions;
    record.updated_at = Utc::now().to_rfc3339();
    let result = record.clone();
    save_registry_and_audit(
        db,
        &records,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: "enabled".into(),
            detail: "签名与摘要已重新验证，插件已在无 WASI 沙箱中启用".into(),
            digest: result.digest.clone(),
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await?;
    Ok(result)
}

fn run_wasm(plugin_id: String, version: String, bytes: Vec<u8>) -> AppResult<PluginRunResult> {
    if bytes.is_empty() || bytes.len() as u64 > MAX_WASM_BYTES {
        return Err(AppError::Validation("插件 WASM 大小无效".into()));
    }
    let started = Instant::now();
    let mut config = Config::default();
    config
        .consume_fuel(true)
        .set_max_recursion_depth(64)
        .set_max_stack_height(64 * 1024)
        .set_max_cached_stacks(1);
    let engine = Engine::new(&config);
    let module = Module::new(&engine, bytes.as_slice())
        .map_err(|error| AppError::Validation(format!("WASM 模块无效：{error}")))?;
    if module.imports().next().is_some() {
        return Err(AppError::Validation(
            "插件导入了未授权宿主能力；CNshell v1 沙箱不提供 WASI、网络、文件或凭据接口".into(),
        ));
    }
    let limits = StoreLimitsBuilder::new()
        .memory_size(SANDBOX_MEMORY_BYTES)
        .table_elements(1024)
        .instances(1)
        .memories(1)
        .tables(1)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(&engine, SandboxState { limits });
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(SANDBOX_FUEL)
        .map_err(|error| AppError::Internal(format!("配置 WASM 燃料失败：{error}")))?;
    let linker = Linker::<SandboxState>::new(&engine);
    let instance = linker
        .instantiate_and_start(&mut store, &module)
        .map_err(|error| AppError::Validation(format!("WASM 初始化失败：{error}")))?;
    let entry = instance
        .get_typed_func::<(), i32>(&store, "cnshell_main")
        .map_err(|_| AppError::Validation("插件必须导出 () -> i32 的 cnshell_main".into()))?;
    let status_code = entry
        .call(&mut store, ())
        .map_err(|error| AppError::Unavailable(format!("WASM 沙箱执行失败：{error}")))?;
    let remaining = store
        .get_fuel()
        .map_err(|error| AppError::Internal(format!("读取 WASM 燃料失败：{error}")))?;
    Ok(PluginRunResult {
        plugin_id,
        version,
        status_code,
        fuel_consumed: SANDBOX_FUEL.saturating_sub(remaining),
        duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
    })
}

pub async fn run(db: &Database, id: &str) -> AppResult<PluginRunResult> {
    if !valid_id(id) {
        return Err(AppError::Validation("插件 ID 无效".into()));
    }
    let record = list_installed(db)
        .await?
        .into_iter()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    if !record.enabled || !record.executable {
        return Err(AppError::Unavailable("插件未启用或不可执行".into()));
    }
    let package = match verified_record_package(db, &record).await {
        Ok(package) => package,
        Err(error) => {
            invalidate_record(db, id, format!("运行前校验失败：{error}")).await?;
            return Err(error);
        }
    };
    let bytes = std::fs::read(&package.entrypoint_path)?;
    let bytes_digest = format!("sha256:{:x}", Sha256::digest(&bytes));
    if bytes_digest != package.entrypoint_digest {
        invalidate_record(db, id, "读取期间 WASM 摘要发生变化".into()).await?;
        return Err(AppError::Validation("插件 WASM 在读取期间发生变化".into()));
    }
    let plugin_id = record.id.clone();
    let version = record.version.clone();
    let result = tokio::task::spawn_blocking(move || run_wasm(plugin_id, version, bytes))
        .await
        .map_err(|error| AppError::Internal(format!("插件沙箱任务失败：{error}")))?;
    let (action, detail) = match &result {
        Ok(value) => (
            "run-completed",
            format!(
                "WASM 沙箱运行完成，状态码 {}，消耗燃料 {}",
                value.status_code, value.fuel_consumed
            ),
        ),
        Err(error) => ("run-failed", format!("WASM 沙箱拒绝或终止运行：{error}")),
    };
    append_audit(
        db,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: id.into(),
            action: action.into(),
            detail,
            digest: record.digest,
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await?;
    result
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
    use ed25519_dalek::{Signer, SigningKey};
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
        assert_eq!(report.default_granted_permissions, vec!["ui"]);
        assert_eq!(report.denied_permissions, vec!["terminalRead", "network"]);
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

    #[cfg(unix)]
    #[test]
    fn canonical_manifest_rejects_symbolic_links_before_resolution() {
        use std::os::unix::fs::symlink;

        let directory = tempdir().unwrap();
        let target = directory.path().join("manifest-target.json");
        let link = directory.path().join("manifest.json");
        std::fs::write(&target, b"{}").unwrap();
        symlink(&target, &link).unwrap();
        assert!(canonical_manifest_path(link.to_str().unwrap()).is_err());
    }

    #[tokio::test]
    async fn registry_keeps_untrusted_plugins_blocked_and_audited() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let manifest_path = directory.path().join("manifest.json");
        std::fs::write(directory.path().join("plugin.wasm"), b"\0asm\x01\0\0\0").unwrap();
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

    async fn trusted_package(
        database: &Database,
        directory: &Path,
        wasm: &[u8],
    ) -> (PluginManifest, PathBuf) {
        let signing_key = SigningKey::from_bytes(&[7_u8; 32]);
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(signing_key.verifying_key().to_bytes())
        );
        let publisher_path = directory.join("publisher.json");
        std::fs::write(
            &publisher_path,
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "publisherId": "com.example",
                "name": "Example Publisher",
                "publicKey": public_key,
            }))
            .unwrap(),
        )
        .unwrap();
        import_publisher(database, publisher_path.to_str().unwrap())
            .await
            .unwrap();
        let wasm_path = directory.join("plugin.wasm");
        std::fs::write(&wasm_path, wasm).unwrap();
        let mut value = PluginManifest {
            id: "com.example.status".into(),
            name: "Status".into(),
            version: "1.0.0".into(),
            api_version: 1,
            entrypoint: "plugin.wasm".into(),
            permissions: vec!["ui".into()],
            network_domains: Vec::new(),
            publisher: Some("com.example".into()),
            signature: None,
        };
        let digest = file_digest(&wasm_path).unwrap();
        let signature = signing_key.sign(&signature_payload(&value, &digest).unwrap());
        value.signature = Some(format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(signature.to_bytes())
        ));
        let manifest_path = directory.join("manifest.json");
        std::fs::write(&manifest_path, serde_json::to_vec(&value).unwrap()).unwrap();
        (value, manifest_path)
    }

    #[tokio::test]
    async fn trusted_signed_plugin_runs_and_revocation_disables_it() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let wasm =
            wat::parse_str("(module (func (export \"cnshell_main\") (result i32) i32.const 7))")
                .unwrap();
        let (_, manifest_path) = trusted_package(&database, directory.path(), &wasm).await;

        let report = inspect_verified(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(report.signature_status, "verified");
        let registered = register(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(registered.executable);
        assert!(!registered.enabled);
        let enabled = enable(&database, &registered.id).await.unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.granted_permissions, vec!["ui"]);
        let result = run(&database, &registered.id).await.unwrap();
        assert_eq!(result.status_code, 7);
        assert!(result.fuel_consumed > 0);

        let old_fingerprint = list_publishers(&database).await.unwrap()[0]
            .fingerprint
            .clone();
        revoke_publisher(&database, "com.example").await.unwrap();
        let disabled = list_installed(&database).await.unwrap().remove(0);
        assert!(!disabled.enabled);
        assert!(!disabled.executable);
        assert_eq!(disabled.signature_status, "publisher-revoked");
        assert!(run(&database, &registered.id).await.is_err());

        let replacement_key = SigningKey::from_bytes(&[8_u8; 32]);
        let replacement_path = directory.path().join("replacement-publisher.json");
        std::fs::write(
            &replacement_path,
            serde_json::to_vec(&serde_json::json!({
                "schemaVersion": 1,
                "publisherId": "com.example",
                "name": "Example Publisher Rotated",
                "publicKey": format!("ed25519:{}", URL_SAFE_NO_PAD.encode(replacement_key.verifying_key().to_bytes())),
            }))
            .unwrap(),
        )
        .unwrap();
        let rotated = import_publisher(&database, replacement_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(rotated.enabled);
        assert_ne!(rotated.fingerprint, old_fingerprint);
    }

    #[tokio::test]
    async fn changed_wasm_invalidates_enabled_record() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let wasm =
            wat::parse_str("(module (func (export \"cnshell_main\") (result i32) i32.const 0))")
                .unwrap();
        let (_, manifest_path) = trusted_package(&database, directory.path(), &wasm).await;
        let record = register(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        enable(&database, &record.id).await.unwrap();
        std::fs::write(
            directory.path().join("plugin.wasm"),
            wat::parse_str("(module (func (export \"cnshell_main\") (result i32) i32.const 1))")
                .unwrap(),
        )
        .unwrap();

        assert!(run(&database, &record.id).await.is_err());
        let invalidated = list_installed(&database).await.unwrap().remove(0);
        assert!(!invalidated.enabled);
        assert!(!invalidated.executable);
        assert_eq!(invalidated.signature_status, "invalidated");
    }

    #[test]
    fn sandbox_rejects_imports_and_stops_infinite_loops() {
        let imported = wat::parse_str(
            "(module (import \"wasi_snapshot_preview1\" \"fd_write\" (func)) (func (export \"cnshell_main\") (result i32) i32.const 0))",
        )
        .unwrap();
        assert!(run_wasm("com.example.import".into(), "1".into(), imported).is_err());
        let looping = wat::parse_str(
            "(module (func (export \"cnshell_main\") (result i32) (loop br 0) i32.const 0))",
        )
        .unwrap();
        let error = run_wasm("com.example.loop".into(), "1".into(), looping).unwrap_err();
        assert!(error.to_string().contains("燃料") || error.to_string().contains("fuel"));
    }
}
