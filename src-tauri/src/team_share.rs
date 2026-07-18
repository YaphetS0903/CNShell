use crate::{
    db::{self, Database},
    error::{AppError, AppResult},
    models::{
        ConnectionProfile, SaveConnectionInput, TeamDevice, TeamShareExportInput, TeamSharePreview,
    },
    ssh,
    team::{self, TeamAuthorization},
};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use parking_lot::Mutex;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

const KEYCHAIN_SERVICE: &str = "com.cnshell.team-device";
const MAX_DEVICE_BUNDLE_BYTES: u64 = 64 * 1024;
const MAX_SHARE_FILE_BYTES: u64 = 4 * 1024 * 1024;
const MAX_SHARE_PLAINTEXT_BYTES: usize = 1024 * 1024;
const MAX_RECIPIENTS: usize = 64;
const PREVIEW_TTL: Duration = Duration::from_secs(5 * 60);

fn keychain_access() -> parking_lot::MutexGuard<'static, ()> {
    static ACCESS: OnceLock<parking_lot::Mutex<()>> = OnceLock::new();
    ACCESS.get_or_init(|| parking_lot::Mutex::new(())).lock()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct DeviceBundle {
    schema_version: u32,
    workspace_id: String,
    member_id: String,
    device_id: String,
    name: String,
    encryption_public_key: String,
    signing_public_key: String,
    fingerprint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareRecipient {
    device_id: String,
    member_id: String,
    nonce: String,
    wrapped_key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ShareEnvelope {
    schema_version: u32,
    share_id: String,
    workspace_id: String,
    key_epoch: i64,
    sender_member_id: String,
    sender_device_id: String,
    ephemeral_public_key: String,
    payload_nonce: String,
    ciphertext: String,
    recipients: Vec<ShareRecipient>,
    signature: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SharedConnection {
    protocol: String,
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    tags: Vec<String>,
    encoding: String,
    credential: Option<String>,
}

impl Drop for SharedConnection {
    fn drop(&mut self) {
        if let Some(credential) = self.credential.as_mut() {
            credential.zeroize();
        }
    }
}

struct PendingShare {
    workspace_id: String,
    sender_member_id: String,
    key_epoch: i64,
    connection: SharedConnection,
    expires_at: Instant,
}

#[derive(Clone, Default)]
pub struct TeamShareManager {
    pending: Arc<Mutex<HashMap<String, PendingShare>>>,
}

impl TeamShareManager {
    fn insert(
        &self,
        workspace_id: String,
        sender_member_id: String,
        key_epoch: i64,
        connection: SharedConnection,
    ) -> TeamSharePreview {
        let mut pending = self.pending.lock();
        pending.retain(|_, value| value.expires_at > Instant::now());
        let request_id = uuid::Uuid::new_v4().to_string();
        let preview = TeamSharePreview {
            request_id: request_id.clone(),
            workspace_id: workspace_id.clone(),
            sender_member_id: sender_member_id.clone(),
            connection_name: connection.name.clone(),
            protocol: connection.protocol.clone(),
            host: connection.host.clone(),
            has_credential: connection.credential.is_some(),
            key_epoch,
            expires_at: (Utc::now() + ChronoDuration::minutes(5)).to_rfc3339(),
        };
        pending.insert(
            request_id,
            PendingShare {
                workspace_id,
                sender_member_id,
                key_epoch,
                connection,
                expires_at: Instant::now() + PREVIEW_TTL,
            },
        );
        preview
    }

    fn take(&self, request_id: &str) -> AppResult<PendingShare> {
        if uuid::Uuid::parse_str(request_id).is_err() {
            return Err(AppError::Validation("安全分享请求 ID 无效".into()));
        }
        let mut pending = self.pending.lock();
        pending.retain(|_, value| value.expires_at > Instant::now());
        pending
            .remove(request_id)
            .ok_or_else(|| AppError::Unavailable("安全分享预览已过期，请重新选择文件".into()))
    }
}

fn clean_name(value: &str, field: &str) -> AppResult<String> {
    let value = value.trim();
    if value.is_empty() || value.len() > 256 || value.chars().any(|value| value.is_control()) {
        return Err(AppError::Validation(format!("{field}无效")));
    }
    Ok(value.into())
}

fn key_account(device_id: &str, kind: &str) -> String {
    format!("device:{device_id}:{kind}")
}

pub(crate) fn save_private_key(device_id: &str, kind: &str, bytes: &[u8; 32]) -> AppResult<()> {
    let _access = keychain_access();
    let mut encoded = URL_SAFE_NO_PAD.encode(bytes);
    let result = keyring::Entry::new(KEYCHAIN_SERVICE, &key_account(device_id, kind))
        .map_err(|error| {
            AppError::Storage(format!(
                "无法在{}中创建设备密钥项：{error}",
                crate::platform::credential_store_name()
            ))
        })?
        .set_password(&encoded)
        .map_err(|error| AppError::Storage(format!("保存设备私钥失败：{error}")));
    encoded.zeroize();
    result
}

pub(crate) fn load_private_key(device_id: &str, kind: &str) -> AppResult<[u8; 32]> {
    let _access = keychain_access();
    let mut value = keyring::Entry::new(KEYCHAIN_SERVICE, &key_account(device_id, kind))
        .map_err(|error| {
            AppError::Storage(format!(
                "无法在{}中创建设备密钥项：{error}",
                crate::platform::credential_store_name()
            ))
        })?
        .get_password()
        .map_err(|error| AppError::Unavailable(format!("读取本机设备私钥失败：{error}")))?;
    let decoded = URL_SAFE_NO_PAD.decode(&value).map_err(|_| {
        AppError::Storage(format!(
            "{}中的设备私钥编码无效",
            crate::platform::credential_store_name()
        ))
    });
    value.zeroize();
    decoded?.try_into().map_err(|_| {
        AppError::Storage(format!(
            "{}中的设备私钥长度无效",
            crate::platform::credential_store_name()
        ))
    })
}

pub(crate) fn delete_private_keys(device_id: &str) {
    let _access = keychain_access();
    for kind in ["x25519", "ed25519"] {
        if let Ok(entry) = keyring::Entry::new(KEYCHAIN_SERVICE, &key_account(device_id, kind)) {
            let _ = entry.delete_credential();
        }
    }
}

pub(crate) fn encode_key(prefix: &str, bytes: &[u8; 32]) -> String {
    format!("{prefix}:{}", URL_SAFE_NO_PAD.encode(bytes))
}

pub(crate) fn device_fingerprint(encryption_key: &[u8; 32], signing_key: &[u8; 32]) -> String {
    let mut digest = Sha256::new();
    digest.update(b"cnshell-team-device-v1\0");
    digest.update(encryption_key);
    digest.update(signing_key);
    format!("sha256:{:x}", digest.finalize())
}

pub(crate) fn decode_key(value: &str, prefix: &str) -> AppResult<[u8; 32]> {
    let encoded = value
        .strip_prefix(&format!("{prefix}:"))
        .ok_or_else(|| AppError::Validation(format!("密钥必须使用 {prefix}:<base64url> 格式")))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Validation("设备公钥 Base64URL 无效".into()))?;
    bytes
        .try_into()
        .map_err(|_| AppError::Validation("设备公钥必须为 32 字节".into()))
}

pub(crate) fn validated_device_keys(device: &TeamDevice) -> AppResult<([u8; 32], [u8; 32])> {
    let encryption = decode_key(&device.encryption_public_key, "x25519")?;
    let signing = decode_key(&device.signing_public_key, "ed25519")?;
    if device.fingerprint != device_fingerprint(&encryption, &signing) {
        return Err(AppError::Validation(format!(
            "设备 {} 的组合 SHA-256 指纹与公钥不一致",
            device.id
        )));
    }
    Ok((encryption, signing))
}

async fn atomic_write(path: &str, extension: &str, payload: Vec<u8>) -> AppResult<()> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("导出路径超过 16 KB".into()));
    }
    let target = Path::new(path);
    if !target.is_absolute()
        || target.extension().and_then(|value| value.to_str()) != Some(extension)
    {
        return Err(AppError::Validation(format!(
            "导出文件必须使用 .{extension} 扩展名"
        )));
    }
    let parent = target
        .parent()
        .filter(|value| value.is_dir())
        .ok_or_else(|| AppError::Validation("导出目录不存在".into()))?;
    let target = target.to_path_buf();
    let temporary = parent.join(format!(".cnshell-team-{}.tmp", uuid::Uuid::new_v4()));
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
    .map_err(|error| AppError::Internal(format!("原子导出任务失败：{error}")))?
}

