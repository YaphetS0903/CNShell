use crate::{
    backup,
    db::Database,
    error::{AppError, AppResult},
    models::{SaveWebDavProfileInput, SyncOptions, SyncResult, WebDavProfile, WebDavSyncProgress},
};
use chrono::Utc;
use reqwest::{Client, StatusCode, Url, redirect::Policy};
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use zeroize::Zeroizing;

const PROFILES_KEY: &str = "cnshell.sync.webdav.profiles";
const KEYCHAIN_SERVICE: &str = "cn.cnshell.webdav";
const MAX_REMOTE_BYTES: usize = 50 * 1024 * 1024;

struct RemoteObject {
    bytes: Vec<u8>,
    etag: Option<String>,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct StoredWebDavProfile {
    id: String,
    name: String,
    url: String,
    username: String,
    #[serde(default)]
    sync_on_startup: bool,
    #[serde(default = "default_sync_options")]
    sync_options: SyncOptions,
}

fn default_sync_options() -> SyncOptions {
    SyncOptions {
        include_hosts: true,
        include_private_key_paths: false,
        include_credentials: false,
    }
}

pub async fn profiles(db: &Database) -> AppResult<Vec<WebDavProfile>> {
    let stored: Vec<StoredWebDavProfile> =
        db.load_named_state(PROFILES_KEY).await?.unwrap_or_default();
    Ok(stored
        .into_iter()
        .map(|profile| WebDavProfile {
            has_credential: load_password(&profile.id).ok().flatten().is_some(),
            has_sync_passphrase: load_sync_passphrase(&profile.id).ok().flatten().is_some(),
            sync_on_startup: profile.sync_on_startup,
            sync_options: profile.sync_options.clone(),
            id: profile.id,
            name: profile.name,
            url: profile.url,
            username: profile.username,
        })
        .collect())
}

pub async fn save_profile(
    db: &Database,
    input: SaveWebDavProfileInput,
) -> AppResult<WebDavProfile> {
    validate_id_name(&input.id, &input.name)?;
    let url = validate_base_url(&input.url)?.to_string();
    if input.username.trim().is_empty() || input.username.len() > 512 {
        return Err(AppError::Validation(
            "WebDAV 用户名不能为空且不能超过 512 字符".into(),
        ));
    }
    if let Some(password) = input.password.as_deref() {
        if password.is_empty() || password.len() > 4096 {
            return Err(AppError::Validation(
                "WebDAV 密码不能为空且不能超过 4096 字符".into(),
            ));
        }
        save_password(&input.id, password)?;
    }
    if let Some(sync_passphrase) = input.sync_passphrase.as_deref() {
        if sync_passphrase.is_empty() {
            delete_sync_passphrase(&input.id)?;
        } else if sync_passphrase.len() < 8 || sync_passphrase.len() > 4096 {
            return Err(AppError::Validation(
                "启动同步口令必须为 8～4096 字符".into(),
            ));
        } else {
            save_sync_passphrase(&input.id, sync_passphrase)?;
        }
    }
    let profile = StoredWebDavProfile {
        id: input.id,
        name: input.name.trim().into(),
        url,
        username: input.username,
        sync_on_startup: input.sync_on_startup,
        sync_options: input.sync_options,
    };
    let mut profiles: Vec<StoredWebDavProfile> =
        db.load_named_state(PROFILES_KEY).await?.unwrap_or_default();
    if let Some(existing) = profiles.iter_mut().find(|item| item.id == profile.id) {
        *existing = profile.clone();
    } else {
        profiles.push(profile.clone());
    }
    db.save_named_state(
        PROFILES_KEY,
        &serde_json::to_value(&profiles).map_err(|error| AppError::Internal(error.to_string()))?,
    )
    .await?;
    Ok(WebDavProfile {
        has_credential: load_password(&profile.id)?.is_some(),
        has_sync_passphrase: load_sync_passphrase(&profile.id)?.is_some(),
        sync_on_startup: profile.sync_on_startup,
        sync_options: profile.sync_options,
        id: profile.id,
        name: profile.name,
        url: profile.url,
        username: profile.username,
    })
}

pub async fn delete_profile(db: &Database, id: &str) -> AppResult<()> {
    validate_id_name(id, "profile")?;
    let mut profiles: Vec<StoredWebDavProfile> =
        db.load_named_state(PROFILES_KEY).await?.unwrap_or_default();
    profiles.retain(|item| item.id != id);
    db.save_named_state(
        PROFILES_KEY,
        &serde_json::to_value(&profiles).map_err(|error| AppError::Internal(error.to_string()))?,
    )
    .await?;
    delete_password(id)?;
    delete_sync_passphrase(id)
}

pub fn start_startup_sync(app: tauri::AppHandle, db: Database, tasks: crate::task::TaskManager) {
    tauri::async_runtime::spawn(async move {
        let profiles: Vec<StoredWebDavProfile> = match db.load_named_state(PROFILES_KEY).await {
            Ok(Some(value)) => value,
            _ => return,
        };
        for profile in profiles
            .into_iter()
            .filter(|profile| profile.sync_on_startup)
        {
            let Some(passphrase) = load_sync_passphrase(&profile.id).ok().flatten() else {
                continue;
            };
            let db_for_run = db.clone();
            let profile_id = profile.id.clone();
            let progress_app = app.clone();
            tasks.spawn(
                app.clone(),
                "webdav-startup-sync",
                move |cancelled| async move {
                    let passphrase = Zeroizing::new(passphrase);
                    serde_json::to_value(
                        read(
                            &progress_app,
                            &db_for_run,
                            &profile_id,
                            passphrase.as_str(),
                            cancelled,
                        )
                        .await?,
                    )
                    .map_err(|error| AppError::Internal(error.to_string()))
                },
            );
        }
    });
}

pub async fn write(
    app: &tauri::AppHandle,
    db: &Database,
    profile_id: &str,
    passphrase: &str,
    options: &SyncOptions,
    cancelled: Arc<AtomicBool>,
) -> AppResult<SyncResult> {
    let (profile, password) = profile_with_password(db, profile_id).await?;
    ensure_not_cancelled(&cancelled)?;
    emit_progress(app, profile_id, "encrypting", 0, 0);
    let directory = tempfile::tempdir()?;
    let local = backup::sync_write(
        db,
        directory.path().to_string_lossy().as_ref(),
        passphrase,
        options,
    )
    .await?;
    let payload = std::fs::read(directory.path().join("CNshell-sync.cnshell.json"))?;
    let payload_size = payload.len() as u64;
    emit_progress(app, profile_id, "uploading", 0, payload_size);
    let client = client()?;
    let target = target_url(&profile.url, "CNshell-sync.cnshell.json")?;
    let existing = get_optional(&client, &target, &profile.username, &password, &cancelled).await?;
    ensure_not_cancelled(&cancelled)?;
    let (conflict_copy, target_etag) = if let Some(existing) = existing {
        let etag = existing.etag.clone().ok_or_else(|| {
            AppError::Unavailable(
                "WebDAV 服务器未提供 ETag，CNshell 拒绝覆盖可能已被其他设备修改的同步包".into(),
            )
        })?;
        let name = format!(
            "CNshell-sync.conflict-{}-{}.cnshell.json",
            Utc::now().format("%Y%m%d-%H%M%S"),
            &uuid::Uuid::new_v4().to_string()[..8]
        );
        let conflict = target_url(&profile.url, &name)?;
        put(
            &client,
            &conflict,
            &profile.username,
            &password,
            existing.bytes,
            None,
            true,
            &cancelled,
        )
        .await?;
        (Some(conflict.to_string()), Some(etag))
    } else {
        (None, None)
    };
    ensure_not_cancelled(&cancelled)?;
    put(
        &client,
        &target,
        &profile.username,
        &password,
        payload,
        target_etag.as_deref(),
        target_etag.is_none(),
        &cancelled,
    )
    .await?;
    emit_progress(app, profile_id, "completed", payload_size, payload_size);
    Ok(SyncResult {
        path: target.to_string(),
        conflict_copy,
        connection_count: local.connection_count,
        encrypted: true,
    })
}

pub async fn read(
    app: &tauri::AppHandle,
    db: &Database,
    profile_id: &str,
    passphrase: &str,
    cancelled: Arc<AtomicBool>,
) -> AppResult<SyncResult> {
    let (profile, password) = profile_with_password(db, profile_id).await?;
    ensure_not_cancelled(&cancelled)?;
    emit_progress(app, profile_id, "downloading", 0, 0);
    let client = client()?;
    let target = target_url(&profile.url, "CNshell-sync.cnshell.json")?;
    let payload = get_optional(&client, &target, &profile.username, &password, &cancelled)
        .await?
        .ok_or_else(|| AppError::NotFound("WebDAV 中没有 CNshell-sync.cnshell.json".into()))?
        .bytes;
    emit_progress(
        app,
        profile_id,
        "downloading",
        payload.len() as u64,
        payload.len() as u64,
    );
    ensure_not_cancelled(&cancelled)?;
    let directory = tempfile::tempdir()?;
    std::fs::write(directory.path().join("CNshell-sync.cnshell.json"), &payload)?;
    let imported =
        backup::sync_read(db, directory.path().to_string_lossy().as_ref(), passphrase).await?;
    emit_progress(
        app,
        profile_id,
        "completed",
        payload.len() as u64,
        payload.len() as u64,
    );
    Ok(SyncResult {
        path: target.to_string(),
        conflict_copy: None,
        connection_count: imported.connection_count,
        encrypted: true,
    })
}

fn emit_progress(
    app: &tauri::AppHandle,
    profile_id: &str,
    phase: &str,
    transferred_bytes: u64,
    total_bytes: u64,
) {
    let _ = tauri::Emitter::emit(
        app,
        "webdav-sync-progress",
        WebDavSyncProgress {
            profile_id: profile_id.into(),
            phase: phase.into(),
            transferred_bytes,
            total_bytes,
        },
    );
}

async fn profile_with_password(
    db: &Database,
    id: &str,
) -> AppResult<(StoredWebDavProfile, Zeroizing<String>)> {
    let profiles: Vec<StoredWebDavProfile> =
        db.load_named_state(PROFILES_KEY).await?.unwrap_or_default();
    let profile = profiles
        .into_iter()
        .find(|item| item.id == id)
        .ok_or_else(|| AppError::NotFound(format!("WebDAV 配置 {id}")))?;
    let password = load_password(id)?
        .ok_or_else(|| AppError::Authentication("WebDAV 配置没有保存密码".into()))?;
    Ok((profile, Zeroizing::new(password)))
}

fn client() -> AppResult<Client> {
    Client::builder()
        .redirect(Policy::none())
        .connect_timeout(std::time::Duration::from_secs(10))
        .timeout(std::time::Duration::from_secs(60))
        .build()
        .map_err(|error| AppError::Unavailable(format!("WebDAV 客户端初始化失败：{error}")))
}

async fn get_optional(
    client: &Client,
    url: &Url,
    username: &str,
    password: &str,
    cancelled: &Arc<AtomicBool>,
) -> AppResult<Option<RemoteObject>> {
    let request = client.get(url.clone()).basic_auth(username, Some(password));
    let response = send_cancellable(request, cancelled).await?;
    if response.status() == StatusCode::NOT_FOUND {
        return Ok(None);
    }
    if !response.status().is_success() {
        return Err(http_error("读取", response.status()));
    }
    if response.content_length().unwrap_or(0) > MAX_REMOTE_BYTES as u64 {
        return Err(AppError::Validation("WebDAV 同步包不能超过 50 MB".into()));
    }
    let etag = response
        .headers()
        .get(reqwest::header::ETAG)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let bytes = response.bytes().await.map_err(network_error)?;
    if bytes.len() > MAX_REMOTE_BYTES {
        return Err(AppError::Validation("WebDAV 同步包不能超过 50 MB".into()));
    }
    Ok(Some(RemoteObject {
        bytes: bytes.to_vec(),
        etag,
    }))
}

async fn put(
    client: &Client,
    url: &Url,
    username: &str,
    password: &str,
    bytes: Vec<u8>,
    if_match: Option<&str>,
    create_only: bool,
    cancelled: &Arc<AtomicBool>,
) -> AppResult<()> {
    let mut request = client
        .put(url.clone())
        .basic_auth(username, Some(password))
        .header(reqwest::header::CONTENT_TYPE, "application/octet-stream")
        .body(bytes);
    if let Some(etag) = if_match {
        request = request.header(reqwest::header::IF_MATCH, etag);
    } else if create_only {
        request = request.header(reqwest::header::IF_NONE_MATCH, "*");
    }
    let response = send_cancellable(request, cancelled).await?;
    if response.status() == StatusCode::PRECONDITION_FAILED {
        return Err(AppError::Remote(
            "WebDAV 同步冲突：远端文件已被其他设备修改，本次未覆盖".into(),
        ));
    }
    if response.status().is_success() {
        Ok(())
    } else {
        Err(http_error("写入", response.status()))
    }
}

async fn send_cancellable(
    request: reqwest::RequestBuilder,
    cancelled: &Arc<AtomicBool>,
) -> AppResult<reqwest::Response> {
    tokio::select! {
        result = request.send() => result.map_err(network_error),
        _ = async {
            while !cancelled.load(Ordering::Acquire) {
                tokio::time::sleep(std::time::Duration::from_millis(50)).await;
            }
        } => Err(AppError::Unavailable("WebDAV 同步已取消".into())),
    }
}

fn validate_base_url(value: &str) -> AppResult<Url> {
    let mut url =
        Url::parse(value.trim()).map_err(|_| AppError::Validation("WebDAV 地址无效".into()))?;
    let loopback = matches!(url.host_str(), Some("localhost" | "127.0.0.1" | "::1"));
    if url.scheme() != "https" && !(url.scheme() == "http" && loopback) {
        return Err(AppError::Validation(
            "WebDAV 必须使用 HTTPS；仅本机测试允许 HTTP".into(),
        ));
    }
    if !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::Validation(
            "WebDAV 地址不能包含账号、密码、查询参数或片段".into(),
        ));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    Ok(url)
}

