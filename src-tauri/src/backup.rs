use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{ConnectionProfile, SaveConnectionInput, SyncOptions, SyncResult},
    ssh::{delete_credential, load_credential, save_credential},
};
use aes_gcm::{Aes256Gcm, KeyInit, Nonce, aead::Aead};
use argon2::Argon2;
use base64::{Engine, engine::general_purpose::STANDARD};
use chrono::Utc;
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ExportConnection {
    id: String,
    folder_id: Option<String>,
    protocol: String,
    name: String,
    host: String,
    port: i64,
    username: String,
    auth_type: String,
    private_key_path: Option<String>,
    #[serde(default)]
    certificate_path: Option<String>,
    host_key_policy: String,
    note: String,
    tags: Vec<String>,
    encoding: String,
    startup_command: Option<String>,
    proxy_id: Option<String>,
    #[serde(default)]
    environment: std::collections::BTreeMap<String, String>,
    credential: Option<String>,
}

impl ExportConnection {
    fn from_profile(profile: ConnectionProfile, include_secret: bool) -> AppResult<Self> {
        let credential =
            if include_secret && !matches!(profile.auth_type.as_str(), "sshAgent" | "fido2Agent") {
                load_credential(&profile.id)?
            } else {
                None
            };
        Ok(Self {
            id: profile.id,
            folder_id: profile.folder_id,
            protocol: profile.protocol,
            name: profile.name,
            host: profile.host,
            port: profile.port,
            username: profile.username,
            auth_type: profile.auth_type,
            private_key_path: profile.private_key_path,
            certificate_path: profile.certificate_path,
            host_key_policy: profile.host_key_policy,
            note: profile.note,
            tags: profile.tags,
            encoding: profile.encoding,
            startup_command: profile.startup_command,
            proxy_id: profile.proxy_id,
            environment: profile.environment,
            credential,
        })
    }

    fn into_input(self) -> SaveConnectionInput {
        SaveConnectionInput {
            id: self.id,
            folder_id: self.folder_id,
            protocol: self.protocol,
            name: self.name,
            host: self.host,
            port: self.port,
            username: self.username,
            auth_type: self.auth_type,
            private_key_path: self.private_key_path,
            certificate_path: self.certificate_path,
            host_key_policy: self.host_key_policy,
            note: self.note,
            tags: self.tags,
            encoding: self.encoding,
            startup_command: self.startup_command,
            proxy_id: self.proxy_id,
            environment: self.environment,
            credential: self.credential,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "format")]
enum BackupEnvelope {
    Plain {
        version: u32,
        connections: Vec<ExportConnection>,
    },
    Encrypted {
        version: u32,
        kdf: String,
        salt: String,
        nonce: String,
        ciphertext: String,
    },
}

fn derive_key(passphrase: &str, salt: &[u8]) -> AppResult<[u8; 32]> {
    let mut key = [0_u8; 32];
    Argon2::default()
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|error| AppError::Internal(format!("密钥派生失败：{error}")))?;
    Ok(key)
}

pub async fn export(
    db: &Database,
    path: &str,
    include_secrets: bool,
    passphrase: Option<&str>,
) -> AppResult<()> {
    export_profiles(db, path, include_secrets, passphrase, None).await
}

pub async fn export_one(db: &Database, id: &str, path: &str) -> AppResult<()> {
    export_profiles(db, path, false, None, Some(id)).await
}