fn validate_input_file(path: &str, extension: &str, max_bytes: u64) -> AppResult<PathBuf> {
    if path.len() > 16 * 1024 {
        return Err(AppError::Validation("导入路径超过 16 KB".into()));
    }
    let path = Path::new(path);
    if !path.is_absolute() || path.extension().and_then(|value| value.to_str()) != Some(extension) {
        return Err(AppError::Validation(format!(
            "导入文件必须使用 .{extension} 扩展名"
        )));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > max_bytes
    {
        return Err(AppError::Validation("导入文件大小无效或为符号链接".into()));
    }
    std::fs::canonicalize(path).map_err(AppError::from)
}

pub async fn list_devices(db: &Database, workspace_id: &str) -> AppResult<Vec<TeamDevice>> {
    team::authorize(db, workspace_id, "memberRead").await?;
    let devices = sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE workspace_id=? ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END,is_local DESC,name COLLATE NOCASE")
        .bind(workspace_id)
        .fetch_all(&db.pool)
        .await?;
    for device in &devices {
        validated_device_keys(device)?;
    }
    Ok(devices)
}

pub async fn ensure_local_device(
    db: &Database,
    workspace_id: &str,
    name: &str,
) -> AppResult<TeamDevice> {
    let authorization = team::authorize(db, workspace_id, "workspaceRead").await?;
    if let Some(device_id) = authorization.local_device_id.as_deref() {
        return sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE id=? AND workspace_id=? AND member_id=? AND is_local=1 AND status='active'")
            .bind(device_id)
            .bind(workspace_id)
            .bind(&authorization.member_id)
            .fetch_optional(&db.pool)
            .await?
            .ok_or_else(|| AppError::Storage("工作区本机设备引用无效".into()));
    }
    let name = clean_name(name, "设备名称")?;
    let device_id = uuid::Uuid::new_v4().to_string();
    let mut encryption_secret = Zeroizing::new([0_u8; 32]);
    let mut signing_secret = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(&mut *encryption_secret);
    OsRng.fill_bytes(&mut *signing_secret);
    let encryption_private = StaticSecret::from(*encryption_secret);
    let encryption_public = X25519PublicKey::from(&encryption_private).to_bytes();
    let signing_key = SigningKey::from_bytes(&signing_secret);
    let signing_public = signing_key.verifying_key().to_bytes();
    save_private_key(&device_id, "x25519", &encryption_secret)?;
    if let Err(error) = save_private_key(&device_id, "ed25519", &signing_secret) {
        delete_private_keys(&device_id);
        return Err(error);
    }
    let now = Utc::now().to_rfc3339();
    let device = TeamDevice {
        id: device_id.clone(),
        workspace_id: workspace_id.into(),
        member_id: authorization.member_id.clone(),
        name,
        encryption_public_key: encode_key("x25519", &encryption_public),
        signing_public_key: encode_key("ed25519", &signing_public),
        fingerprint: device_fingerprint(&encryption_public, &signing_public),
        is_local: true,
        status: "active".into(),
        created_at: now.clone(),
        updated_at: now.clone(),
        revoked_at: None,
    };
    let result = async {
        let mut transaction = db.pool.begin().await?;
        sqlx::query("INSERT INTO team_devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,1,'active',?,?,NULL)")
            .bind(&device.id).bind(&device.workspace_id).bind(&device.member_id).bind(&device.name).bind(&device.encryption_public_key).bind(&device.signing_public_key).bind(&device.fingerprint).bind(&device.created_at).bind(&device.updated_at).execute(&mut *transaction).await?;
        sqlx::query("UPDATE team_workspaces SET local_device_id=?,updated_at=? WHERE id=? AND local_device_id IS NULL")
            .bind(&device.id).bind(&now).bind(workspace_id).execute(&mut *transaction).await?;
        team::audit(&mut transaction,workspace_id,&authorization.member_id,"device-created","device",&device.id).await?;
        transaction.commit().await?;
        Ok::<(), AppError>(())
    }
    .await;
    if let Err(error) = result {
        delete_private_keys(&device_id);
        return Err(error);
    }
    Ok(device)
}

pub async fn export_local_device(db: &Database, workspace_id: &str, path: &str) -> AppResult<()> {
    let authorization = team::authorize(db, workspace_id, "workspaceRead").await?;
    let device_id = authorization
        .local_device_id
        .ok_or_else(|| AppError::Unavailable("尚未创建本机团队设备身份".into()))?;
    let device = sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE id=? AND workspace_id=? AND status='active'")
        .bind(device_id).bind(workspace_id).fetch_optional(&db.pool).await?
        .ok_or_else(|| AppError::NotFound("本机团队设备".into()))?;
    validated_device_keys(&device)?;
    let payload = serde_json::to_vec_pretty(&DeviceBundle {
        schema_version: 1,
        workspace_id: workspace_id.into(),
        member_id: device.member_id,
        device_id: device.id,
        name: device.name,
        encryption_public_key: device.encryption_public_key,
        signing_public_key: device.signing_public_key,
        fingerprint: device.fingerprint,
    })
    .map_err(|error| AppError::Internal(error.to_string()))?;
    atomic_write(path, "cnshelldevice", payload).await
}

pub async fn import_device(db: &Database, workspace_id: &str, path: &str) -> AppResult<TeamDevice> {
    let authorization = team::authorize(db, workspace_id, "shareManage").await?;
    let path = validate_input_file(path, "cnshelldevice", MAX_DEVICE_BUNDLE_BYTES)?;
    let bundle: DeviceBundle = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|error| AppError::Validation(format!("设备公钥文件无效：{error}")))?;
    if bundle.schema_version != 1
        || bundle.workspace_id != workspace_id
        || uuid::Uuid::parse_str(&bundle.member_id).is_err()
        || uuid::Uuid::parse_str(&bundle.device_id).is_err()
    {
        return Err(AppError::Validation(
            "设备公钥工作区、成员或版本无效".into(),
        ));
    }
    let name = clean_name(&bundle.name, "设备名称")?;
    let encryption_bytes = decode_key(&bundle.encryption_public_key, "x25519")?;
    let signing_bytes = decode_key(&bundle.signing_public_key, "ed25519")?;
    VerifyingKey::from_bytes(&signing_bytes)
        .map_err(|_| AppError::Validation("设备 Ed25519 公钥无效".into()))?;
    let expected_fingerprint = device_fingerprint(&encryption_bytes, &signing_bytes);
    if bundle.fingerprint != expected_fingerprint {
        return Err(AppError::Validation("设备组合 SHA-256 指纹不一致".into()));
    }
    let member_role: String = sqlx::query_scalar(
        "SELECT role FROM team_members WHERE id=? AND workspace_id=? AND status='active'",
    )
    .bind(&bundle.member_id)
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::Validation("设备所属成员不存在或已移除".into()))?;
    if member_role == "owner" {
        team::require_permission(&authorization.role, "ownerManage")?;
    }
    let existing_workspace: Option<String> =
        sqlx::query_scalar("SELECT workspace_id FROM team_devices WHERE id=?")
            .bind(&bundle.device_id)
            .fetch_optional(&db.pool)
            .await?;
    if existing_workspace
        .as_deref()
        .is_some_and(|value| value != workspace_id)
    {
        return Err(AppError::Validation("设备 ID 已属于另一个工作区".into()));
    }
    let existing = sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE id=? AND workspace_id=?")
        .bind(&bundle.device_id).bind(workspace_id).fetch_optional(&db.pool).await?;
    if let Some(existing) = &existing
        && (existing.is_local
            || existing.member_id != bundle.member_id
            || existing.encryption_public_key != bundle.encryption_public_key
            || existing.signing_public_key != bundle.signing_public_key
            || existing.fingerprint != bundle.fingerprint
            || existing.status != "active")
    {
        return Err(AppError::Validation(
            "设备 ID 已是本机设备、密钥不一致或已撤销；必须使用新设备 ID".into(),
        ));
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM team_devices WHERE workspace_id=? AND status='active'",
    )
    .bind(workspace_id)
    .fetch_one(&db.pool)
    .await?;
    if existing.is_none() && count >= 512 {
        return Err(AppError::Validation("每个工作区最多 512 台活动设备".into()));
    }
    let now = Utc::now().to_rfc3339();
    let created_at = existing
        .as_ref()
        .map(|value| value.created_at.clone())
        .unwrap_or_else(|| now.clone());
    let device = TeamDevice {
        id: bundle.device_id,
        workspace_id: workspace_id.into(),
        member_id: bundle.member_id,
        name,
        encryption_public_key: bundle.encryption_public_key,
        signing_public_key: bundle.signing_public_key,
        fingerprint: bundle.fingerprint,
        is_local: false,
        status: "active".into(),
        created_at,
        updated_at: now.clone(),
        revoked_at: None,
    };
    let mut transaction = db.pool.begin().await?;
    sqlx::query("INSERT INTO team_devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,0,'active',?,?,NULL) ON CONFLICT(id) DO UPDATE SET name=excluded.name,updated_at=excluded.updated_at")
        .bind(&device.id).bind(&device.workspace_id).bind(&device.member_id).bind(&device.name).bind(&device.encryption_public_key).bind(&device.signing_public_key).bind(&device.fingerprint).bind(&device.created_at).bind(&device.updated_at).execute(&mut *transaction).await?;
    team::audit(
        &mut transaction,
        workspace_id,
        &authorization.member_id,
        "device-imported",
        "device",
        &device.id,
    )
    .await?;
    transaction.commit().await?;
    Ok(device)
}

