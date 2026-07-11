use crate::error::{AppError, AppResult};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "com.cnshell.desktop";

pub fn bookmark_ref(connection_id: &str) -> String {
    format!("private-key-bookmark:{connection_id}")
}

pub fn save(connection_id: &str, path: &Path) -> AppResult<()> {
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::Validation(
            "私钥必须是存在的本地绝对文件路径".into(),
        ));
    }
    let encoded = STANDARD.encode(create(path)?);
    keyring::Entry::new(KEYCHAIN_SERVICE, &bookmark_ref(connection_id))
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(&encoded)
        .map_err(|error| AppError::Storage(format!("私钥 Bookmark 保存失败：{error}")))
}

pub fn load(connection_id: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &bookmark_ref(connection_id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!(
            "私钥 Bookmark 读取失败：{error}"
        ))),
    }
}

pub fn restore(connection_id: &str, previous: Option<&str>) {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &bookmark_ref(connection_id));
    if let Ok(entry) = entry {
        if let Some(value) = previous {
            let _ = entry.set_password(value);
        } else {
            let _ = entry.delete_credential();
        }
    }
}

pub fn copy(source_id: &str, destination_id: &str) -> AppResult<()> {
    let Some(value) = load(source_id)? else {
        return Ok(());
    };
    keyring::Entry::new(KEYCHAIN_SERVICE, &bookmark_ref(destination_id))
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(&value)
        .map_err(|error| AppError::Storage(format!("私钥 Bookmark 复制失败：{error}")))
}

pub fn delete(connection_id: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &bookmark_ref(connection_id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!(
            "私钥 Bookmark 清理失败：{error}"
        ))),
    }
}

pub struct PrivateKeyAccess {
    path: PathBuf,
    #[cfg(target_os = "macos")]
    url: Option<objc2::rc::Retained<objc2_foundation::NSURL>>,
    scoped: bool,
}

impl PrivateKeyAccess {
    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for PrivateKeyAccess {
    fn drop(&mut self) {
        #[cfg(target_os = "macos")]
        if self.scoped {
            if let Some(url) = &self.url {
                unsafe { url.stopAccessingSecurityScopedResource() };
            }
        }
    }
}

pub fn access(connection_id: &str, fallback: &Path) -> AppResult<PrivateKeyAccess> {
    let Some(encoded) = load(connection_id)? else {
        return Ok(PrivateKeyAccess {
            path: fallback.to_path_buf(),
            #[cfg(target_os = "macos")]
            url: None,
            scoped: false,
        });
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Storage("私钥 Bookmark 数据损坏，请重新选择私钥".into()))?;
    resolve(&bytes)
}

#[cfg(target_os = "macos")]
fn create(path: &Path) -> AppResult<Vec<u8>> {
    use objc2_foundation::{NSString, NSURL, NSURLBookmarkCreationOptions};
    let path = NSString::from_str(&path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&path);
    let options = NSURLBookmarkCreationOptions::WithSecurityScope
        | NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess;
    url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
        options, None, None,
    )
    .map(|data| data.to_vec())
    .map_err(|error| AppError::Storage(format!("无法创建私钥安全 Bookmark：{error}")))
}

#[cfg(not(target_os = "macos"))]
fn create(path: &Path) -> AppResult<Vec<u8>> {
    Ok(path.to_string_lossy().as_bytes().to_vec())
}

#[cfg(target_os = "macos")]
fn resolve(bytes: &[u8]) -> AppResult<PrivateKeyAccess> {
    use objc2::runtime::Bool;
    use objc2_foundation::{NSData, NSURL, NSURLBookmarkResolutionOptions};
    let data = NSData::with_bytes(bytes);
    let mut stale = Bool::NO;
    let options = NSURLBookmarkResolutionOptions::WithSecurityScope
        | NSURLBookmarkResolutionOptions::WithoutUI;
    let url = unsafe {
        NSURL::URLByResolvingBookmarkData_options_relativeToURL_bookmarkDataIsStale_error(
            &data, options, None, &mut stale,
        )
    }
    .map_err(|error| {
        AppError::Storage(format!("私钥 Bookmark 无法解析，请重新选择私钥：{error}"))
    })?;
    if bool::from(stale) {
        return Err(AppError::Storage(
            "私钥 Bookmark 已过期，请重新选择私钥".into(),
        ));
    }
    let path = url
        .path()
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| AppError::Storage("私钥 Bookmark 没有有效路径，请重新选择私钥".into()))?;
    let scoped = unsafe { url.startAccessingSecurityScopedResource() };
    Ok(PrivateKeyAccess {
        path,
        url: Some(url),
        scoped,
    })
}

#[cfg(not(target_os = "macos"))]
fn resolve(bytes: &[u8]) -> AppResult<PrivateKeyAccess> {
    let path = String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::Storage("私钥 Bookmark 数据损坏".into()))?;
    Ok(PrivateKeyAccess {
        path: PathBuf::from(path),
        scoped: false,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bookmark_reference_does_not_include_the_path() {
        assert_eq!(
            bookmark_ref("connection"),
            "private-key-bookmark:connection"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_bookmark_round_trips_a_private_key_path() {
        let directory = tempfile::tempdir().unwrap();
        let key = directory.path().join("id_ed25519");
        std::fs::write(&key, b"private-key-fixture").unwrap();
        let bookmark = create(&key).unwrap();
        let access = resolve(&bookmark).unwrap();
        assert_eq!(
            access.path().canonicalize().unwrap(),
            key.canonicalize().unwrap()
        );
        assert_eq!(
            std::fs::read(access.path()).unwrap(),
            b"private-key-fixture"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_bookmark_keychain_lifecycle_keeps_file_accessible() {
        let id = format!("bookmark-test-{}", uuid::Uuid::new_v4());
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete(&self.0);
            }
        }
        let _cleanup = Cleanup(id.clone());
        let directory = tempfile::tempdir().unwrap();
        let key = directory.path().join("id_ed25519");
        std::fs::write(&key, b"keychain-bookmark-fixture").unwrap();
        save(&id, &key).unwrap();
        assert!(load(&id).unwrap().is_some());
        let access = access(&id, Path::new("/unused/fallback")).unwrap();
        assert_eq!(
            std::fs::read(access.path()).unwrap(),
            b"keychain-bookmark-fixture"
        );
        delete(&id).unwrap();
        assert!(load(&id).unwrap().is_none());
    }

    #[test]
    fn bookmark_rejects_missing_or_relative_private_keys() {
        assert!(save("missing", Path::new("relative-key")).is_err());
        assert!(save("missing", Path::new("/definitely/missing/cnshell-key")).is_err());
    }
}