fn target_url(base: &str, file_name: &str) -> AppResult<Url> {
    if file_name.contains(['/', '\\', '\0']) {
        return Err(AppError::Validation("WebDAV 文件名无效".into()));
    }
    validate_base_url(base)?
        .join(file_name)
        .map_err(|_| AppError::Validation("WebDAV 目标地址无效".into()))
}

fn validate_id_name(id: &str, name: &str) -> AppResult<()> {
    if id.is_empty()
        || id.len() > 128
        || !id
            .bytes()
            .all(|value| value.is_ascii_alphanumeric() || value == b'-' || value == b'_')
        || name.trim().is_empty()
        || name.len() > 256
    {
        return Err(AppError::Validation("WebDAV 配置 ID 或名称无效".into()));
    }
    Ok(())
}

fn save_password(id: &str, password: &str) -> AppResult<()> {
    keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(password)
        .map_err(|error| AppError::Storage(format!("WebDAV 密码保存失败：{error}")))
}

fn sync_account(id: &str) -> String {
    format!("{id}:sync")
}

fn save_sync_passphrase(id: &str, passphrase: &str) -> AppResult<()> {
    keyring::Entry::new(KEYCHAIN_SERVICE, &sync_account(id))
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(passphrase)
        .map_err(|error| AppError::Storage(format!("启动同步口令保存失败：{error}")))
}