pub async fn revoke_device(db: &Database, workspace_id: &str, device_id: &str) -> AppResult<()> {
    let authorization = team::authorize(db, workspace_id, "shareManage").await?;
    if uuid::Uuid::parse_str(device_id).is_err() {
        return Err(AppError::Validation("团队设备 ID 无效".into()));
    }
    let device = sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE id=? AND workspace_id=?")
        .bind(device_id).bind(workspace_id).fetch_optional(&db.pool).await?
        .ok_or_else(|| AppError::NotFound(format!("团队设备 {device_id}")))?;
    if device.status == "revoked" {
        return Ok(());
    }
    let member_role: String = sqlx::query_scalar(
        "SELECT role FROM team_members WHERE id=? AND workspace_id=? AND status='active'",
    )
    .bind(&device.member_id)
    .bind(workspace_id)
    .fetch_optional(&db.pool)
    .await?
    .ok_or_else(|| AppError::Validation("设备所属成员不存在或已移除".into()))?;
    if member_role == "owner" {
        team::require_permission(&authorization.role, "ownerManage")?;
    }
    let now = Utc::now().to_rfc3339();
    let mut transaction = db.pool.begin().await?;
    sqlx::query("UPDATE team_devices SET status='revoked',revoked_at=?,updated_at=? WHERE id=? AND workspace_id=?")
        .bind(&now).bind(&now).bind(device_id).bind(workspace_id).execute(&mut *transaction).await?;
    sqlx::query("UPDATE team_workspaces SET local_device_id=CASE WHEN local_device_id=? THEN NULL ELSE local_device_id END,key_epoch=key_epoch+1,updated_at=? WHERE id=?")
        .bind(device_id).bind(&now).bind(workspace_id).execute(&mut *transaction).await?;
    team::audit(
        &mut transaction,
        workspace_id,
        &authorization.member_id,
        "device-revoked",
        "device",
        device_id,
    )
    .await?;
    transaction.commit().await?;
    if device.is_local {
        delete_private_keys(device_id);
    }
    Ok(())
}