pub async fn sync_write(
    db: &Database,
    folder: &str,
    passphrase: &str,
    options: &SyncOptions,
) -> AppResult<SyncResult> {
    if passphrase.len() < 8 {
        return Err(AppError::Validation("同步口令至少需要 8 位".into()));
    }
    let folder = Path::new(folder);
    if !folder.is_dir() {
        return Err(AppError::Validation(
            "同步位置必须是已存在的文件夹（可选择 iCloud Drive、WebDAV 或 Git 的本地挂载目录）"
                .into(),
        ));
    }
    let mut connections = Vec::new();
    if options.include_hosts {
        for profile in db.list_connections().await? {
            let mut exported =
                ExportConnection::from_profile(profile, options.include_credentials)?;
            if !options.include_private_key_paths {
                exported.private_key_path = None;
                exported.certificate_path = None;
            }
            connections.push(exported);
        }
    }
    let clear =
        serde_json::to_vec(&connections).map_err(|error| AppError::Internal(error.to_string()))?;
    let mut salt = [0_u8; 16];
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce);
    let key = derive_key(passphrase, &salt)?;
    let cipher =
        Aes256Gcm::new_from_slice(&key).map_err(|error| AppError::Internal(error.to_string()))?;
    let ciphertext = cipher
        .encrypt(Nonce::from_slice(&nonce), clear.as_ref())
        .map_err(|_| AppError::Internal("同步加密失败".into()))?;
    let envelope = BackupEnvelope::Encrypted {
        version: 1,
        kdf: "argon2id+aes-256-gcm".into(),
        salt: STANDARD.encode(salt),
        nonce: STANDARD.encode(nonce),
        ciphertext: STANDARD.encode(ciphertext),
    };
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let target = folder.join("CNshell-sync.cnshell.json");
    let conflict_copy = if target.exists() && std::fs::read(&target)? != bytes {
        let conflict = folder.join(format!(
            "CNshell-sync.conflict-{}.cnshell.json",
            Utc::now().format("%Y%m%d-%H%M%S")
        ));
        std::fs::copy(&target, &conflict)?;
        Some(conflict.to_string_lossy().into_owned())
    } else {
        None
    };
    let temporary = folder.join(format!(".CNshell-sync-{}.tmp", uuid::Uuid::new_v4()));
    {
        use std::io::Write;
        let mut file = std::fs::File::create(&temporary)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    std::fs::rename(&temporary, &target)?;
    Ok(SyncResult {
        path: target.to_string_lossy().into_owned(),
        conflict_copy,
        connection_count: connections.len(),
        encrypted: true,
    })
}

async fn export_profiles(
    db: &Database,
    path: &str,
    include_secrets: bool,
    passphrase: Option<&str>,
    only_id: Option<&str>,
) -> AppResult<()> {
    if include_secrets && passphrase.filter(|value| value.len() >= 8).is_none() {
        return Err(AppError::Validation(
            "包含凭据的导出必须设置至少 8 位口令".into(),
        ));
    }
    let mut connections = Vec::new();
    for profile in db
        .list_connections()
        .await?
        .into_iter()
        .filter(|profile| only_id.is_none() || only_id == Some(profile.id.as_str()))
    {
        connections.push(ExportConnection::from_profile(profile, include_secrets)?);
    }
    if only_id.is_some() && connections.is_empty() {
        return Err(AppError::NotFound(only_id.unwrap_or_default().into()));
    }
    let envelope = if include_secrets {
        let clear = serde_json::to_vec(&connections)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let mut salt = [0_u8; 16];
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut salt);
        OsRng.fill_bytes(&mut nonce);
        let key = derive_key(passphrase.unwrap_or_default(), &salt)?;
        let cipher = Aes256Gcm::new_from_slice(&key)
            .map_err(|error| AppError::Internal(error.to_string()))?;
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), clear.as_ref())
            .map_err(|_| AppError::Internal("加密导出失败".into()))?;
        BackupEnvelope::Encrypted {
            version: 1,
            kdf: "argon2id+aes-256-gcm".into(),
            salt: STANDARD.encode(salt),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        }
    } else {
        BackupEnvelope::Plain {
            version: 1,
            connections,
        }
    };
    let bytes = serde_json::to_vec_pretty(&envelope)
        .map_err(|error| AppError::Internal(error.to_string()))?;
    let target = Path::new(path);
    let temp = target.with_extension("cnshell.tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(temp, target)?;
    Ok(())
}

pub async fn import(db: &Database, path: &str, passphrase: Option<&str>) -> AppResult<usize> {
    import_file(db, path, passphrase, false).await
}

