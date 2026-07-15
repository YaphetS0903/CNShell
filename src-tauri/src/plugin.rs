use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        PluginAuditEvent, PluginCredentialProxyRequest, PluginInstallRecord, PluginManifest,
        PluginPermissionReport, PluginPublisherRoot, PluginRunInput, PluginRunResult,
        PluginTerminalInputRequest,
    },
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use parking_lot::Mutex;
use reqwest::{Client, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    net::{IpAddr, SocketAddr},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use wasmi::{
    Caller, Config, Engine, Extern, Linker, Module, Store, StoreLimits, StoreLimitsBuilder,
};

const MAX_MANIFEST_BYTES: u64 = 256 * 1024;
const MAX_PUBLISHER_KEY_BYTES: u64 = 64 * 1024;
const MAX_WASM_BYTES: u64 = 16 * 1024 * 1024;
const MAX_PERMISSIONS: usize = 16;
const MAX_INSTALLED_PLUGINS: usize = 256;
const MAX_PUBLISHER_ROOTS: usize = 128;
const SANDBOX_FUEL: u64 = 10_000_000;
const SANDBOX_MEMORY_BYTES: usize = 32 * 1024 * 1024;
const MAX_PLUGIN_LOG_BYTES: usize = 8 * 1024;
const MAX_PLUGIN_LOGS: usize = 32;
const MAX_PLUGIN_LOG_TOTAL_BYTES: usize = 32 * 1024;
const MAX_CONNECTION_METADATA_BYTES: usize = 16 * 1024;
const MAX_TERMINAL_SELECTION_BYTES: usize = 64 * 1024;
const MAX_PENDING_PROXY_REQUESTS: usize = 32;
const MAX_NETWORK_URL_BYTES: usize = 2048;
const MAX_NETWORK_RESPONSE_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_LISTING_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_FILE_BYTES: usize = 64 * 1024;
const MAX_DIRECTORY_ENTRIES: usize = 256;
const MAX_TERMINAL_INPUT_BYTES: usize = 4 * 1024;
const MAX_PENDING_TERMINAL_REQUESTS: usize = 32;
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
    permissions: HashSet<String>,
    connection_metadata: Vec<u8>,
    terminal_selection: Vec<u8>,
    network_metadata: Vec<u8>,
    network_response: Vec<u8>,
    network_status: i32,
    directory_listing: Vec<u8>,
    directory_file: Vec<u8>,
    logs: Vec<String>,
    log_bytes: usize,
    credential_proxy_requested: bool,
    terminal_input_requested: Option<String>,
}

#[derive(Clone)]
pub(crate) struct PendingPluginProxyRequest {
    pub plugin_id: String,
    pub plugin_digest: String,
    pub connection_id: String,
    pub operation: String,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Clone)]
pub(crate) struct PendingPluginTerminalInputRequest {
    pub plugin_id: String,
    pub plugin_digest: String,
    pub session_id: String,
    pub data: String,
    pub expires_at: chrono::DateTime<Utc>,
}

#[derive(Clone, Default)]
pub struct PluginManager {
    pending_proxy_requests: Arc<Mutex<HashMap<String, PendingPluginProxyRequest>>>,
    pending_terminal_requests: Arc<Mutex<HashMap<String, PendingPluginTerminalInputRequest>>>,
}

