use crate::error::{AppError, AppResult};
use crate::models::TouchIdSyncStatus;
use sha2::{Digest, Sha256};

const SERVICE: &str = "com.cnshell.desktop.touch-id-sync";
const ERR_SEC_USER_CANCELED: i32 = -128;
const ERR_SEC_NOT_AVAILABLE: i32 = -25291;
const ERR_SEC_AUTH_FAILED: i32 = -25293;
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
const ERR_SEC_INTERACTION_NOT_ALLOWED: i32 = -25308;

fn account_for_folder(folder: &str) -> AppResult<String> {
    let canonical = std::fs::canonicalize(folder)
        .map_err(|_| AppError::Validation("同步位置必须是已存在的文件夹".into()))?;
    if !canonical.is_dir() {
        return Err(AppError::Validation("同步位置必须是已存在的文件夹".into()));
    }
    #[cfg(unix)]
    let digest = {
        use std::os::unix::ffi::OsStrExt;
        Sha256::digest(canonical.as_os_str().as_bytes())
    };
    #[cfg(not(unix))]
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    Ok(format!("sync-folder:{digest:x}"))
}

#[cfg(target_os = "macos")]
fn touch_id_available() -> bool {
    use objc2_local_authentication::{LABiometryType, LAContext, LAPolicy};
    let context = unsafe { LAContext::new() };
    unsafe {
        context
            .canEvaluatePolicy_error(LAPolicy::DeviceOwnerAuthenticationWithBiometrics)
            .is_ok()
            && context.biometryType() == LABiometryType::TouchID
    }
}

#[cfg(not(target_os = "macos"))]
fn touch_id_available() -> bool {
    false
}

#[cfg(target_os = "macos")]
fn password_options(account: &str) -> security_framework::passwords::PasswordOptions {
    let mut options =
        security_framework::passwords::PasswordOptions::new_generic_password(SERVICE, account);
    options.use_protected_keychain();
    options.set_access_synchronized(Some(false));
    options
}

#[cfg(target_os = "macos")]
fn has_saved_account(account: &str) -> AppResult<bool> {
    use security_framework::item::{ItemClass, ItemSearchOptions};
    let result = ItemSearchOptions::new()
        .class(ItemClass::generic_password())
        .service(SERVICE)
        .account(account)
        .ignore_legacy_keychains()
        .search();
    match result {
        Ok(_) => Ok(true),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(false),
        Err(error) => Err(AppError::Storage(format!(
            "Touch ID 同步密钥状态读取失败：{error}"
        ))),
    }
}

#[cfg(not(target_os = "macos"))]
fn has_saved_account(_account: &str) -> AppResult<bool> {
    Ok(false)
}

pub fn status(folder: &str) -> AppResult<TouchIdSyncStatus> {
    let account = account_for_folder(folder)?;
    let supported = touch_id_available();
    let saved = has_saved_account(&account)?;
    let message = match (supported, saved) {
        (true, true) => "已保存同步口令；使用时 macOS 会要求 Touch ID".into(),
        (true, false) => "可使用 Touch ID 保护此同步文件夹的口令".into(),
        (false, true) => "已保存口令，但当前 Touch ID 不可用；请使用手动口令恢复".into(),
        (false, false) => "当前 Mac 未提供可用的 Touch ID，仍可使用手动同步口令".into(),
    };
    Ok(TouchIdSyncStatus {
        supported,
        saved,
        message,
    })
}