fn share_aad(workspace_id: &str, share_id: &str, epoch: i64) -> Vec<u8> {
    format!("cnshell-team-share-v1\0{workspace_id}\0{share_id}\0{epoch}").into_bytes()
}

fn wrap_key(
    shared_secret: &[u8; 32],
    share_id: &str,
    device_id: &str,
    content_key: &[u8; 32],
    nonce: &[u8; 12],
) -> AppResult<Vec<u8>> {
    let hkdf = Hkdf::<Sha256>::new(Some(share_id.as_bytes()), shared_secret);
    let mut wrapping_key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(
        format!("cnshell-team-wrap-v1\0{device_id}").as_bytes(),
        &mut *wrapping_key,
    )
    .map_err(|_| AppError::Internal("派生设备封装密钥失败".into()))?;
    Aes256Gcm::new_from_slice(&wrapping_key[..])
        .map_err(|_| AppError::Internal("初始化设备封装密钥失败".into()))?
        .encrypt(Nonce::from_slice(nonce), content_key.as_slice())
        .map_err(|_| AppError::Internal("封装内容密钥失败".into()))
}

fn unwrap_key(
    shared_secret: &[u8; 32],
    share_id: &str,
    device_id: &str,
    wrapped: &[u8],
    nonce: &[u8; 12],
) -> AppResult<[u8; 32]> {
    let hkdf = Hkdf::<Sha256>::new(Some(share_id.as_bytes()), shared_secret);
    let mut wrapping_key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(
        format!("cnshell-team-wrap-v1\0{device_id}").as_bytes(),
        &mut *wrapping_key,
    )
    .map_err(|_| AppError::Internal("派生设备封装密钥失败".into()))?;
    let plaintext = Aes256Gcm::new_from_slice(&wrapping_key[..])
        .map_err(|_| AppError::Internal("初始化设备封装密钥失败".into()))?
        .decrypt(Nonce::from_slice(nonce), wrapped)
        .map_err(|_| AppError::Authentication("当前设备无法解封内容密钥".into()));
    plaintext?
        .try_into()
        .map_err(|_| AppError::Validation("解封后的内容密钥长度无效".into()))
}

fn envelope_signing_payload(envelope: &ShareEnvelope) -> AppResult<Vec<u8>> {
    let mut unsigned = envelope.clone();
    unsigned.signature = None;
    serde_jcs::to_vec(&unsigned)
        .map_err(|error| AppError::Internal(format!("规范化分享签名载荷失败：{error}")))
}

fn shared_connection(profile: ConnectionProfile, credential: Option<String>) -> SharedConnection {
    SharedConnection {
        protocol: profile.protocol,
        name: profile.name,
        host: profile.host,
        port: profile.port,
        username: profile.username,
        auth_type: profile.auth_type,
        tags: profile.tags,
        encoding: profile.encoding,
        credential,
    }
}

async fn local_device(
    db: &Database,
    workspace_id: &str,
    authorization: &TeamAuthorization,
) -> AppResult<TeamDevice> {
    let device_id = authorization
        .local_device_id
        .as_deref()
        .ok_or_else(|| AppError::Unavailable("尚未创建本机团队设备身份".into()))?;
    let device = sqlx::query_as::<_, TeamDevice>("SELECT id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at FROM team_devices WHERE id=? AND workspace_id=? AND member_id=? AND is_local=1 AND status='active'")
        .bind(device_id).bind(workspace_id).bind(&authorization.member_id).fetch_optional(&db.pool).await?
        .ok_or_else(|| AppError::Unavailable("本机团队设备已撤销或引用无效".into()))?;
    validated_device_keys(&device)?;
    Ok(device)
}