impl PluginManager {
    fn create_proxy_request(
        &self,
        record: &PluginInstallRecord,
        connection_id: &str,
        connection_name: &str,
    ) -> AppResult<PluginCredentialProxyRequest> {
        let now = Utc::now();
        let expires_at = now + ChronoDuration::minutes(2);
        let mut pending = self.pending_proxy_requests.lock();
        pending.retain(|_, request| request.expires_at > now);
        if pending.len() >= MAX_PENDING_PROXY_REQUESTS {
            return Err(AppError::Unavailable(
                "待确认的插件凭据代理请求过多，请先处理已有请求".into(),
            ));
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        pending.insert(
            request_id.clone(),
            PendingPluginProxyRequest {
                plugin_id: record.id.clone(),
                plugin_digest: record.digest.clone(),
                connection_id: connection_id.into(),
                operation: "connectionTest".into(),
                expires_at,
            },
        );
        Ok(PluginCredentialProxyRequest {
            request_id,
            plugin_id: record.id.clone(),
            plugin_name: record.name.clone(),
            connection_id: connection_id.into(),
            connection_name: connection_name.into(),
            operation: "connectionTest".into(),
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub fn take_proxy_request(&self, request_id: &str) -> AppResult<PendingPluginProxyRequest> {
        if request_id.len() > 128 {
            return Err(AppError::Validation("插件凭据代理请求 ID 无效".into()));
        }
        let request = self
            .pending_proxy_requests
            .lock()
            .remove(request_id)
            .ok_or_else(|| AppError::NotFound("插件凭据代理请求已过期或已使用".into()))?;
        if request.expires_at <= Utc::now() {
            return Err(AppError::Unavailable(
                "插件凭据代理请求已过期，请重新运行插件".into(),
            ));
        }
        Ok(request)
    }

    pub fn reject_proxy_request(&self, request_id: &str) -> AppResult<PendingPluginProxyRequest> {
        self.take_proxy_request(request_id)
    }

    fn create_terminal_input_request(
        &self,
        record: &PluginInstallRecord,
        session_id: &str,
        data: String,
    ) -> AppResult<PluginTerminalInputRequest> {
        let now = Utc::now();
        let expires_at = now + ChronoDuration::minutes(2);
        let mut pending = self.pending_terminal_requests.lock();
        pending.retain(|_, request| request.expires_at > now);
        if pending.len() >= MAX_PENDING_TERMINAL_REQUESTS {
            return Err(AppError::Unavailable(
                "待确认的插件终端输入请求过多，请先处理已有请求".into(),
            ));
        }
        let request_id = uuid::Uuid::new_v4().to_string();
        pending.insert(
            request_id.clone(),
            PendingPluginTerminalInputRequest {
                plugin_id: record.id.clone(),
                plugin_digest: record.digest.clone(),
                session_id: session_id.into(),
                data: data.clone(),
                expires_at,
            },
        );
        Ok(PluginTerminalInputRequest {
            request_id,
            plugin_id: record.id.clone(),
            plugin_name: record.name.clone(),
            session_id: session_id.into(),
            data,
            expires_at: expires_at.to_rfc3339(),
        })
    }

    pub fn take_terminal_input_request(
        &self,
        request_id: &str,
    ) -> AppResult<PendingPluginTerminalInputRequest> {
        if request_id.len() > 128 {
            return Err(AppError::Validation("插件终端输入请求 ID 无效".into()));
        }
        let request = self
            .pending_terminal_requests
            .lock()
            .remove(request_id)
            .ok_or_else(|| AppError::NotFound("插件终端输入请求已过期或已使用".into()))?;
        if request.expires_at <= Utc::now() {
            return Err(AppError::Unavailable(
                "插件终端输入请求已过期，请重新运行插件".into(),
            ));
        }
        Ok(request)
    }
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
        warnings
            .push("terminalInput 不会直接写入终端；每次请求都必须展示完整内容并由用户确认".into());
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
        network_domains: report.manifest.network_domains.clone(),
        denied_permissions: report.denied_permissions.clone(),
        granted_permissions: Vec::new(),
        enabled: false,
        executable: report.signature_status == "verified",
        installed_at,
        updated_at: now.clone(),
    };
    let permissions_changed = previous.as_ref().is_some_and(|item| {
        item.requested_permissions != record.requested_permissions
            || item.network_domains != record.network_domains
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
        || package.report.manifest.network_domains != record.network_domains
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

pub async fn enable(
    db: &Database,
    input: crate::models::PluginEnableInput,
) -> AppResult<PluginInstallRecord> {
    let id = input.id.as_str();
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
    if input.permissions.len() > MAX_PERMISSIONS {
        return Err(AppError::Validation("插件授予权限数量超限".into()));
    }
    let requested = package
        .report
        .requested_permissions
        .iter()
        .map(String::as_str)
        .collect::<HashSet<_>>();
    let mut granted = HashSet::new();
    for permission in &input.permissions {
        if !requested.contains(&permission.as_str()) || !granted.insert(permission.as_str()) {
            return Err(AppError::Validation(format!(
                "插件不能授予未声明或重复权限：{permission}"
            )));
        }
    }
    let mut records = list_installed(db).await?;
    let record = records
        .iter_mut()
        .find(|record| record.id == id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {id}")))?;
    record.signature_status = "verified".into();
    record.executable = true;
    record.enabled = true;
    record.granted_permissions = input.permissions;
    record.denied_permissions = record
        .requested_permissions
        .iter()
        .filter(|permission| !record.granted_permissions.contains(permission))
        .cloned()
        .collect();
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

#[derive(Default)]
struct NetworkResource {
    metadata: Vec<u8>,
    response: Vec<u8>,
    status: i32,
}

#[derive(Default)]
struct DirectoryResource {
    listing: Vec<u8>,
    file: Vec<u8>,
}

fn is_public_plugin_address(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            let [first, second, ..] = address.octets();
            !address.is_private()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_unspecified()
                && !address.is_multicast()
                && !address.is_broadcast()
                && !(first == 100 && (64..=127).contains(&second))
                && !(first == 198 && matches!(second, 18 | 19))
        }
        IpAddr::V6(address) => {
            !address.is_loopback()
                && !address.is_unspecified()
                && !address.is_multicast()
                && !address.is_unique_local()
                && !address.is_unicast_link_local()
        }
    }
}

fn validate_network_url(value: &str, domains: &[String]) -> AppResult<Url> {
    if value.is_empty() || value.len() > MAX_NETWORK_URL_BYTES {
        return Err(AppError::Validation(
            "插件网络 URL 不能为空且不能超过 2 KB".into(),
        ));
    }
    let url =
        Url::parse(value).map_err(|_| AppError::Validation("插件网络 URL 格式无效".into()))?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("插件网络 URL 缺少主机名".into()))?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
        || url.port_or_known_default() != Some(443)
    {
        return Err(AppError::Validation(
            "插件网络请求必须是无内嵌凭据、无片段且使用 443 端口的 HTTPS URL".into(),
        ));
    }
    if !domains.iter().any(|domain| domain == host) {
        return Err(AppError::Validation(format!(
            "插件未在 manifest 中声明网络域名 {host}"
        )));
    }
    Ok(url)
}

async fn fetch_network_resource(value: &str, domains: &[String]) -> AppResult<NetworkResource> {
    use futures_util::StreamExt;

    let url = validate_network_url(value, domains)?;
    let host = url
        .host_str()
        .ok_or_else(|| AppError::Validation("插件网络 URL 缺少主机名".into()))?
        .to_string();
    let mut addresses = tokio::net::lookup_host((host.as_str(), 443))
        .await
        .map_err(|error| AppError::Unavailable(format!("插件网络域名解析失败：{error}")))?
        .collect::<Vec<SocketAddr>>();
    addresses.sort_unstable();
    addresses.dedup();
    if addresses.is_empty()
        || addresses
            .iter()
            .any(|address| !is_public_plugin_address(address.ip()))
    {
        return Err(AppError::Validation(
            "插件网络域名解析到了本机、私网、链路本地或其他非公网地址".into(),
        ));
    }
    let client = Client::builder()
        .redirect(Policy::none())
        .no_proxy()
        .connect_timeout(Duration::from_secs(5))
        .timeout(Duration::from_secs(15))
        .resolve_to_addrs(&host, &addresses)
        .build()
        .map_err(|error| AppError::Unavailable(format!("插件网络客户端初始化失败：{error}")))?;
    let response = client
        .get(url.clone())
        .header("accept", "application/json, text/plain;q=0.9, */*;q=0.1")
        .send()
        .await
        .map_err(|error| AppError::Unavailable(format!("插件 HTTPS GET 失败：{error}")))?;
    if response.content_length().unwrap_or(0) > MAX_NETWORK_RESPONSE_BYTES as u64 {
        return Err(AppError::Validation("插件网络响应超过 64 KB".into()));
    }
    let status = i32::from(response.status().as_u16());
    let content_type = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("")
        .chars()
        .take(256)
        .collect::<String>();
    let final_url = response.url().to_string();
    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|error| AppError::Unavailable(format!("读取插件网络响应失败：{error}")))?;
        if body.len().saturating_add(chunk.len()) > MAX_NETWORK_RESPONSE_BYTES {
            return Err(AppError::Validation("插件网络响应超过 64 KB".into()));
        }
        body.extend_from_slice(&chunk);
    }
    let metadata = serde_json::to_vec(&serde_json::json!({
        "url": final_url,
        "status": status,
        "contentType": content_type,
        "length": body.len(),
    }))
    .map_err(|error| AppError::Internal(format!("生成插件网络元数据失败：{error}")))?;
    Ok(NetworkResource {
        metadata,
        response: body,
        status,
    })
}

fn valid_relative_plugin_path(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 4096
        && !Path::new(value).is_absolute()
        && Path::new(value)
            .components()
            .all(|part| matches!(part, std::path::Component::Normal(_)))
}

fn load_directory_resource(
    path: &str,
    relative_path: Option<&str>,
) -> AppResult<DirectoryResource> {
    let access = crate::bookmark::access_selected_directory(Path::new(path))?;
    let root = access.path().canonicalize()?;
    let mut entries = Vec::new();
    for item in std::fs::read_dir(&root)?.take(MAX_DIRECTORY_ENTRIES + 1) {
        if entries.len() >= MAX_DIRECTORY_ENTRIES {
            return Err(AppError::Validation(
                "插件授权目录超过 256 个顶层条目，请选择更具体的目录".into(),
            ));
        }
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| AppError::Validation("插件授权目录包含非 UTF-8 文件名".into()))?;
        if name.len() > 512 {
            return Err(AppError::Validation(
                "插件授权目录文件名超过 512 字节".into(),
            ));
        }
        let metadata = std::fs::symlink_metadata(item.path())?;
        let kind = if metadata.file_type().is_symlink() {
            "symlink"
        } else if metadata.is_dir() {
            "directory"
        } else if metadata.is_file() {
            "file"
        } else {
            "other"
        };
        entries.push(serde_json::json!({
            "name": name,
            "kind": kind,
            "size": if metadata.is_file() { metadata.len() } else { 0 },
        }));
    }
    entries.sort_by(|left, right| {
        left.get("name")
            .and_then(Value::as_str)
            .cmp(&right.get("name").and_then(Value::as_str))
    });
    let listing = serde_json::to_vec(&serde_json::json!({
        "entries": entries,
        "selectedRelativePath": relative_path,
    }))
    .map_err(|error| AppError::Internal(format!("生成插件目录清单失败：{error}")))?;
    if listing.len() > MAX_DIRECTORY_LISTING_BYTES {
        return Err(AppError::Validation("插件目录清单超过 64 KB".into()));
    }
    let file = match relative_path {
        None | Some("") => Vec::new(),
        Some(relative_path) => {
            if !valid_relative_plugin_path(relative_path) {
                return Err(AppError::Validation(
                    "插件目录文件必须是目录内的普通相对路径".into(),
                ));
            }
            let target = root.join(relative_path);
            let metadata = std::fs::symlink_metadata(&target)?;
            if metadata.file_type().is_symlink()
                || !metadata.is_file()
                || metadata.len() > MAX_DIRECTORY_FILE_BYTES as u64
            {
                return Err(AppError::Validation(
                    "插件只能读取授权目录内 64 KB 以内的非符号链接普通文件".into(),
                ));
            }
            let canonical = target.canonicalize()?;
            if !canonical.starts_with(&root) {
                return Err(AppError::Validation("插件目录文件越过授权根目录".into()));
            }
            std::fs::read(canonical)?
        }
    };
    Ok(DirectoryResource { listing, file })
}

struct WasmRunOutput {
    result: PluginRunResult,
    credential_proxy_requested: bool,
    terminal_input_requested: Option<String>,
}

fn required_permission_for_import(name: &str) -> Option<&'static str> {
    match name {
        "log" => Some("ui"),
        "connection_metadata_len" | "connection_metadata_read" => Some("connectionMetadata"),
        "terminal_selection_len" | "terminal_selection_read" => Some("terminalRead"),
        "network_status"
        | "network_metadata_len"
        | "network_metadata_read"
        | "network_response_len"
        | "network_response_read" => Some("network"),
        "directory_listing_len"
        | "directory_listing_read"
        | "directory_file_len"
        | "directory_file_read" => Some("directory"),
        "terminal_input_request" => Some("terminalInput"),
        "credential_proxy_connection_test" => Some("credentialProxy"),
        _ => None,
    }
}

fn guest_memory(caller: &Caller<'_, SandboxState>) -> Option<wasmi::Memory> {
    caller.get_export("memory").and_then(Extern::into_memory)
}

fn read_guest_bytes(
    caller: &Caller<'_, SandboxState>,
    pointer: i32,
    length: i32,
    maximum: usize,
) -> Option<Vec<u8>> {
    let pointer = usize::try_from(pointer).ok()?;
    let length = usize::try_from(length).ok()?;
    if length > maximum {
        return None;
    }
    let mut bytes = vec![0; length];
    guest_memory(caller)?
        .read(caller, pointer, &mut bytes)
        .ok()?;
    Some(bytes)
}

fn write_guest_bytes(
    caller: &mut Caller<'_, SandboxState>,
    pointer: i32,
    capacity: i32,
    bytes: &[u8],
) -> i32 {
    let Ok(pointer) = usize::try_from(pointer) else {
        return -1;
    };
    let Ok(capacity) = usize::try_from(capacity) else {
        return -1;
    };
    if capacity < bytes.len() {
        return -3;
    }
    let Some(memory) = guest_memory(caller) else {
        return -1;
    };
    if memory.write(caller, pointer, bytes).is_err() {
        return -1;
    }
    i32::try_from(bytes.len()).unwrap_or(-3)
}

fn run_wasm(
    plugin_id: String,
    version: String,
    bytes: Vec<u8>,
    permissions: HashSet<String>,
    connection_metadata: Vec<u8>,
    terminal_selection: Vec<u8>,
    network: NetworkResource,
    directory: DirectoryResource,
) -> AppResult<WasmRunOutput> {
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
    for import in module.imports() {
        let permission = if import.module() == "cnshell_v1" {
            required_permission_for_import(import.name())
        } else {
            None
        }
        .ok_or_else(|| {
            AppError::Validation(format!(
                "插件导入了未知宿主能力：{}.{}",
                import.module(),
                import.name()
            ))
        })?;
        if !permissions.contains(permission) {
            return Err(AppError::Validation(format!(
                "插件导入 {} 但未获 {} 权限",
                import.name(),
                permission
            )));
        }
    }
    let limits = StoreLimitsBuilder::new()
        .memory_size(SANDBOX_MEMORY_BYTES)
        .table_elements(1024)
        .instances(1)
        .memories(1)
        .tables(1)
        .trap_on_grow_failure(true)
        .build();
    let mut store = Store::new(
        &engine,
        SandboxState {
            limits,
            permissions,
            connection_metadata,
            terminal_selection,
            network_metadata: network.metadata,
            network_response: network.response,
            network_status: network.status,
            directory_listing: directory.listing,
            directory_file: directory.file,
            logs: Vec::new(),
            log_bytes: 0,
            credential_proxy_requested: false,
            terminal_input_requested: None,
        },
    );
    store.limiter(|state| &mut state.limits);
    store
        .set_fuel(SANDBOX_FUEL)
        .map_err(|error| AppError::Internal(format!("配置 WASM 燃料失败：{error}")))?;
    let mut linker = Linker::<SandboxState>::new(&engine);
    linker
        .func_wrap(
            "cnshell_v1",
            "log",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, length: i32| -> i32 {
                if !caller.data().permissions.contains("ui") {
                    return -2;
                }
                let Some(bytes) = read_guest_bytes(&caller, pointer, length, MAX_PLUGIN_LOG_BYTES)
                else {
                    return -1;
                };
                let Ok(message) = String::from_utf8(bytes) else {
                    return -1;
                };
                let state = caller.data_mut();
                if state.logs.len() >= MAX_PLUGIN_LOGS
                    || state.log_bytes.saturating_add(message.len()) > MAX_PLUGIN_LOG_TOTAL_BYTES
                {
                    return -3;
                }
                state.log_bytes += message.len();
                state.logs.push(message);
                0
            },
        )
        .map_err(|error| AppError::Internal(format!("注册插件日志 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "connection_metadata_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("connectionMetadata") {
                    return -2;
                }
                i32::try_from(caller.data().connection_metadata.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册连接元数据 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "connection_metadata_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("connectionMetadata") {
                    return -2;
                }
                let bytes = caller.data().connection_metadata.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册连接元数据读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "terminal_selection_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("terminalRead") {
                    return -2;
                }
                i32::try_from(caller.data().terminal_selection.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册终端选区 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "terminal_selection_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("terminalRead") {
                    return -2;
                }
                let bytes = caller.data().terminal_selection.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册终端选区读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "network_status",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("network") {
                    return -2;
                }
                caller.data().network_status
            },
        )
        .map_err(|error| AppError::Internal(format!("注册网络状态 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "network_metadata_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("network") {
                    return -2;
                }
                i32::try_from(caller.data().network_metadata.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册网络元数据 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "network_metadata_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("network") {
                    return -2;
                }
                let bytes = caller.data().network_metadata.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册网络元数据读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "network_response_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("network") {
                    return -2;
                }
                i32::try_from(caller.data().network_response.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册网络响应 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "network_response_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("network") {
                    return -2;
                }
                let bytes = caller.data().network_response.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册网络响应读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "directory_listing_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("directory") {
                    return -2;
                }
                i32::try_from(caller.data().directory_listing.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册目录清单 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "directory_listing_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("directory") {
                    return -2;
                }
                let bytes = caller.data().directory_listing.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册目录清单读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "directory_file_len",
            |caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("directory") {
                    return -2;
                }
                i32::try_from(caller.data().directory_file.len()).unwrap_or(-3)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册目录文件 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "directory_file_read",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, capacity: i32| -> i32 {
                if !caller.data().permissions.contains("directory") {
                    return -2;
                }
                let bytes = caller.data().directory_file.clone();
                write_guest_bytes(&mut caller, pointer, capacity, &bytes)
            },
        )
        .map_err(|error| AppError::Internal(format!("注册目录文件读取 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "terminal_input_request",
            |mut caller: Caller<'_, SandboxState>, pointer: i32, length: i32| -> i32 {
                if !caller.data().permissions.contains("terminalInput") {
                    return -2;
                }
                let Some(bytes) =
                    read_guest_bytes(&caller, pointer, length, MAX_TERMINAL_INPUT_BYTES)
                else {
                    return -1;
                };
                let Ok(data) = String::from_utf8(bytes) else {
                    return -1;
                };
                if data.is_empty() || data.contains('\0') {
                    return -1;
                }
                if caller.data().terminal_input_requested.is_some() {
                    return -3;
                }
                caller.data_mut().terminal_input_requested = Some(data);
                0
            },
        )
        .map_err(|error| AppError::Internal(format!("注册终端输入请求 ABI 失败：{error}")))?;
    linker
        .func_wrap(
            "cnshell_v1",
            "credential_proxy_connection_test",
            |mut caller: Caller<'_, SandboxState>| -> i32 {
                if !caller.data().permissions.contains("credentialProxy") {
                    return -2;
                }
                caller.data_mut().credential_proxy_requested = true;
                0
            },
        )
        .map_err(|error| AppError::Internal(format!("注册凭据代理 ABI 失败：{error}")))?;
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
    let state = store.data();
    Ok(WasmRunOutput {
        result: PluginRunResult {
            plugin_id,
            version,
            status_code,
            fuel_consumed: SANDBOX_FUEL.saturating_sub(remaining),
            duration_ms: started.elapsed().as_millis().try_into().unwrap_or(u64::MAX),
            logs: state.logs.clone(),
            credential_proxy_request: None,
            terminal_input_request: None,
        },
        credential_proxy_requested: state.credential_proxy_requested,
        terminal_input_requested: state.terminal_input_requested.clone(),
    })
}

fn sanitized_connection_metadata(profile: &crate::models::ConnectionProfile) -> AppResult<Vec<u8>> {
    let bytes = serde_json::to_vec(&serde_json::json!({
        "id": profile.id,
        "name": profile.name,
        "protocol": profile.protocol,
        "host": profile.host,
        "port": profile.port,
        "username": profile.username,
        "tags": profile.tags,
        "encoding": profile.encoding,
        "hasCredential": profile.has_credential,
    }))
    .map_err(|error| AppError::Internal(format!("生成插件连接元数据失败：{error}")))?;
    if bytes.len() > MAX_CONNECTION_METADATA_BYTES {
        return Err(AppError::Validation("插件连接元数据超过 16 KB".into()));
    }
    Ok(bytes)
}

pub async fn run(
    manager: &PluginManager,
    db: &Database,
    input: PluginRunInput,
) -> AppResult<PluginRunResult> {
    let id = input.id.as_str();
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
    let permissions = record
        .granted_permissions
        .iter()
        .cloned()
        .collect::<HashSet<_>>();
    let needs_connection =
        permissions.contains("connectionMetadata") || permissions.contains("credentialProxy");
    let connection = match input.connection_id.as_deref() {
        Some(connection_id) => Some(db.get_connection(connection_id).await?),
        None if needs_connection => {
            return Err(AppError::Validation(
                "该插件需要用户明确选择一个连接后才能运行".into(),
            ));
        }
        None => None,
    };
    if permissions.contains("credentialProxy")
        && connection
            .as_ref()
            .is_some_and(|profile| profile.protocol != "ssh")
    {
        return Err(AppError::Validation(
            "凭据代理 connectionTest 当前只支持 SSH 连接".into(),
        ));
    }
    let connection_metadata = if permissions.contains("connectionMetadata") {
        sanitized_connection_metadata(connection.as_ref().expect("connection required"))?
    } else {
        Vec::new()
    };
    let terminal_selection = if permissions.contains("terminalRead") {
        let selected = input.selected_text.ok_or_else(|| {
            AppError::Validation("该插件需要用户明确提供终端选中文本后才能运行".into())
        })?;
        if selected.len() > MAX_TERMINAL_SELECTION_BYTES {
            return Err(AppError::Validation("终端选中文本超过 64 KB".into()));
        }
        selected.into_bytes()
    } else {
        Vec::new()
    };
    let network = if permissions.contains("network") {
        let url = input.network_url.as_deref().ok_or_else(|| {
            AppError::Validation("该插件需要用户为本次运行明确填写 HTTPS URL".into())
        })?;
        fetch_network_resource(url, &package.report.manifest.network_domains).await?
    } else {
        NetworkResource::default()
    };
    let directory = if permissions.contains("directory") {
        let directory_path = input.directory_path.ok_or_else(|| {
            AppError::Validation("该插件需要用户为本次运行明确选择一个本地目录".into())
        })?;
        let relative_path = input.directory_relative_path.clone();
        tokio::task::spawn_blocking(move || {
            load_directory_resource(&directory_path, relative_path.as_deref())
        })
        .await
        .map_err(|error| AppError::Internal(format!("插件目录读取任务失败：{error}")))??
    } else {
        DirectoryResource::default()
    };
    let terminal_session_id = if permissions.contains("terminalInput") {
        Some(
            input
                .terminal_session_id
                .filter(|value| !value.is_empty() && value.len() <= 128)
                .ok_or_else(|| AppError::Validation("该插件需要用户明确选择当前终端会话".into()))?,
        )
    } else {
        None
    };
    let plugin_id = record.id.clone();
    let version = record.version.clone();
    let result = tokio::task::spawn_blocking(move || {
        run_wasm(
            plugin_id,
            version,
            bytes,
            permissions,
            connection_metadata,
            terminal_selection,
            network,
            directory,
        )
    })
    .await
    .map_err(|error| AppError::Internal(format!("插件沙箱任务失败：{error}")))?;
    let (action, detail) = match &result {
        Ok(value) => (
            "run-completed",
            format!(
                "WASM 沙箱运行完成，状态码 {}，消耗燃料 {}",
                value.result.status_code, value.result.fuel_consumed
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
            digest: record.digest.clone(),
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await?;
    let mut output = result?;
    if output.credential_proxy_requested {
        let profile = connection
            .ok_or_else(|| AppError::Validation("插件请求凭据代理时必须选择 SSH 连接".into()))?;
        let request = manager.create_proxy_request(&record, &profile.id, &profile.name)?;
        append_audit(
            db,
            PluginAuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: id.into(),
                action: "credential-proxy-requested".into(),
                detail: "插件申请一次性 connectionTest；尚未使用凭据，等待用户确认".into(),
                digest: record.digest.clone(),
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await?;
        output.result.credential_proxy_request = Some(request);
    }
    if let Some(data) = output.terminal_input_requested {
        let session_id = terminal_session_id
            .ok_or_else(|| AppError::Validation("插件请求终端输入时必须选择当前终端会话".into()))?;
        let request = manager.create_terminal_input_request(&record, &session_id, data)?;
        append_audit(
            db,
            PluginAuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                plugin_id: id.into(),
                action: "terminal-input-requested".into(),
                detail: "插件提出一次性终端输入请求；尚未发送，等待用户确认".into(),
                digest: record.digest,
                created_at: Utc::now().to_rfc3339(),
            },
        )
        .await?;
        output.result.terminal_input_request = Some(request);
    }
    Ok(output.result)
}

pub async fn validate_proxy_approval(
    db: &Database,
    request: &PendingPluginProxyRequest,
) -> AppResult<crate::models::ConnectionProfile> {
    if request.operation != "connectionTest" {
        return Err(AppError::Validation("插件凭据代理操作无效".into()));
    }
    let record = list_installed(db)
        .await?
        .into_iter()
        .find(|record| record.id == request.plugin_id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {}", request.plugin_id)))?;
    if !record.enabled
        || !record.executable
        || record.digest != request.plugin_digest
        || !record
            .granted_permissions
            .contains(&"credentialProxy".into())
    {
        return Err(AppError::Unavailable(
            "插件已禁用、变更或凭据代理权限已失效".into(),
        ));
    }
    let profile = db.get_connection(&request.connection_id).await?;
    if profile.protocol != "ssh" {
        return Err(AppError::Validation(
            "凭据代理 connectionTest 只支持 SSH 连接".into(),
        ));
    }
    Ok(profile)
}

pub async fn audit_proxy_decision(
    db: &Database,
    request: &PendingPluginProxyRequest,
    approved: bool,
) -> AppResult<()> {
    append_audit(
        db,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: request.plugin_id.clone(),
            action: if approved {
                "credential-proxy-approved".into()
            } else {
                "credential-proxy-rejected".into()
            },
            detail: if approved {
                "用户批准一次性 connectionTest；凭据仅由 CNshell 后端使用".into()
            } else {
                "用户拒绝一次性 connectionTest；未读取或使用凭据".into()
            },
            digest: request.plugin_digest.clone(),
            created_at: Utc::now().to_rfc3339(),
        },
    )
    .await
}

pub async fn validate_terminal_input_approval(
    db: &Database,
    request: &PendingPluginTerminalInputRequest,
) -> AppResult<()> {
    let record = list_installed(db)
        .await?
        .into_iter()
        .find(|record| record.id == request.plugin_id)
        .ok_or_else(|| AppError::NotFound(format!("插件 {}", request.plugin_id)))?;
    if !record.enabled
        || !record.executable
        || record.digest != request.plugin_digest
        || !record.granted_permissions.contains(&"terminalInput".into())
    {
        return Err(AppError::Unavailable(
            "插件已禁用、变更或终端输入权限已失效".into(),
        ));
    }
    if request.data.is_empty()
        || request.data.len() > MAX_TERMINAL_INPUT_BYTES
        || request.data.contains('\0')
    {
        return Err(AppError::Validation("插件终端输入请求内容无效".into()));
    }
    Ok(())
}

pub async fn audit_terminal_input_decision(
    db: &Database,
    request: &PendingPluginTerminalInputRequest,
    approved: bool,
) -> AppResult<()> {
    append_audit(
        db,
        PluginAuditEvent {
            id: uuid::Uuid::new_v4().to_string(),
            plugin_id: request.plugin_id.clone(),
            action: if approved {
                "terminal-input-approved".into()
            } else {
                "terminal-input-rejected".into()
            },
            detail: if approved {
                "用户核对完整内容后批准一次性终端输入；审计不记录正文".into()
            } else {
                "用户拒绝一次性终端输入；未向会话发送任何数据".into()
            },
            digest: request.plugin_digest.clone(),
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

    #[tokio::test]
    async fn signed_plugin_with_sensitive_permissions_can_be_enabled_selectively() {
        let directory = tempdir().unwrap();
        let database = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let wasm = wat::parse_str(
            r#"(module
                (import "cnshell_v1" "network_status" (func (result i32)))
                (func (export "cnshell_main") (result i32) i32.const 0))"#,
        )
        .unwrap();
        let signing_key = SigningKey::from_bytes(&[9_u8; 32]);
        let public_key = format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(signing_key.verifying_key().to_bytes())
        );
        let publisher_path = directory.path().join("publisher.json");
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
        import_publisher(&database, publisher_path.to_str().unwrap())
            .await
            .unwrap();
        let wasm_path = directory.path().join("plugin.wasm");
        std::fs::write(&wasm_path, &wasm).unwrap();
        let mut plugin_manifest = PluginManifest {
            id: "com.example.network".into(),
            name: "Network".into(),
            version: "1.0.0".into(),
            api_version: 1,
            entrypoint: "plugin.wasm".into(),
            permissions: vec!["network".into()],
            network_domains: vec!["api.example.com".into()],
            publisher: Some("com.example".into()),
            signature: None,
        };
        let digest = file_digest(&wasm_path).unwrap();
        plugin_manifest.signature = Some(format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(
                signing_key
                    .sign(&signature_payload(&plugin_manifest, &digest).unwrap())
                    .to_bytes()
            )
        ));
        let manifest_path = directory.path().join("manifest.json");
        std::fs::write(
            &manifest_path,
            serde_json::to_vec(&plugin_manifest).unwrap(),
        )
        .unwrap();
        let record = register(&database, manifest_path.to_str().unwrap())
            .await
            .unwrap();
        assert!(record.executable);
        assert!(!record.enabled);
        let enabled = enable(
            &database,
            crate::models::PluginEnableInput {
                id: record.id.clone(),
                permissions: Vec::new(),
            },
        )
        .await
        .unwrap();
        assert!(enabled.enabled);
        assert!(enabled.granted_permissions.is_empty());
        assert_eq!(enabled.denied_permissions, vec!["network"]);
        assert!(
            run(
                &PluginManager::default(),
                &database,
                PluginRunInput {
                    id: record.id,
                    connection_id: None,
                    selected_text: None,
                    network_url: None,
                    directory_path: None,
                    directory_relative_path: None,
                    terminal_session_id: None,
                },
            )
            .await
            .is_err()
        );
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

    fn run_input(id: &str) -> PluginRunInput {
        PluginRunInput {
            id: id.into(),
            connection_id: None,
            selected_text: None,
            network_url: None,
            directory_path: None,
            directory_relative_path: None,
            terminal_session_id: None,
        }
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
        let enabled = enable(
            &database,
            crate::models::PluginEnableInput {
                id: registered.id.clone(),
                permissions: vec!["ui".into()],
            },
        )
        .await
        .unwrap();
        assert!(enabled.enabled);
        assert_eq!(enabled.granted_permissions, vec!["ui"]);
        let manager = PluginManager::default();
        let result = run(&manager, &database, run_input(&registered.id))
            .await
            .unwrap();
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
        assert!(
            run(&manager, &database, run_input(&registered.id))
                .await
                .is_err()
        );

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
        enable(
            &database,
            crate::models::PluginEnableInput {
                id: record.id.clone(),
                permissions: vec!["ui".into()],
            },
        )
        .await
        .unwrap();
        std::fs::write(
            directory.path().join("plugin.wasm"),
            wat::parse_str("(module (func (export \"cnshell_main\") (result i32) i32.const 1))")
                .unwrap(),
        )
        .unwrap();

        assert!(
            run(&PluginManager::default(), &database, run_input(&record.id))
                .await
                .is_err()
        );
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
        assert!(
            run_wasm(
                "com.example.import".into(),
                "1".into(),
                imported,
                HashSet::new(),
                Vec::new(),
                Vec::new(),
                NetworkResource::default(),
                DirectoryResource::default(),
            )
            .is_err()
        );
        let looping = wat::parse_str(
            "(module (func (export \"cnshell_main\") (result i32) (loop br 0) i32.const 0))",
        )
        .unwrap();
        let error = run_wasm(
            "com.example.loop".into(),
            "1".into(),
            looping,
            HashSet::new(),
            Vec::new(),
            Vec::new(),
            NetworkResource::default(),
            DirectoryResource::default(),
        )
        .err()
        .unwrap();
        assert!(error.to_string().contains("燃料") || error.to_string().contains("fuel"));
    }

    #[test]
    fn bounded_host_abi_exposes_only_granted_explicit_inputs() {
        let wasm = wat::parse_str(
            r#"(module
                (import "cnshell_v1" "log" (func $log (param i32 i32) (result i32)))
                (import "cnshell_v1" "connection_metadata_len" (func $metadata_len (result i32)))
                (import "cnshell_v1" "connection_metadata_read" (func $metadata_read (param i32 i32) (result i32)))
                (import "cnshell_v1" "terminal_selection_len" (func $selection_len (result i32)))
                (import "cnshell_v1" "terminal_selection_read" (func $selection_read (param i32 i32) (result i32)))
                (import "cnshell_v1" "credential_proxy_connection_test" (func $proxy (result i32)))
                (memory (export "memory") 1)
                (data (i32.const 0) "hello")
                (func (export "cnshell_main") (result i32)
                    i32.const 0 i32.const 5 call $log drop
                    i32.const 128 call $metadata_len call $metadata_read drop
                    i32.const 512 call $selection_len call $selection_read drop
                    call $proxy drop
                    i32.const 0))"#,
        )
        .unwrap();
        let permissions = [
            "ui",
            "connectionMetadata",
            "terminalRead",
            "credentialProxy",
        ]
        .into_iter()
        .map(str::to_string)
        .collect();
        let output = run_wasm(
            "com.example.abi".into(),
            "1".into(),
            wasm.clone(),
            permissions,
            br#"{"name":"server"}"#.to_vec(),
            b"selected output".to_vec(),
            NetworkResource::default(),
            DirectoryResource::default(),
        )
        .unwrap();
        assert_eq!(output.result.logs, vec!["hello"]);
        assert!(output.credential_proxy_requested);

        let missing_permission = ["ui", "connectionMetadata", "terminalRead"]
            .into_iter()
            .map(str::to_string)
            .collect();
        assert!(
            run_wasm(
                "com.example.abi".into(),
                "1".into(),
                wasm,
                missing_permission,
                Vec::new(),
                Vec::new(),
                NetworkResource::default(),
                DirectoryResource::default(),
            )
            .is_err()
        );
    }

    #[test]
    fn network_and_directory_boundaries_reject_unsafe_inputs() {
        let domains = vec!["api.example.com".to_string()];
        assert!(validate_network_url("http://api.example.com/status", &domains).is_err());
        assert!(validate_network_url("https://api.example.com:444/status", &domains).is_err());
        assert!(validate_network_url("https://other.example.com/status", &domains).is_err());
        assert!(validate_network_url("https://api.example.com/status#secret", &domains).is_err());
        assert!(valid_relative_plugin_path("status.json"));
        assert!(!valid_relative_plugin_path("../status.json"));
        assert!(valid_relative_plugin_path("nested/status.json"));
        assert!(!valid_relative_plugin_path("/tmp/status.json"));
    }
}