#[cfg(target_os = "macos")]
pub fn save(folder: &str, passphrase: &str) -> AppResult<TouchIdSyncStatus> {
    use security_framework::access_control::{ProtectionMode, SecAccessControl};
    use security_framework::passwords::{AccessControlOptions, set_generic_password_options};
    if passphrase.len() < 8 {
        return Err(AppError::Validation("同步口令至少需要 8 位".into()));
    }
    if !touch_id_available() {
        return Err(AppError::Unavailable(
            "当前 Mac 没有可用的 Touch ID，或尚未录入指纹".into(),
        ));
    }
    let account = account_for_folder(folder)?;
    if has_saved_account(&account)? {
        set_generic_password_options(passphrase.as_bytes(), password_options(&account)).map_err(
            |error| match error.code() {
                ERR_SEC_USER_CANCELED => {
                    AppError::Authentication("已取消 Touch ID 验证，原同步口令保持不变".into())
                }
                ERR_SEC_AUTH_FAILED => AppError::Authentication(
                    "Touch ID 验证失败，原同步口令保持不变；也可以移除后重新保存".into(),
                ),
                _ => AppError::Storage(format!("Touch ID 同步密钥更新失败：{error}")),
            },
        )?;
        return status(folder);
    }
    let access_control = SecAccessControl::create_with_protection(
        Some(ProtectionMode::AccessibleWhenPasscodeSetThisDeviceOnly),
        AccessControlOptions::BIOMETRY_CURRENT_SET.bits(),
    )
    .map_err(|error| AppError::Unavailable(format!("无法创建 Touch ID 访问策略：{error}")))?;
    let mut options = password_options(&account);
    options.set_access_control(access_control);
    options.set_label("CNshell 加密同步口令");
    options.set_description("仅在 Touch ID 验证后用于读取或写入 CNshell 加密同步包");
    set_generic_password_options(passphrase.as_bytes(), options)
        .map_err(|error| AppError::Storage(format!("Touch ID 同步密钥保存失败：{error}")))?;
    status(folder)
}

#[cfg(not(target_os = "macos"))]
pub fn save(_folder: &str, _passphrase: &str) -> AppResult<TouchIdSyncStatus> {
    Err(AppError::Unavailable("Touch ID 仅在 macOS 上可用".into()))
}

#[cfg(target_os = "macos")]
pub fn delete(folder: &str) -> AppResult<()> {
    use security_framework::passwords::delete_generic_password_options;
    let account = account_for_folder(folder)?;
    match delete_generic_password_options(password_options(&account)) {
        Ok(()) => Ok(()),
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Ok(()),
        Err(error) => Err(AppError::Storage(format!(
            "Touch ID 同步密钥删除失败：{error}"
        ))),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn delete(_folder: &str) -> AppResult<()> {
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn load(folder: &str) -> AppResult<zeroize::Zeroizing<String>> {
    use security_framework::passwords::generic_password;
    let account = account_for_folder(folder)?;
    match generic_password(password_options(&account)) {
        Ok(secret) => match String::from_utf8(secret) {
            Ok(secret) => Ok(zeroize::Zeroizing::new(secret)),
            Err(error) => {
                use zeroize::Zeroize as _;
                let mut invalid = error.into_bytes();
                invalid.zeroize();
                Err(AppError::Storage(
                    "Touch ID 同步密钥内容无效，请删除后重新保存".into(),
                ))
            }
        },
        Err(error) if error.code() == ERR_SEC_ITEM_NOT_FOUND => Err(AppError::Validation(
            "尚未为此同步文件夹保存 Touch ID 同步口令".into(),
        )),
        Err(error) if error.code() == ERR_SEC_USER_CANCELED => Err(AppError::Authentication(
            "已取消 Touch ID 验证；可以改用手动同步口令".into(),
        )),
        Err(error) if error.code() == ERR_SEC_AUTH_FAILED => Err(AppError::Authentication(
            "Touch ID 验证失败或已录入的指纹发生变化；可以改用手动口令并重新保存".into(),
        )),
        Err(error)
            if matches!(
                error.code(),
                ERR_SEC_NOT_AVAILABLE | ERR_SEC_INTERACTION_NOT_ALLOWED
            ) =>
        {
            Err(AppError::Unavailable(
                "当前无法显示 Touch ID 验证，请解锁 Mac 后重试或使用手动口令".into(),
            ))
        }
        Err(error) => Err(AppError::Storage(format!(
            "Touch ID 同步密钥读取失败：{error}"
        ))),
    }
}

#[cfg(not(target_os = "macos"))]
pub fn load(_folder: &str) -> AppResult<zeroize::Zeroizing<String>> {
    Err(AppError::Unavailable("Touch ID 仅在 macOS 上可用".into()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_account_is_hashed_and_does_not_expose_the_path() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().to_string_lossy();
        let account = account_for_folder(&folder).unwrap();
        assert!(account.starts_with("sync-folder:"));
        assert!(!account.contains(folder.as_ref()));
        assert_eq!(account, account_for_folder(&folder).unwrap());
    }

    #[test]
    fn missing_folder_cannot_create_a_vault_account() {
        assert!(matches!(
            account_for_folder("/definitely/missing/cnshell-touch-id"),
            Err(AppError::Validation(_))
        ));
    }
}