pub async fn export_share(db: &Database, input: TeamShareExportInput) -> AppResult<()> {
    if input.recipient_device_ids.is_empty() || input.recipient_device_ids.len() > MAX_RECIPIENTS {
        return Err(AppError::Validation(
            "安全分享接收设备必须为 1 至 64 台".into(),
        ));
    }
    let mut unique = HashSet::new();
    if input
        .recipient_device_ids
        .iter()
        .any(|id| uuid::Uuid::parse_str(id).is_err() || !unique.insert(id.clone()))
    {
        return Err(AppError::Validation(
            "安全分享接收设备 ID 无效或重复".into(),
        ));
    }
    let authorization = team::authorize(db, &input.workspace_id, "shareCreate").await?;
    if input.include_credential {
        team::require_permission(&authorization.role, "shareManage")?;
    }
    let sender_device = local_device(db, &input.workspace_id, &authorization).await?;
    let profile = db.get_connection(&input.connection_id).await?;
    if !matches!(profile.protocol.as_str(), "ssh" | "rdp")
        || profile.proxy_id.is_some()
        || profile.private_key_path.is_some()
        || profile.certificate_path.is_some()
        || !matches!(
            profile.auth_type.as_str(),
            "none" | "password" | "sshAgent" | "fido2Agent"
        )
    {
        return Err(AppError::Validation(
            "安全分享首版只支持无代理、无私钥文件的 SSH/RDP 连接".into(),
        ));
    }
    let credential = if input.include_credential {
        ssh::load_credential(&profile.id)?
            .ok_or_else(|| {
                AppError::Unavailable(format!(
                    "该连接在{}中没有可分享的凭据",
                    crate::platform::credential_store_name()
                ))
            })?
            .into()
    } else {
        None
    };
    let connection = shared_connection(profile, credential);
    let mut plaintext =
        serde_json::to_vec(&connection).map_err(|error| AppError::Internal(error.to_string()))?;
    if plaintext.len() > MAX_SHARE_PLAINTEXT_BYTES {
        return Err(AppError::Validation("安全分享明文超过 1 MB".into()));
    }
    let mut devices = Vec::with_capacity(input.recipient_device_ids.len());
    for device_id in &input.recipient_device_ids {
        let device = sqlx::query_as::<_, TeamDevice>("SELECT d.id,d.workspace_id,d.member_id,d.name,d.encryption_public_key,d.signing_public_key,d.fingerprint,d.is_local,d.status,d.created_at,d.updated_at,d.revoked_at FROM team_devices d JOIN team_members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.status='active' AND m.status='active'")
            .bind(device_id).bind(&input.workspace_id).fetch_optional(&db.pool).await?
            .ok_or_else(|| AppError::Validation(format!("接收设备 {device_id} 不存在、已撤销或成员已移除")))?;
        devices.push(device);
    }
    let share_id = uuid::Uuid::new_v4().to_string();
    let aad = share_aad(&input.workspace_id, &share_id, authorization.key_epoch);
    let mut content_key = Zeroizing::new([0_u8; 32]);
    let mut payload_nonce = [0_u8; 12];
    let mut ephemeral_secret_bytes = Zeroizing::new([0_u8; 32]);
    OsRng.fill_bytes(&mut *content_key);
    OsRng.fill_bytes(&mut payload_nonce);
    OsRng.fill_bytes(&mut *ephemeral_secret_bytes);
    let ephemeral_secret = StaticSecret::from(*ephemeral_secret_bytes);
    let ephemeral_public = X25519PublicKey::from(&ephemeral_secret).to_bytes();
    let ciphertext = Aes256Gcm::new_from_slice(&content_key[..])
        .map_err(|_| AppError::Internal("初始化分享内容密钥失败".into()))?
        .encrypt(
            Nonce::from_slice(&payload_nonce),
            Payload {
                msg: &plaintext,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::Internal("加密分享内容失败".into()))?;
    plaintext.zeroize();
    let mut recipients = Vec::with_capacity(devices.len());
    for device in devices {
        let (encryption_public, _) = validated_device_keys(&device)?;
        let public = X25519PublicKey::from(encryption_public);
        let shared = ephemeral_secret.diffie_hellman(&public);
        if !shared.was_contributory() {
            return Err(AppError::Validation("接收设备 X25519 公钥不可用".into()));
        }
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut nonce);
        let wrapped_key = wrap_key(
            shared.as_bytes(),
            &share_id,
            &device.id,
            &content_key,
            &nonce,
        )?;
        recipients.push(ShareRecipient {
            device_id: device.id,
            member_id: device.member_id,
            nonce: URL_SAFE_NO_PAD.encode(nonce),
            wrapped_key: URL_SAFE_NO_PAD.encode(wrapped_key),
        });
    }
    let mut envelope = ShareEnvelope {
        schema_version: 1,
        share_id,
        workspace_id: input.workspace_id.clone(),
        key_epoch: authorization.key_epoch,
        sender_member_id: authorization.member_id.clone(),
        sender_device_id: sender_device.id.clone(),
        ephemeral_public_key: encode_key("x25519", &ephemeral_public),
        payload_nonce: URL_SAFE_NO_PAD.encode(payload_nonce),
        ciphertext: URL_SAFE_NO_PAD.encode(ciphertext),
        recipients,
        signature: None,
    };
    let signing_secret = Zeroizing::new(load_private_key(&sender_device.id, "ed25519")?);
    let signing_key = SigningKey::from_bytes(&signing_secret);
    let signature = signing_key.sign(&envelope_signing_payload(&envelope)?);
    envelope.signature = Some(format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(signature.to_bytes())
    ));
    let payload = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    if payload.len() as u64 > MAX_SHARE_FILE_BYTES {
        return Err(AppError::Validation("安全分享文件超过 4 MB".into()));
    }
    let final_authorization = team::authorize(db, &input.workspace_id, "shareCreate").await?;
    if input.include_credential {
        team::require_permission(&final_authorization.role, "shareManage")?;
    }
    if final_authorization.member_id != authorization.member_id
        || final_authorization.key_epoch != authorization.key_epoch
        || final_authorization.local_device_id.as_deref() != Some(sender_device.id.as_str())
    {
        return Err(AppError::PermissionDenied(
            "分享生成期间成员、设备或密钥 epoch 已变化，请重新操作".into(),
        ));
    }
    for recipient in &envelope.recipients {
        let still_active: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM team_devices d JOIN team_members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.member_id=? AND d.status='active' AND m.status='active'")
            .bind(&recipient.device_id)
            .bind(&input.workspace_id)
            .bind(&recipient.member_id)
            .fetch_one(&db.pool)
            .await?;
        if still_active == 0 {
            return Err(AppError::PermissionDenied(
                "分享生成期间接收设备或成员已被撤销，请重新选择".into(),
            ));
        }
    }
    atomic_write(&input.output_path, "cnshellshare", payload).await?;
    let mut transaction = match db.pool.begin().await {
        Ok(transaction) => transaction,
        Err(error) => {
            let _ = std::fs::remove_file(&input.output_path);
            return Err(error.into());
        }
    };
    if let Err(error) = team::audit(
        &mut transaction,
        &input.workspace_id,
        &authorization.member_id,
        "connection-shared",
        "connection",
        &input.connection_id,
    )
    .await
    {
        let _ = transaction.rollback().await;
        let _ = std::fs::remove_file(&input.output_path);
        return Err(error);
    }
    if let Err(error) = transaction.commit().await {
        let _ = std::fs::remove_file(&input.output_path);
        return Err(error.into());
    }
    Ok(())
}