pub async fn sync_read(db: &Database, folder: &str, passphrase: &str) -> AppResult<SyncResult> {
    if passphrase.len() < 8 {
        return Err(AppError::Validation("同步口令至少需要 8 位".into()));
    }
    let path = Path::new(folder).join("CNshell-sync.cnshell.json");
    if !path.is_file() {
        return Err(AppError::NotFound(
            "同步目录中没有 CNshell-sync.cnshell.json".into(),
        ));
    }
    let count = import_file(
        db,
        path.to_str()
            .ok_or_else(|| AppError::Validation("同步路径编码无效".into()))?,
        Some(passphrase),
        true,
    )
    .await?;
    Ok(SyncResult {
        path: path.to_string_lossy().into_owned(),
        conflict_copy: None,
        connection_count: count,
        encrypted: true,
    })
}

async fn import_file(
    db: &Database,
    path: &str,
    passphrase: Option<&str>,
    preserve_conflicts: bool,
) -> AppResult<usize> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(AppError::Validation("备份路径必须是普通文件".into()));
    }
    if metadata.len() > 50 * 1024 * 1024 {
        return Err(AppError::Validation("备份文件不能超过 50 MB".into()));
    }
    let bytes = std::fs::read(path)?;
    let envelope: BackupEnvelope = serde_json::from_slice(&bytes)
        .map_err(|error| AppError::Validation(format!("备份文件无效：{error}")))?;
    let (connections, encrypted): (Vec<ExportConnection>, bool) = match envelope {
        BackupEnvelope::Plain {
            version: 1,
            connections,
        } => (connections, false),
        BackupEnvelope::Encrypted {
            version: 1,
            salt,
            nonce,
            ciphertext,
            ..
        } => {
            let passphrase = passphrase
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::Validation("该备份已加密，请输入导出口令".into()))?;
            let salt = STANDARD
                .decode(salt)
                .map_err(|_| AppError::Validation("备份盐值损坏".into()))?;
            let nonce = STANDARD
                .decode(nonce)
                .map_err(|_| AppError::Validation("备份 nonce 损坏".into()))?;
            if nonce.len() != 12 {
                return Err(AppError::Validation("备份 nonce 长度无效".into()));
            }
            let ciphertext = STANDARD
                .decode(ciphertext)
                .map_err(|_| AppError::Validation("备份密文损坏".into()))?;
            let key = derive_key(passphrase, &salt)?;
            let cipher = Aes256Gcm::new_from_slice(&key)
                .map_err(|error| AppError::Internal(error.to_string()))?;
            let clear = cipher
                .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
                .map_err(|_| AppError::Authentication("导出口令错误或备份已损坏".into()))?;
            (
                serde_json::from_slice(&clear)
                    .map_err(|_| AppError::Validation("备份内容损坏".into()))?,
                true,
            )
        }
        _ => return Err(AppError::Validation("不支持的备份版本".into())),
    };
    if !encrypted
        && connections
            .iter()
            .any(|connection| connection.credential.is_some())
    {
        return Err(AppError::Validation(
            "普通备份不得包含凭据；请使用 CNshell 加密备份格式".into(),
        ));
    }
    let mut inputs = Vec::with_capacity(connections.len());
    let existing_ids = if preserve_conflicts {
        db.list_connections()
            .await?
            .into_iter()
            .map(|item| item.id)
            .collect::<std::collections::HashSet<_>>()
    } else {
        Default::default()
    };
    for mut connection in connections {
        if existing_ids.contains(&connection.id) {
            connection.id = uuid::Uuid::new_v4().to_string();
            connection.name = format!(
                "{}（同步冲突副本 {}）",
                connection.name,
                Utc::now().format("%Y-%m-%d %H:%M")
            );
        }
        let mut input = ExportConnection {
            id: connection.id.clone(),
            folder_id: connection.folder_id.clone(),
            protocol: connection.protocol.clone(),
            name: connection.name.clone(),
            host: connection.host.clone(),
            port: connection.port,
            username: connection.username.clone(),
            auth_type: connection.auth_type.clone(),
            private_key_path: connection.private_key_path.clone(),
            certificate_path: connection.certificate_path.clone(),
            host_key_policy: connection.host_key_policy.clone(),
            note: connection.note.clone(),
            tags: connection.tags.clone(),
            encoding: connection.encoding.clone(),
            startup_command: connection.startup_command.clone(),
            proxy_id: connection.proxy_id.clone(),
            environment: connection.environment.clone(),
            credential: connection.credential.clone(),
        }
        .into_input();
        db.sanitize_import_references(&mut input).await?;
        crate::db::validate_connection(&input)?;
        inputs.push(input);
    }
    let count = inputs.len();
    let mut prepared = Vec::with_capacity(count);
    let mut changed = Vec::new();
    for input in inputs {
        let reference = if input.protocol == "ssh"
            && !matches!(input.auth_type.as_str(), "sshAgent" | "fido2Agent")
        {
            if let Some(secret) = input.credential.as_deref() {
                let previous = match load_credential(&input.id) {
                    Ok(previous) => previous,
                    Err(error) => {
                        rollback_credentials(&changed);
                        return Err(error);
                    }
                };
                match save_credential(&input.id, secret) {
                    Ok(reference) => {
                        changed.push((input.id.clone(), previous));
                        Some(reference)
                    }
                    Err(error) => {
                        rollback_credentials(&changed);
                        return Err(error);
                    }
                }
            } else {
                None
            }
        } else {
            None
        };
        prepared.push((input, reference));
    }
    if let Err(error) = db.import_connections(&prepared).await {
        rollback_credentials(&changed);
        return Err(error);
    }
    Ok(count)
}

