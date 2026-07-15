use crate::error::{AppError, AppResult};
use base64::{Engine, engine::general_purpose::STANDARD};
use std::path::{Path, PathBuf};

const KEYCHAIN_SERVICE: &str = "com.cnshell.desktop";

pub fn bookmark_ref(connection_id: &str) -> String {
    format!("private-key-bookmark:{connection_id}")
}

pub fn certificate_bookmark_ref(connection_id: &str) -> String {
    format!("ssh-certificate-bookmark:{connection_id}")
}

pub fn rdp_drive_bookmark_ref(connection_id: &str) -> String {
    format!("rdp-drive-bookmark:{connection_id}")
}

pub fn save(connection_id: &str, path: &Path) -> AppResult<()> {
    save_value(&bookmark_ref(connection_id), path, "私钥")
}

pub fn save_certificate(connection_id: &str, path: &Path) -> AppResult<()> {
    save_value(
        &certificate_bookmark_ref(connection_id),
        path,
        "SSH Certificate",
    )
}

fn save_value(reference: &str, path: &Path, label: &str) -> AppResult<()> {
    if !path.is_absolute() || !path.is_file() {
        return Err(AppError::Validation(format!(
            "{label}必须是存在的本地绝对文件路径"
        )));
    }
    let encoded = STANDARD.encode(create(path, label, true)?);
    keyring::Entry::new(KEYCHAIN_SERVICE, reference)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(&encoded)
        .map_err(|error| AppError::Storage(format!("{label} Bookmark 保存失败：{error}")))
}

pub fn save_rdp_drive(connection_id: &str, path: &Path) -> AppResult<()> {
    if !path.is_absolute() || !path.is_dir() {
        return Err(AppError::Validation(
            "RDP 映射目录必须是存在的本地绝对文件夹".into(),
        ));
    }
    let reference = rdp_drive_bookmark_ref(connection_id);
    let encoded = STANDARD.encode(create(path, "RDP 映射目录", false)?);
    keyring::Entry::new(KEYCHAIN_SERVICE, &reference)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(&encoded)
        .map_err(|error| AppError::Storage(format!("RDP 映射目录 Bookmark 保存失败：{error}")))
}

pub fn load(connection_id: &str) -> AppResult<Option<String>> {
    load_value(&bookmark_ref(connection_id), "私钥")
}

pub fn load_certificate(connection_id: &str) -> AppResult<Option<String>> {
    load_value(&certificate_bookmark_ref(connection_id), "SSH Certificate")
}

pub fn load_rdp_drive(connection_id: &str) -> AppResult<Option<String>> {
    load_value(&rdp_drive_bookmark_ref(connection_id), "RDP 映射目录")
}

fn load_value(reference: &str, label: &str) -> AppResult<Option<String>> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, reference)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(value) => Ok(Some(value)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(format!(
            "{label} Bookmark 读取失败：{error}"
        ))),
    }
}

pub fn restore(connection_id: &str, previous: Option<&str>) {
    restore_value(&bookmark_ref(connection_id), previous);
}

pub fn restore_certificate(connection_id: &str, previous: Option<&str>) {
    restore_value(&certificate_bookmark_ref(connection_id), previous);
}

pub fn restore_rdp_drive(connection_id: &str, previous: Option<&str>) {
    restore_value(&rdp_drive_bookmark_ref(connection_id), previous);
}

fn restore_value(reference: &str, previous: Option<&str>) {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, reference);
    if let Ok(entry) = entry {
        if let Some(value) = previous {
            let _ = entry.set_password(value);
        } else {
            let _ = entry.delete_credential();
        }
    }
}

pub fn copy(source_id: &str, destination_id: &str) -> AppResult<()> {
    copy_value(
        &bookmark_ref(source_id),
        &bookmark_ref(destination_id),
        "私钥",
    )
}

pub fn copy_certificate(source_id: &str, destination_id: &str) -> AppResult<()> {
    copy_value(
        &certificate_bookmark_ref(source_id),
        &certificate_bookmark_ref(destination_id),
        "SSH Certificate",
    )
}

pub fn copy_rdp_drive(source_id: &str, destination_id: &str) -> AppResult<()> {
    copy_value(
        &rdp_drive_bookmark_ref(source_id),
        &rdp_drive_bookmark_ref(destination_id),
        "RDP 映射目录",
    )
}

fn copy_value(source: &str, destination: &str, label: &str) -> AppResult<()> {
    let Some(value) = load_value(source, label)? else {
        return Ok(());
    };
    keyring::Entry::new(KEYCHAIN_SERVICE, destination)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(&value)
        .map_err(|error| AppError::Storage(format!("{label} Bookmark 复制失败：{error}")))
}

pub fn delete(connection_id: &str) -> AppResult<()> {
    delete_value(&bookmark_ref(connection_id), "私钥")
}

pub fn delete_certificate(connection_id: &str) -> AppResult<()> {
    delete_value(&certificate_bookmark_ref(connection_id), "SSH Certificate")
}