fn decode_nonce(value: &str) -> AppResult<[u8; 12]> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::Validation("分享 nonce Base64URL 无效".into()))?
        .try_into()
        .map_err(|_| AppError::Validation("分享 nonce 必须为 12 字节".into()))
}

pub async fn preview_share(
    db: &Database,
    manager: &TeamShareManager,
    path: &str,
) -> AppResult<TeamSharePreview> {
    let path = validate_input_file(path, "cnshellshare", MAX_SHARE_FILE_BYTES)?;
    let envelope: ShareEnvelope = serde_json::from_slice(&std::fs::read(path)?)
        .map_err(|error| AppError::Validation(format!("安全分享文件无效：{error}")))?;
    if envelope.schema_version != 1
        || uuid::Uuid::parse_str(&envelope.share_id).is_err()
        || uuid::Uuid::parse_str(&envelope.workspace_id).is_err()
        || envelope.key_epoch < 1
        || envelope.recipients.is_empty()
        || envelope.recipients.len() > MAX_RECIPIENTS
    {
        return Err(AppError::Validation(
            "安全分享版本、ID、epoch 或接收者无效".into(),
        ));
    }
    let authorization = team::authorize(db, &envelope.workspace_id, "connectionManage").await?;
    if envelope.key_epoch > authorization.key_epoch {
        return Err(AppError::Validation(
            "安全分享来自未知的未来密钥 epoch".into(),
        ));
    }
    let local_device = local_device(db, &envelope.workspace_id, &authorization).await?;
    let recipient = envelope
        .recipients
        .iter()
        .find(|recipient| {
            recipient.device_id == local_device.id && recipient.member_id == authorization.member_id
        })
        .ok_or_else(|| AppError::PermissionDenied("该分享未加密给当前设备".into()))?;
    let sender = sqlx::query_as::<_, TeamDevice>("SELECT d.id,d.workspace_id,d.member_id,d.name,d.encryption_public_key,d.signing_public_key,d.fingerprint,d.is_local,d.status,d.created_at,d.updated_at,d.revoked_at FROM team_devices d JOIN team_members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.member_id=? AND d.status='active' AND m.status='active'")
        .bind(&envelope.sender_device_id).bind(&envelope.workspace_id).bind(&envelope.sender_member_id).fetch_optional(&db.pool).await?
        .ok_or_else(|| AppError::Authentication("发送设备不存在、已撤销或成员已移除".into()))?;
    let (_, signing_bytes) = validated_device_keys(&sender)?;
    let verifying_key = VerifyingKey::from_bytes(&signing_bytes)
        .map_err(|_| AppError::Validation("发送设备签名公钥无效".into()))?;
    let signature_value = envelope
        .signature
        .as_deref()
        .and_then(|value| value.strip_prefix("ed25519:"))
        .ok_or_else(|| AppError::Validation("安全分享缺少 Ed25519 签名".into()))?;
    let signature_bytes = URL_SAFE_NO_PAD
        .decode(signature_value)
        .map_err(|_| AppError::Validation("安全分享签名 Base64URL 无效".into()))?;
    let signature = Signature::from_slice(&signature_bytes)
        .map_err(|_| AppError::Validation("安全分享签名长度无效".into()))?;
    verifying_key
        .verify(&envelope_signing_payload(&envelope)?, &signature)
        .map_err(|_| AppError::Authentication("安全分享签名验证失败".into()))?;
    let ephemeral_public =
        X25519PublicKey::from(decode_key(&envelope.ephemeral_public_key, "x25519")?);
    let local_secret_bytes = Zeroizing::new(load_private_key(&local_device.id, "x25519")?);
    let local_secret = StaticSecret::from(*local_secret_bytes);
    let shared = local_secret.diffie_hellman(&ephemeral_public);
    if !shared.was_contributory() {
        return Err(AppError::Authentication(
            "安全分享 X25519 共享密钥无效".into(),
        ));
    }
    let wrapped = URL_SAFE_NO_PAD
        .decode(&recipient.wrapped_key)
        .map_err(|_| AppError::Validation("封装内容密钥 Base64URL 无效".into()))?;
    let wrap_nonce = decode_nonce(&recipient.nonce)?;
    let content_key = Zeroizing::new(unwrap_key(
        shared.as_bytes(),
        &envelope.share_id,
        &local_device.id,
        &wrapped,
        &wrap_nonce,
    )?);
    let payload_nonce = decode_nonce(&envelope.payload_nonce)?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&envelope.ciphertext)
        .map_err(|_| AppError::Validation("分享密文 Base64URL 无效".into()))?;
    if ciphertext.len() > MAX_SHARE_PLAINTEXT_BYTES + 16 {
        return Err(AppError::Validation("分享密文超过限制".into()));
    }
    let aad = share_aad(
        &envelope.workspace_id,
        &envelope.share_id,
        envelope.key_epoch,
    );
    let mut plaintext = Aes256Gcm::new_from_slice(&content_key[..])
        .map_err(|_| AppError::Internal("初始化分享内容密钥失败".into()))?
        .decrypt(
            Nonce::from_slice(&payload_nonce),
            Payload {
                msg: &ciphertext,
                aad: &aad,
            },
        )
        .map_err(|_| AppError::Authentication("安全分享密文验证失败".into()))?;
    let parsed = serde_json::from_slice(&plaintext)
        .map_err(|error| AppError::Validation(format!("分享连接载荷无效：{error}")));
    plaintext.zeroize();
    let connection: SharedConnection = parsed?;
    validate_shared_connection(&connection)?;
    Ok(manager.insert(
        envelope.workspace_id,
        envelope.sender_member_id,
        envelope.key_epoch,
        connection,
    ))
}