fn rollback_credentials(changed: &[(String, Option<String>)]) {
    for (id, previous) in changed.iter().rev() {
        if let Some(previous) = previous {
            let _ = save_credential(id, previous);
        } else {
            let _ = delete_credential(id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn derived_keys_are_stable_and_salt_specific() {
        let a = derive_key("long-password", b"0123456789abcdef").unwrap();
        let b = derive_key("long-password", b"0123456789abcdef").unwrap();
        let c = derive_key("long-password", b"fedcba9876543210").unwrap();
        assert_eq!(a, b);
        assert_ne!(a, c);
    }
    #[tokio::test]
    async fn encrypted_backup_round_trip_and_wrong_passphrase_rejection() {
        let directory = tempfile::tempdir().unwrap();
        let source = Database::open(&directory.path().join("source.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "backup-roundtrip".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Backup Server".into(),
            host: "backup.example".into(),
            port: 2222,
            username: "operator".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "encrypted".into(),
            tags: vec!["production".into()],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        source.save_connection(&input, None).await.unwrap();
        let path = directory.path().join("connections.json");
        export(
            &source,
            path.to_str().unwrap(),
            true,
            Some("correct horse battery staple"),
        )
        .await
        .unwrap();
        let serialized = std::fs::read_to_string(&path).unwrap();
        assert!(serialized.contains("argon2id+aes-256-gcm"));
        assert!(!serialized.contains("backup.example"));
        let target = Database::open(&directory.path().join("target.sqlite"))
            .await
            .unwrap();
        assert!(matches!(
            import(&target, path.to_str().unwrap(), Some("wrong passphrase")).await,
            Err(AppError::Authentication(_))
        ));
        assert_eq!(
            import(
                &target,
                path.to_str().unwrap(),
                Some("correct horse battery staple")
            )
            .await
            .unwrap(),
            1
        );
        let restored = target.get_connection("backup-roundtrip").await.unwrap();
        assert_eq!(restored.host, "backup.example");
        assert_eq!(restored.tags, vec!["production"]);
    }
    #[tokio::test]
    async fn encrypted_sync_hides_hosts_preserves_versions_and_import_conflicts() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("sync.sqlite"))
            .await
            .unwrap();
        let input = SaveConnectionInput {
            id: "same-id".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Local".into(),
            host: "secret-host.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: Some("/secret/key".into()),
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        let options = SyncOptions {
            include_hosts: true,
            include_private_key_paths: false,
            include_credentials: false,
        };
        let first = sync_write(
            &db,
            directory.path().to_str().unwrap(),
            "sync-password",
            &options,
        )
        .await
        .unwrap();
        let serialized = std::fs::read_to_string(&first.path).unwrap();
        assert!(!serialized.contains("secret-host.example"));
        assert!(!serialized.contains("/secret/key"));
        let second = sync_write(
            &db,
            directory.path().to_str().unwrap(),
            "sync-password",
            &options,
        )
        .await
        .unwrap();
        assert!(second.conflict_copy.is_some());
        assert!(Path::new(second.conflict_copy.as_ref().unwrap()).is_file());
        assert_eq!(
            sync_read(&db, directory.path().to_str().unwrap(), "sync-password")
                .await
                .unwrap()
                .connection_count,
            1
        );
        let all = db.list_connections().await.unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(
            all.iter().find(|item| item.id == "same-id").unwrap().name,
            "Local"
        );
        assert!(all.iter().any(|item| item.name.contains("同步冲突副本")));
    }
    #[tokio::test]
    async fn single_connection_export_uses_importable_secret_free_envelope() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("single.sqlite"))
            .await
            .unwrap();
        for (id, host) in [("one", "one.example"), ("two", "two.example")] {
            let input = SaveConnectionInput {
                id: id.into(),
                folder_id: None,
                protocol: "ssh".into(),
                name: id.into(),
                host: host.into(),
                port: 22,
                username: "root".into(),
                auth_type: "sshAgent".into(),
                private_key_path: None,
                certificate_path: None,
                host_key_policy: "strict".into(),
                note: "".into(),
                tags: vec![],
                encoding: "UTF-8".into(),
                startup_command: None,
                proxy_id: None,
                environment: Default::default(),
                credential: None,
            };
            db.save_connection(&input, None).await.unwrap();
        }
        let path = directory.path().join("one.json");
        export_one(&db, "one", path.to_str().unwrap())
            .await
            .unwrap();
        let serialized = std::fs::read_to_string(&path).unwrap();
        assert!(serialized.contains("one.example"));
        assert!(!serialized.contains("two.example"));
        let imported = Database::open(&directory.path().join("imported.sqlite"))
            .await
            .unwrap();
        assert_eq!(
            import(&imported, path.to_str().unwrap(), None)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            imported.get_connection("one").await.unwrap().host,
            "one.example"
        );
    }
    #[tokio::test]
    async fn import_drops_references_missing_on_the_target_mac() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("portable.json");
        let connection = ExportConnection {
            id: "portable".into(),
            folder_id: Some("other-mac-folder".into()),
            protocol: "ssh".into(),
            name: "Portable".into(),
            host: "portable.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: Some("other-mac-proxy".into()),
            environment: Default::default(),
            credential: None,
        };
        std::fs::write(
            &path,
            serde_json::to_vec(&BackupEnvelope::Plain {
                version: 1,
                connections: vec![connection],
            })
            .unwrap(),
        )
        .unwrap();
        let db = Database::open(&directory.path().join("target.sqlite"))
            .await
            .unwrap();
        assert_eq!(import(&db, path.to_str().unwrap(), None).await.unwrap(), 1);
        let imported = db.get_connection("portable").await.unwrap();
        assert!(imported.folder_id.is_none());
        assert!(imported.proxy_id.is_none());
    }
    #[tokio::test]
    async fn plain_backup_cannot_smuggle_credentials_into_keychain() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unsafe.json");
        let connection = ExportConnection {
            id: "unsafe".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Unsafe".into(),
            host: "unsafe.example".into(),
            port: 22,
            username: "root".into(),
            auth_type: "password".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            credential: Some("must-not-import".into()),
        };
        std::fs::write(
            &path,
            serde_json::to_vec(&BackupEnvelope::Plain {
                version: 1,
                connections: vec![connection],
            })
            .unwrap(),
        )
        .unwrap();
        let db = Database::open(&directory.path().join("unsafe.sqlite"))
            .await
            .unwrap();
        assert!(matches!(
            import(&db, path.to_str().unwrap(), None).await,
            Err(AppError::Validation(_))
        ));
        assert!(db.list_connections().await.unwrap().is_empty());
    }
    #[tokio::test]
    async fn oversized_backup_is_rejected_before_reading_contents() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("huge.json");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(50 * 1024 * 1024 + 1).unwrap();
        let db = Database::open(&directory.path().join("huge.sqlite"))
            .await
            .unwrap();
        assert!(matches!(
            import(&db, path.to_str().unwrap(), None).await,
            Err(AppError::Validation(_))
        ));
    }
}