pub fn delete_rdp_drive(connection_id: &str) -> AppResult<()> {
    delete_value(&rdp_drive_bookmark_ref(connection_id), "RDP 映射目录")
}

fn delete_value(reference: &str, label: &str) -> AppResult<()> {
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, reference)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!(
            "{label} Bookmark 清理失败：{error}"
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
    access_value(load(connection_id)?, fallback, "私钥")
}

pub fn access_certificate(connection_id: &str, fallback: &Path) -> AppResult<PrivateKeyAccess> {
    access_value(
        load_certificate(connection_id)?,
        fallback,
        "SSH Certificate",
    )
}

pub fn access_rdp_drive(connection_id: &str, fallback: &Path) -> AppResult<PrivateKeyAccess> {
    access_value(load_rdp_drive(connection_id)?, fallback, "RDP 映射目录")
}

pub fn access_selected_directory(path: &Path) -> AppResult<PrivateKeyAccess> {
    if !path.is_absolute() || path.to_string_lossy().len() > 16 * 1024 {
        return Err(AppError::Validation(
            "插件授权目录必须是长度受限的本地绝对路径".into(),
        ));
    }
    let metadata = std::fs::symlink_metadata(path)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AppError::Validation(
            "插件授权目录必须是存在的非符号链接文件夹".into(),
        ));
    }
    let access = resolve(&create(path, "插件授权目录", true)?, "插件授权目录")?;
    if !access.path().is_dir() {
        return Err(AppError::Unavailable(
            "插件授权目录已不存在，请重新选择".into(),
        ));
    }
    Ok(access)
}

fn access_value(
    encoded: Option<String>,
    fallback: &Path,
    label: &str,
) -> AppResult<PrivateKeyAccess> {
    let Some(encoded) = encoded else {
        return Ok(PrivateKeyAccess {
            path: fallback.to_path_buf(),
            #[cfg(target_os = "macos")]
            url: None,
            scoped: false,
        });
    };
    let bytes = STANDARD
        .decode(encoded)
        .map_err(|_| AppError::Storage(format!("{label} Bookmark 数据损坏，请重新选择")))?;
    resolve(&bytes, label)
}

#[cfg(target_os = "macos")]
fn create(path: &Path, label: &str, read_only: bool) -> AppResult<Vec<u8>> {
    use objc2_foundation::{NSString, NSURL, NSURLBookmarkCreationOptions};
    let path = NSString::from_str(&path.to_string_lossy());
    let url = NSURL::fileURLWithPath(&path);
    let mut options = NSURLBookmarkCreationOptions::WithSecurityScope;
    if read_only {
        options |= NSURLBookmarkCreationOptions::SecurityScopeAllowOnlyReadAccess;
    }
    url.bookmarkDataWithOptions_includingResourceValuesForKeys_relativeToURL_error(
        options, None, None,
    )
    .map(|data| data.to_vec())
    .map_err(|error| AppError::Storage(format!("无法创建{label}安全 Bookmark：{error}")))
}

#[cfg(not(target_os = "macos"))]
fn create(path: &Path, _label: &str, _read_only: bool) -> AppResult<Vec<u8>> {
    Ok(path.to_string_lossy().as_bytes().to_vec())
}

#[cfg(target_os = "macos")]
fn resolve(bytes: &[u8], label: &str) -> AppResult<PrivateKeyAccess> {
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
        AppError::Storage(format!("{label} Bookmark 无法解析，请重新选择：{error}"))
    })?;
    if bool::from(stale) {
        return Err(AppError::Storage(format!(
            "{label} Bookmark 已过期，请重新选择"
        )));
    }
    let path = url
        .path()
        .map(|path| PathBuf::from(path.to_string()))
        .ok_or_else(|| AppError::Storage(format!("{label} Bookmark 没有有效路径，请重新选择")))?;
    let scoped = unsafe { url.startAccessingSecurityScopedResource() };
    Ok(PrivateKeyAccess {
        path,
        url: Some(url),
        scoped,
    })
}

#[cfg(not(target_os = "macos"))]
fn resolve(bytes: &[u8], label: &str) -> AppResult<PrivateKeyAccess> {
    let path = String::from_utf8(bytes.to_vec())
        .map_err(|_| AppError::Storage(format!("{label} Bookmark 数据损坏")))?;
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
        let bookmark = create(&key, "私钥", true).unwrap();
        let access = resolve(&bookmark, "私钥").unwrap();
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
    fn rdp_drive_bookmark_keeps_the_selected_directory_writable() {
        let directory = tempfile::tempdir().unwrap();
        let bookmark = create(directory.path(), "RDP 映射目录", false).unwrap();
        let access = resolve(&bookmark, "RDP 映射目录").unwrap();
        let fixture = access.path().join("cnshell-rdp-bookmark.txt");
        std::fs::write(&fixture, b"mapped-drive").unwrap();
        assert_eq!(std::fs::read(&fixture).unwrap(), b"mapped-drive");
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
