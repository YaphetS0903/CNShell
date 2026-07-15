use crate::{
    error::{AppError, AppResult},
    models::{PluginManifest, PluginPermissionReport},
};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::HashSet, path::Path};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PERMISSIONS: usize = 16;
const KNOWN_PERMISSIONS: &[&str] = &[
    "ui",
    "network",
    "directory",
    "terminalRead",
    "terminalInput",
    "connectionMetadata",
    "credentialProxy",
];

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