fn load_sync_passphrase(id: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &sync_account(id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!("启动同步口令读取失败：{error}"))),
    }
}

fn delete_sync_passphrase(id: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &sync_account(id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!("启动同步口令删除失败：{error}"))),
    }
}

fn load_password(id: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!("WebDAV 密码读取失败：{error}"))),
    }
}

fn delete_password(id: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, id)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!("WebDAV 密码删除失败：{error}"))),
    }
}

fn ensure_not_cancelled(cancelled: &Arc<AtomicBool>) -> AppResult<()> {
    if cancelled.load(Ordering::Acquire) {
        Err(AppError::Unavailable("WebDAV 同步已取消".into()))
    } else {
        Ok(())
    }
}

fn network_error(error: reqwest::Error) -> AppError {
    if error.is_timeout() {
        AppError::Unavailable("WebDAV 请求超时".into())
    } else {
        AppError::Unavailable(format!("WebDAV 网络请求失败：{error}"))
    }
}

fn http_error(operation: &str, status: StatusCode) -> AppError {
    match status {
        StatusCode::UNAUTHORIZED | StatusCode::FORBIDDEN => {
            AppError::Authentication(format!("WebDAV {operation}被拒绝（HTTP {status}）"))
        }
        _ => AppError::Remote(format!("WebDAV {operation}失败（HTTP {status}）")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn validates_https_and_builds_scoped_target_urls() {
        assert!(validate_base_url("https://dav.example.test/cnshell").is_ok());
        assert!(validate_base_url("http://dav.example.test/cnshell").is_err());
        assert!(validate_base_url("http://127.0.0.1:8080/cnshell").is_ok());
        assert_eq!(
            target_url(
                "https://dav.example.test/cnshell",
                "CNshell-sync.cnshell.json"
            )
            .unwrap()
            .as_str(),
            "https://dav.example.test/cnshell/CNshell-sync.cnshell.json"
        );
    }

    #[test]
    fn rejects_credentials_inside_urls_and_path_escape() {
        assert!(validate_base_url("https://user:secret@dav.example.test/").is_err());
        assert!(target_url("https://dav.example.test/", "../secret").is_err());
    }

    #[tokio::test]
    async fn local_webdav_round_trip_uses_basic_auth_and_binary_bytes() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (sender, mut receiver) = tokio::sync::mpsc::channel(2);
        tokio::spawn(async move {
            for index in 0..2 {
                let (mut socket, _) = listener.accept().await.unwrap();
                let mut request = Vec::new();
                let mut buffer = [0_u8; 1024];
                let header_end = loop {
                    let count = socket.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..count]);
                    if let Some(offset) = request.windows(4).position(|part| part == b"\r\n\r\n") {
                        break offset + 4;
                    }
                };
                let headers = String::from_utf8_lossy(&request[..header_end]);
                let content_length = headers
                    .lines()
                    .find_map(|line| {
                        line.to_ascii_lowercase()
                            .strip_prefix("content-length: ")
                            .and_then(|value| value.parse::<usize>().ok())
                    })
                    .unwrap_or(0);
                while request.len() < header_end + content_length {
                    let count = socket.read(&mut buffer).await.unwrap();
                    request.extend_from_slice(&buffer[..count]);
                }
                sender.send(request.clone()).await.unwrap();
                if index == 0 {
                    socket.write_all(b"HTTP/1.1 201 Created\r\nContent-Length: 0\r\nConnection: close\r\n\r\n").await.unwrap();
                } else {
                    socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 9\r\nConnection: close\r\n\r\nencrypted").await.unwrap();
                }
            }
        });
        let client = client().unwrap();
        let url = Url::parse(&format!("http://{address}/sync.bin")).unwrap();
        let cancelled = Arc::new(AtomicBool::new(false));
        put(
            &client,
            &url,
            "alice",
            "secret",
            b"ciphertext".to_vec(),
            None,
            true,
            &cancelled,
        )
        .await
        .unwrap();
        assert_eq!(
            get_optional(&client, &url, "alice", "secret", &cancelled)
                .await
                .unwrap()
                .unwrap()
                .bytes,
            b"encrypted"
        );
        let put_request = receiver.recv().await.unwrap();
        let get_request = receiver.recv().await.unwrap();
        let put_text = String::from_utf8_lossy(&put_request);
        let get_text = String::from_utf8_lossy(&get_request);
        assert!(put_text.starts_with("PUT /sync.bin HTTP/1.1"));
        assert!(
            put_text
                .to_ascii_lowercase()
                .contains("authorization: basic")
        );
        assert!(put_text.to_ascii_lowercase().contains("if-none-match: *"));
        assert!(put_request.ends_with(b"ciphertext"));
        assert!(get_text.starts_with("GET /sync.bin HTTP/1.1"));
    }
}
