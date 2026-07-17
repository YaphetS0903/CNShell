use crate::error::{AppError, AppResult};
use crate::models::TouchIdSyncStatus;
use sha2::{Digest, Sha256};

const SERVICE: &str = "com.cnshell.desktop.touch-id-sync";
#[cfg(target_os = "macos")]
const ERR_SEC_USER_CANCELED: i32 = -128;
#[cfg(target_os = "macos")]
const ERR_SEC_NOT_AVAILABLE: i32 = -25291;
#[cfg(target_os = "macos")]
const ERR_SEC_AUTH_FAILED: i32 = -25293;
#[cfg(target_os = "macos")]
const ERR_SEC_ITEM_NOT_FOUND: i32 = -25300;
#[cfg(target_os = "macos")]
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

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn touch_id_available() -> bool {
    false
}

#[cfg(target_os = "windows")]
fn touch_id_available() -> bool {
    windows_hello::supported()
}

pub fn supported() -> bool {
    touch_id_available()
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

#[cfg(target_os = "windows")]
fn has_saved_account(account: &str) -> AppResult<bool> {
    windows_hello::has_saved(account)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn has_saved_account(_account: &str) -> AppResult<bool> {
    Ok(false)
}

pub fn status(folder: &str) -> AppResult<TouchIdSyncStatus> {
    let account = account_for_folder(folder)?;
    let supported = touch_id_available();
    let saved = has_saved_account(&account)?;
    let message = status_message(supported, saved);
    Ok(TouchIdSyncStatus {
        supported,
        saved,
        message,
    })
}

#[cfg(target_os = "macos")]
fn status_message(supported: bool, saved: bool) -> String {
    match (supported, saved) {
        (true, true) => "已保存同步口令；使用时 macOS 会要求 Touch ID".into(),
        (true, false) => "可使用 Touch ID 保护此同步文件夹的口令".into(),
        (false, true) => "已保存口令，但当前 Touch ID 不可用；请使用手动口令恢复".into(),
        (false, false) => "当前 Mac 未提供可用的 Touch ID，仍可使用手动同步口令".into(),
    }
}

#[cfg(target_os = "windows")]
fn status_message(supported: bool, saved: bool) -> String {
    match (supported, saved) {
        (true, true) => "已保存同步口令；使用时 Windows 会要求 Hello 验证".into(),
        (true, false) => "可使用 Windows Hello 保护此同步文件夹的口令".into(),
        (false, true) => "已保存口令，但当前 Windows Hello 不可用；请使用手动口令恢复".into(),
        (false, false) => "当前设备未提供可用的 Windows Hello，仍可使用手动同步口令".into(),
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn status_message(_supported: bool, _saved: bool) -> String {
    "此平台不支持生物识别保护，仍可使用手动同步口令".into()
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

#[cfg(target_os = "windows")]
pub fn save(folder: &str, passphrase: &str) -> AppResult<TouchIdSyncStatus> {
    if passphrase.len() < 8 || passphrase.len() > 1024 {
        return Err(AppError::Validation(
            "同步口令长度需要在 8 到 1024 字节之间".into(),
        ));
    }
    let account = account_for_folder(folder)?;
    windows_hello::save(&account, passphrase)?;
    status(folder)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn save(_folder: &str, _passphrase: &str) -> AppResult<TouchIdSyncStatus> {
    Err(AppError::Unavailable("此平台不支持生物识别口令保护".into()))
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

#[cfg(target_os = "windows")]
pub fn delete(folder: &str) -> AppResult<()> {
    windows_hello::delete(&account_for_folder(folder)?)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
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

#[cfg(target_os = "windows")]
pub fn load(folder: &str) -> AppResult<zeroize::Zeroizing<String>> {
    windows_hello::load(&account_for_folder(folder)?)
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub fn load(_folder: &str) -> AppResult<zeroize::Zeroizing<String>> {
    Err(AppError::Unavailable("此平台不支持生物识别口令保护".into()))
}

#[cfg(target_os = "windows")]
mod windows_hello {
    use super::{AppError, AppResult, SERVICE};
    use aes_gcm::{
        Aes256Gcm, Nonce,
        aead::{Aead, KeyInit},
    };
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    use rand::{RngCore as _, rngs::OsRng};
    use serde::{Deserialize, Serialize};
    use sha2::{Digest as _, Sha256};
    use std::{ffi::OsStr, os::windows::ffi::OsStrExt as _, ptr::null_mut};
    use windows_sys::Win32::{
        Foundation::{NTE_BAD_KEYSET, NTE_SILENT_CONTEXT, NTE_USER_CANCELLED},
        Security::Cryptography::{
            BCRYPT_OAEP_PADDING_INFO, BCRYPT_RSA_ALGORITHM, BCRYPT_SHA256_ALGORITHM,
            MS_NGC_KEY_STORAGE_PROVIDER, NCRYPT_ALLOW_DECRYPT_FLAG, NCRYPT_KEY_HANDLE,
            NCRYPT_KEY_USAGE_PROPERTY, NCRYPT_LENGTH_PROPERTY, NCRYPT_PAD_OAEP_FLAG,
            NCRYPT_PROV_HANDLE, NCRYPT_SILENT_FLAG, NCRYPT_UI_FORCE_HIGH_PROTECTION_FLAG,
            NCRYPT_UI_POLICY, NCRYPT_UI_POLICY_PROPERTY, NCRYPT_UI_PROTECT_KEY_FLAG,
            NCryptCreatePersistedKey, NCryptDecrypt, NCryptDeleteKey, NCryptEncrypt,
            NCryptFinalizeKey, NCryptFreeObject, NCryptOpenKey, NCryptOpenStorageProvider,
            NCryptSetProperty,
        },
    };
    use zeroize::{Zeroize as _, Zeroizing};

    const VERSION: u32 = 1;

    #[derive(Serialize, Deserialize)]
    #[serde(rename_all = "camelCase", deny_unknown_fields)]
    struct Envelope {
        version: u32,
        encrypted_key: String,
        nonce: String,
        ciphertext: String,
    }

    struct Provider(NCRYPT_PROV_HANDLE);
    impl Drop for Provider {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { NCryptFreeObject(self.0) };
            }
        }
    }

    struct Key(NCRYPT_KEY_HANDLE);
    impl Drop for Key {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe { NCryptFreeObject(self.0) };
            }
        }
    }

    fn succeeded(status: i32) -> bool {
        status >= 0
    }

    fn ncrypt_error(operation: &str, status: i32) -> AppError {
        if status == NTE_USER_CANCELLED {
            AppError::Authentication(format!("已取消 Windows Hello 验证，{operation}未完成"))
        } else if status == NTE_SILENT_CONTEXT {
            AppError::Unavailable("Windows Hello 当前无法显示验证界面；请解锁电脑后重试".into())
        } else {
            AppError::Storage(format!("{operation}失败（CNG 0x{:08X}）", status as u32))
        }
    }

    fn wide(value: &str) -> Vec<u16> {
        OsStr::new(value)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect()
    }

    fn key_name(account: &str) -> Vec<u16> {
        let digest = Sha256::digest(account.as_bytes());
        wide(&format!("CNshell.Sync.{digest:x}"))
    }

    fn open_provider() -> AppResult<Provider> {
        let mut handle = 0;
        let status =
            unsafe { NCryptOpenStorageProvider(&mut handle, MS_NGC_KEY_STORAGE_PROVIDER, 0) };
        if succeeded(status) && handle != 0 {
            Ok(Provider(handle))
        } else {
            Err(ncrypt_error("打开 Windows Hello 密钥提供程序", status))
        }
    }

    fn open_key(provider: &Provider, account: &str, silent: bool) -> AppResult<Option<Key>> {
        let mut handle = 0;
        let name = key_name(account);
        let status = unsafe {
            NCryptOpenKey(
                provider.0,
                &mut handle,
                name.as_ptr(),
                0,
                if silent { NCRYPT_SILENT_FLAG } else { 0 },
            )
        };
        if succeeded(status) && handle != 0 {
            Ok(Some(Key(handle)))
        } else if status == NTE_BAD_KEYSET {
            Ok(None)
        } else {
            Err(ncrypt_error("打开 Windows Hello 保护密钥", status))
        }
    }

    fn set_property<T>(key: &Key, name: *const u16, value: &T) -> AppResult<()> {
        let status = unsafe {
            NCryptSetProperty(
                key.0,
                name,
                value as *const T as *const u8,
                std::mem::size_of::<T>() as u32,
                0,
            )
        };
        if succeeded(status) {
            Ok(())
        } else {
            Err(ncrypt_error("配置 Windows Hello 保护密钥", status))
        }
    }

    fn create_key(provider: &Provider, account: &str) -> AppResult<Key> {
        let mut handle = 0;
        let name = key_name(account);
        let status = unsafe {
            NCryptCreatePersistedKey(
                provider.0,
                &mut handle,
                BCRYPT_RSA_ALGORITHM,
                name.as_ptr(),
                0,
                0,
            )
        };
        if !succeeded(status) || handle == 0 {
            return Err(ncrypt_error("创建 Windows Hello 保护密钥", status));
        }
        let key = Key(handle);
        set_property(&key, NCRYPT_LENGTH_PROPERTY, &2048_u32)?;
        set_property(&key, NCRYPT_KEY_USAGE_PROPERTY, &NCRYPT_ALLOW_DECRYPT_FLAG)?;
        let title = wide("CNshell Windows Hello");
        let friendly = wide("CNshell 加密同步口令");
        let description = wide("验证身份以读取 CNshell 加密同步口令");
        let policy = NCRYPT_UI_POLICY {
            dwVersion: 1,
            dwFlags: NCRYPT_UI_PROTECT_KEY_FLAG | NCRYPT_UI_FORCE_HIGH_PROTECTION_FLAG,
            pszCreationTitle: title.as_ptr(),
            pszFriendlyName: friendly.as_ptr(),
            pszDescription: description.as_ptr(),
        };
        set_property(&key, NCRYPT_UI_POLICY_PROPERTY, &policy)?;
        let status = unsafe { NCryptFinalizeKey(key.0, 0) };
        if succeeded(status) {
            Ok(key)
        } else {
            Err(ncrypt_error("完成 Windows Hello 保护密钥", status))
        }
    }

    fn crypt_key(key: &Key, input: &[u8], decrypt: bool) -> AppResult<Vec<u8>> {
        let padding = BCRYPT_OAEP_PADDING_INFO {
            pszAlgId: BCRYPT_SHA256_ALGORITHM,
            pbLabel: null_mut(),
            cbLabel: 0,
        };
        let mut size = 0_u32;
        let first = unsafe {
            if decrypt {
                NCryptDecrypt(
                    key.0,
                    input.as_ptr(),
                    input.len() as u32,
                    &padding as *const _ as *const _,
                    null_mut(),
                    0,
                    &mut size,
                    NCRYPT_PAD_OAEP_FLAG,
                )
            } else {
                NCryptEncrypt(
                    key.0,
                    input.as_ptr(),
                    input.len() as u32,
                    &padding as *const _ as *const _,
                    null_mut(),
                    0,
                    &mut size,
                    NCRYPT_PAD_OAEP_FLAG,
                )
            }
        };
        if !succeeded(first) || size == 0 || size > 16 * 1024 {
            return Err(ncrypt_error(
                if decrypt {
                    "Windows Hello 解锁"
                } else {
                    "Windows Hello 密钥封装"
                },
                first,
            ));
        }
        let mut output = vec![0_u8; size as usize];
        let mut written = 0_u32;
        let second = unsafe {
            if decrypt {
                NCryptDecrypt(
                    key.0,
                    input.as_ptr(),
                    input.len() as u32,
                    &padding as *const _ as *const _,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut written,
                    NCRYPT_PAD_OAEP_FLAG,
                )
            } else {
                NCryptEncrypt(
                    key.0,
                    input.as_ptr(),
                    input.len() as u32,
                    &padding as *const _ as *const _,
                    output.as_mut_ptr(),
                    output.len() as u32,
                    &mut written,
                    NCRYPT_PAD_OAEP_FLAG,
                )
            }
        };
        if !succeeded(second) || written == 0 || written > size {
            output.zeroize();
            return Err(ncrypt_error(
                if decrypt {
                    "Windows Hello 解锁"
                } else {
                    "Windows Hello 密钥封装"
                },
                second,
            ));
        }
        output.truncate(written as usize);
        Ok(output)
    }

    fn entry(account: &str) -> AppResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, account)
            .map_err(|error| AppError::Storage(format!("创建 Windows Hello 凭据项失败：{error}")))
    }

    pub fn supported() -> bool {
        open_provider().is_ok()
    }

    pub fn has_saved(account: &str) -> AppResult<bool> {
        match entry(account)?.get_password() {
            Ok(_) => Ok(true),
            Err(keyring::Error::NoEntry) => Ok(false),
            Err(error) => Err(AppError::Storage(format!(
                "读取 Windows Hello 凭据状态失败：{error}"
            ))),
        }
    }

    pub fn save(account: &str, passphrase: &str) -> AppResult<()> {
        if has_saved(account)? {
            drop(load(account)?);
        }
        let provider = open_provider()?;
        let (mut key, created) = match open_key(&provider, account, false)? {
            Some(key) => (key, false),
            None => (create_key(&provider, account)?, true),
        };
        let mut wrapping_key = [0_u8; 32];
        let mut nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut wrapping_key);
        OsRng.fill_bytes(&mut nonce);
        let cipher = Aes256Gcm::new_from_slice(&wrapping_key)
            .map_err(|_| AppError::Internal("无法初始化 Windows Hello 同步加密".into()))?;
        let ciphertext = cipher
            .encrypt(Nonce::from_slice(&nonce), passphrase.as_bytes())
            .map_err(|_| AppError::Storage("Windows Hello 同步口令加密失败".into()))?;
        let encrypted_key = crypt_key(&key, &wrapping_key, false)?;
        wrapping_key.zeroize();
        let envelope = Envelope {
            version: VERSION,
            encrypted_key: STANDARD.encode(encrypted_key),
            nonce: STANDARD.encode(nonce),
            ciphertext: STANDARD.encode(ciphertext),
        };
        let encoded = serde_json::to_string(&envelope)
            .map_err(|error| AppError::Storage(format!("Windows Hello 密文编码失败：{error}")))?;
        if let Err(error) = entry(account)?.set_password(&encoded) {
            if created {
                let _ = unsafe { NCryptDeleteKey(key.0, 0) };
                key.0 = 0;
            }
            return Err(AppError::Storage(format!(
                "Windows Hello 密文保存失败：{error}"
            )));
        }
        Ok(())
    }

    pub fn load(account: &str) -> AppResult<Zeroizing<String>> {
        let mut encoded = match entry(account)?.get_password() {
            Ok(value) => Zeroizing::new(value),
            Err(keyring::Error::NoEntry) => {
                return Err(AppError::Validation(
                    "尚未为此同步文件夹保存 Windows Hello 口令".into(),
                ));
            }
            Err(error) => {
                return Err(AppError::Storage(format!(
                    "读取 Windows Hello 密文失败：{error}"
                )));
            }
        };
        let envelope: Envelope = serde_json::from_str(&encoded)
            .map_err(|_| AppError::Storage("Windows Hello 密文损坏，请移除后重新保存".into()))?;
        encoded.zeroize();
        if envelope.version != VERSION {
            return Err(AppError::Storage("Windows Hello 密文版本不受支持".into()));
        }
        let encrypted_key = STANDARD
            .decode(envelope.encrypted_key)
            .map_err(|_| AppError::Storage("Windows Hello 包裹密钥损坏".into()))?;
        let nonce = STANDARD
            .decode(envelope.nonce)
            .map_err(|_| AppError::Storage("Windows Hello nonce 损坏".into()))?;
        let ciphertext = STANDARD
            .decode(envelope.ciphertext)
            .map_err(|_| AppError::Storage("Windows Hello 口令密文损坏".into()))?;
        if nonce.len() != 12 || encrypted_key.len() > 16 * 1024 || ciphertext.len() > 2048 {
            return Err(AppError::Storage("Windows Hello 密文长度无效".into()));
        }
        let provider = open_provider()?;
        let key = open_key(&provider, account, false)?.ok_or_else(|| {
            AppError::Storage("Windows Hello 保护密钥已丢失，请使用手动口令恢复".into())
        })?;
        let mut wrapping_key = crypt_key(&key, &encrypted_key, true)?;
        if wrapping_key.len() != 32 {
            wrapping_key.zeroize();
            return Err(AppError::Storage("Windows Hello 解锁密钥长度无效".into()));
        }
        let cipher = Aes256Gcm::new_from_slice(&wrapping_key)
            .map_err(|_| AppError::Storage("Windows Hello 解锁密钥无效".into()))?;
        let mut plaintext = cipher
            .decrypt(Nonce::from_slice(&nonce), ciphertext.as_ref())
            .map_err(|_| {
                AppError::Authentication("Windows Hello 验证成功，但同步口令密文校验失败".into())
            })?;
        wrapping_key.zeroize();
        match String::from_utf8(std::mem::take(&mut plaintext)) {
            Ok(value) => Ok(Zeroizing::new(value)),
            Err(error) => {
                let mut invalid = error.into_bytes();
                invalid.zeroize();
                Err(AppError::Storage("Windows Hello 同步口令编码无效".into()))
            }
        }
    }

    pub fn delete(account: &str) -> AppResult<()> {
        if let Ok(provider) = open_provider() {
            if let Some(mut key) = open_key(&provider, account, false)? {
                let status = unsafe { NCryptDeleteKey(key.0, 0) };
                if !succeeded(status) {
                    return Err(ncrypt_error("删除 Windows Hello 保护密钥", status));
                }
                key.0 = 0;
            }
        }
        match entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(AppError::Storage(format!(
                "删除 Windows Hello 密文失败：{error}"
            ))),
        }
    }
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