fn validate_shared_connection(connection: &SharedConnection) -> AppResult<()> {
    if !matches!(connection.protocol.as_str(), "ssh" | "rdp")
        || connection.name.trim().is_empty()
        || connection.name.len() > 256
        || connection.host.trim().is_empty()
        || connection.host.len() > 1024
        || connection.username.len() > 512
        || !(1..=65535).contains(&connection.port)
        || connection.tags.len() > 128
        || connection.tags.iter().any(|tag| tag.len() > 256)
        || connection.encoding.len() > 64
        || !matches!(
            connection.auth_type.as_str(),
            "none" | "password" | "sshAgent" | "fido2Agent"
        )
        || connection
            .credential
            .as_ref()
            .is_some_and(|value| value.len() > 16 * 1024)
    {
        return Err(AppError::Validation(
            "分享连接字段超过限制或枚举无效".into(),
        ));
    }
    Ok(())
}

pub async fn apply_share(
    db: &Database,
    manager: &TeamShareManager,
    request_id: &str,
) -> AppResult<ConnectionProfile> {
    let pending = manager.take(request_id)?;
    let authorization = team::authorize(db, &pending.workspace_id, "connectionManage").await?;
    if pending.key_epoch > authorization.key_epoch {
        return Err(AppError::Validation("安全分享 epoch 已失效".into()));
    }
    let mut input = SaveConnectionInput {
        id: uuid::Uuid::new_v4().to_string(),
        folder_id: None,
        protocol: pending.connection.protocol.clone(),
        name: pending.connection.name.clone(),
        host: pending.connection.host.clone(),
        port: pending.connection.port,
        username: pending.connection.username.clone(),
        auth_type: pending.connection.auth_type.clone(),
        private_key_path: None,
        certificate_path: None,
        host_key_policy: "strict".into(),
        note: String::new(),
        tags: pending.connection.tags.clone(),
        encoding: pending.connection.encoding.clone(),
        startup_command: None,
        proxy_id: None,
        environment: Default::default(),
        credential: pending.connection.credential.clone(),
    };
    let result = async {
        db::validate_connection(&input)?;
        let credential_ref = if let Some(credential) = input.credential.as_deref() {
            Some(ssh::save_credential(&input.id, credential)?)
        } else {
            None
        };
        let profile = match db
            .insert_connection(&input, credential_ref.as_deref())
            .await
        {
            Ok(profile) => profile,
            Err(error) => {
                if credential_ref.is_some() {
                    let _ = ssh::delete_credential(&input.id);
                }
                return Err(error);
            }
        };
        let mut transaction = match db.pool.begin().await {
            Ok(transaction) => transaction,
            Err(error) => {
                let _ = db.remove_inserted_connection(&input.id).await;
                let _ = ssh::delete_credential(&input.id);
                return Err(error.into());
            }
        };
        if let Err(error) = team::audit(
            &mut transaction,
            &pending.workspace_id,
            &authorization.member_id,
            "connection-share-imported",
            "connection",
            &profile.id,
        )
        .await
        {
            let _ = transaction.rollback().await;
            let _ = db.remove_inserted_connection(&input.id).await;
            let _ = ssh::delete_credential(&input.id);
            return Err(error);
        }
        if let Err(error) = transaction.commit().await {
            let _ = db.remove_inserted_connection(&input.id).await;
            let _ = ssh::delete_credential(&input.id);
            return Err(error.into());
        }
        Ok(profile)
    }
    .await;
    if let Some(credential) = input.credential.as_mut() {
        credential.zeroize();
    }
    let _ = pending.sender_member_id;
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::models::CreateTeamWorkspaceInput;

    #[test]
    fn x25519_wrapping_is_recipient_specific_and_authenticated() {
        let sender = StaticSecret::from([3_u8; 32]);
        let recipient = StaticSecret::from([5_u8; 32]);
        let recipient_public = X25519PublicKey::from(&recipient);
        let sender_public = X25519PublicKey::from(&sender);
        let shared_sender = sender.diffie_hellman(&recipient_public);
        let shared_recipient = recipient.diffie_hellman(&sender_public);
        assert_eq!(shared_sender.as_bytes(), shared_recipient.as_bytes());
        let key = [9_u8; 32];
        let nonce = [7_u8; 12];
        let wrapped = wrap_key(
            shared_sender.as_bytes(),
            "share-id",
            "device-a",
            &key,
            &nonce,
        )
        .unwrap();
        assert_eq!(
            unwrap_key(
                shared_recipient.as_bytes(),
                "share-id",
                "device-a",
                &wrapped,
                &nonce,
            )
            .unwrap(),
            key
        );
        assert!(
            unwrap_key(
                shared_recipient.as_bytes(),
                "share-id",
                "device-b",
                &wrapped,
                &nonce,
            )
            .is_err()
        );
    }

    #[test]
    fn envelope_signature_covers_recipients_ciphertext_and_epoch() {
        let signing = SigningKey::from_bytes(&[11_u8; 32]);
        let mut envelope = ShareEnvelope {
            schema_version: 1,
            share_id: uuid::Uuid::new_v4().to_string(),
            workspace_id: uuid::Uuid::new_v4().to_string(),
            key_epoch: 2,
            sender_member_id: uuid::Uuid::new_v4().to_string(),
            sender_device_id: uuid::Uuid::new_v4().to_string(),
            ephemeral_public_key: encode_key("x25519", &[1_u8; 32]),
            payload_nonce: URL_SAFE_NO_PAD.encode([2_u8; 12]),
            ciphertext: URL_SAFE_NO_PAD.encode([3_u8; 32]),
            recipients: vec![ShareRecipient {
                device_id: uuid::Uuid::new_v4().to_string(),
                member_id: uuid::Uuid::new_v4().to_string(),
                nonce: URL_SAFE_NO_PAD.encode([4_u8; 12]),
                wrapped_key: URL_SAFE_NO_PAD.encode([5_u8; 48]),
            }],
            signature: None,
        };
        let signature = signing.sign(&envelope_signing_payload(&envelope).unwrap());
        envelope.key_epoch += 1;
        assert!(
            signing
                .verifying_key()
                .verify(&envelope_signing_payload(&envelope).unwrap(), &signature)
                .is_err()
        );
    }

    #[test]
    fn shared_connection_rejects_private_key_and_unbounded_fields_by_shape() {
        let mut connection = SharedConnection {
            protocol: "ssh".into(),
            name: "Server".into(),
            host: "example.com".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            credential: Some("secret".into()),
        };
        assert!(validate_shared_connection(&connection).is_ok());
        connection.auth_type = "privateKey".into();
        assert!(validate_shared_connection(&connection).is_err());
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn signed_share_round_trip_uses_keychain_device_keys_and_rejects_tampering() {
        struct KeyCleanup(String);
        impl Drop for KeyCleanup {
            fn drop(&mut self) {
                delete_private_keys(&self.0);
            }
        }
        struct CredentialCleanup(Vec<String>);
        impl Drop for CredentialCleanup {
            fn drop(&mut self) {
                for id in &self.0 {
                    let _ = ssh::delete_credential(id);
                }
            }
        }

        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let workspace = team::create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Share Test".into(),
                owner_name: "Owner".into(),
            },
        )
        .await
        .unwrap();
        let device = ensure_local_device(&db, &workspace.id, "Test Mac")
            .await
            .unwrap();
        let _key_cleanup = KeyCleanup(device.id.clone());
        let source_id = uuid::Uuid::new_v4().to_string();
        ssh::save_credential(&source_id, "share-secret").unwrap();
        let mut credential_cleanup = CredentialCleanup(vec![source_id.clone()]);
        let source = SaveConnectionInput {
            id: source_id.clone(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Shared Server".into(),
            host: "example.com".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "must not be shared".into(),
            tags: vec!["prod".into()],
            encoding: "UTF-8".into(),
            startup_command: Some("echo must-not-be-shared".into()),
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.insert_connection(&source, Some(&ssh::credential_ref(&source_id)))
            .await
            .unwrap();
        let share_path = directory.path().join("connection.cnshellshare");
        export_share(
            &db,
            TeamShareExportInput {
                workspace_id: workspace.id.clone(),
                connection_id: source_id,
                recipient_device_ids: vec![device.id.clone()],
                include_credential: true,
                output_path: share_path.to_string_lossy().into_owned(),
            },
        )
        .await
        .unwrap();
        let original_epoch = workspace.key_epoch;
        sqlx::query("UPDATE team_workspaces SET key_epoch=key_epoch+1,updated_at=? WHERE id=?")
            .bind(Utc::now().to_rfc3339())
            .bind(&workspace.id)
            .execute(&db.pool)
            .await
            .unwrap();
        let manager = TeamShareManager::default();
        let preview = preview_share(&db, &manager, share_path.to_str().unwrap())
            .await
            .unwrap();
        assert_eq!(preview.connection_name, "Shared Server");
        assert_eq!(preview.key_epoch, original_epoch);
        assert!(preview.has_credential);
        let imported = apply_share(&db, &manager, &preview.request_id)
            .await
            .unwrap();
        credential_cleanup.0.push(imported.id.clone());
        assert_eq!(imported.host, "example.com");
        assert!(imported.note.is_empty());
        assert!(imported.startup_command.is_none());
        assert_eq!(
            ssh::load_credential(&imported.id).unwrap().as_deref(),
            Some("share-secret")
        );

        let mut envelope: ShareEnvelope =
            serde_json::from_slice(&std::fs::read(&share_path).unwrap()).unwrap();
        envelope.ciphertext.push('A');
        let tampered = directory.path().join("tampered.cnshellshare");
        std::fs::write(&tampered, serde_json::to_vec(&envelope).unwrap()).unwrap();
        assert!(
            preview_share(&db, &manager, tampered.to_str().unwrap())
                .await
                .is_err()
        );
    }
}
