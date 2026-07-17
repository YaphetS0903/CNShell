use crate::error::{AppError, AppResult};
use crate::models::{PlatformCapabilities, PlatformFeatureCapability};
use std::path::Path;

fn feature(available: bool, message: impl Into<String>) -> PlatformFeatureCapability {
    PlatformFeatureCapability {
        available,
        message: message.into(),
    }
}

pub fn capabilities() -> PlatformCapabilities {
    let rdp = crate::rdp::preflight();
    let mosh = crate::mosh::helper_path();
    let kermit = crate::kermit::available();
    let x11 = crate::x11::availability();
    let agent = ssh_agent_available();
    PlatformCapabilities {
        operating_system: std::env::consts::OS.into(),
        architecture: std::env::consts::ARCH.into(),
        display_name: platform_display_name().into(),
        shortcut_modifier: shortcut_modifier().into(),
        credential_store_name: credential_store_name().into(),
        file_manager_name: file_manager_name().into(),
        biometric_name: biometric_name().into(),
        rdp: feature(rdp.available, rdp.message),
        mosh: feature(
            mosh.is_some(),
            if mosh.is_some() {
                "内置 Mosh 客户端可用"
            } else {
                "此平台的内置 Mosh 客户端尚未安装"
            },
        ),
        kermit: feature(
            kermit,
            if kermit {
                "内置 G-Kermit 客户端可用"
            } else {
                "此平台的内置 G-Kermit 客户端尚未安装"
            },
        ),
        x11: feature(x11.is_ok(), x11.unwrap_or_else(|message| message)),
        ssh_agent: feature(
            agent,
            if agent {
                "已检测到可用的 SSH Agent"
            } else {
                "未检测到可用的 SSH Agent"
            },
        ),
        biometric: biometric_capability(),
        serial: feature(true, "Serial/COM 串口支持已启用"),
    }
}

#[cfg(target_os = "macos")]
fn platform_display_name() -> &'static str {
    "macOS"
}

#[cfg(target_os = "windows")]
fn platform_display_name() -> &'static str {
    "Windows"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn platform_display_name() -> &'static str {
    std::env::consts::OS
}

#[cfg(target_os = "macos")]
fn shortcut_modifier() -> &'static str {
    "⌘"
}

#[cfg(not(target_os = "macos"))]
fn shortcut_modifier() -> &'static str {
    "Ctrl"
}

#[cfg(target_os = "macos")]
fn credential_store_name() -> &'static str {
    "macOS Keychain"
}

#[cfg(target_os = "windows")]
fn credential_store_name() -> &'static str {
    "Windows 凭据管理器"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn credential_store_name() -> &'static str {
    "系统凭据库"
}

#[cfg(target_os = "macos")]
fn file_manager_name() -> &'static str {
    "Finder"
}

#[cfg(target_os = "windows")]
fn file_manager_name() -> &'static str {
    "文件资源管理器"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn file_manager_name() -> &'static str {
    "文件管理器"
}

#[cfg(target_os = "macos")]
fn biometric_name() -> &'static str {
    "Touch ID"
}

#[cfg(target_os = "windows")]
fn biometric_name() -> &'static str {
    "Windows Hello"
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn biometric_name() -> &'static str {
    "系统生物识别"
}

#[cfg(target_os = "macos")]
fn biometric_capability() -> PlatformFeatureCapability {
    feature(
        crate::touch_id::supported(),
        if crate::touch_id::supported() {
            "Touch ID 可用于保护加密同步口令"
        } else {
            "当前 Mac 未提供可用的 Touch ID"
        },
    )
}

#[cfg(target_os = "windows")]
fn biometric_capability() -> PlatformFeatureCapability {
    feature(false, "Windows Hello 同步口令保护正在适配")
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn biometric_capability() -> PlatformFeatureCapability {
    feature(false, "此平台暂不支持生物识别保护")
}

#[cfg(target_os = "windows")]
pub fn ssh_agent_available() -> bool {
    std::env::var_os("SSH_AUTH_SOCK").is_some() || which::which("ssh-add.exe").is_ok()
}

#[cfg(not(target_os = "windows"))]
pub fn ssh_agent_available() -> bool {
    std::env::var_os("SSH_AUTH_SOCK").is_some()
}

pub fn open_local_path(path: &Path, application: Option<&str>) -> AppResult<()> {
    if !path.exists() {
        return Err(AppError::Validation("要打开的本地文件不存在".into()));
    }
    if let Some(application) = application {
        validate_application_path(application)?;
    }
    open_local_path_impl(path, application)
}

#[cfg(target_os = "macos")]
fn open_local_path_impl(path: &Path, application: Option<&str>) -> AppResult<()> {
    let mut command = std::process::Command::new("/usr/bin/open");
    if let Some(application) = application {
        command.arg("-a").arg(application);
    }
    command
        .arg(path)
        .spawn()
        .map_err(|error| AppError::Unavailable(format!("无法打开本地文件：{error}")))?;
    Ok(())
}

#[cfg(target_os = "windows")]
fn open_local_path_impl(path: &Path, application: Option<&str>) -> AppResult<()> {
    if let Some(application) = application {
        std::process::Command::new(application)
            .arg(path)
            .spawn()
            .map_err(|error| AppError::Unavailable(format!("无法打开本地文件：{error}")))?;
        return Ok(());
    }
    use std::os::windows::ffi::OsStrExt as _;
    use std::ptr::{null, null_mut};
    use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
    let operation = "open\0".encode_utf16().collect::<Vec<_>>();
    let target = path
        .as_os_str()
        .encode_wide()
        .chain(std::iter::once(0))
        .collect::<Vec<_>>();
    let result = unsafe {
        ShellExecuteW(
            null_mut(),
            operation.as_ptr(),
            target.as_ptr(),
            null(),
            null(),
            SW_SHOWNORMAL,
        )
    } as isize;
    if result <= 32 {
        return Err(AppError::Unavailable(format!(
            "Windows 无法打开本地文件（ShellExecute 错误 {result}）"
        )));
    }
    Ok(())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn open_local_path_impl(path: &Path, application: Option<&str>) -> AppResult<()> {
    let executable = application.unwrap_or("xdg-open");
    std::process::Command::new(executable)
        .arg(path)
        .spawn()
        .map_err(|error| AppError::Unavailable(format!("无法打开本地文件：{error}")))?;
    Ok(())
}

pub(crate) fn validate_application_path(application: &str) -> AppResult<()> {
    let path = Path::new(application);
    if !path.is_absolute() {
        return Err(AppError::Validation("外部编辑器必须是本地绝对路径".into()));
    }
    #[cfg(target_os = "macos")]
    if !application.ends_with(".app") || !path.is_dir() {
        return Err(AppError::Validation(
            "外部编辑器必须选择已安装的 macOS 应用".into(),
        ));
    }
    #[cfg(target_os = "windows")]
    {
        let extension = path
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or("")
            .to_ascii_lowercase();
        if !path.is_file() || !["exe", "com", "bat", "cmd"].contains(&extension.as_str()) {
            return Err(AppError::Validation(
                "外部编辑器必须选择已安装的 Windows 程序".into(),
            ));
        }
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    if !path.is_file() {
        return Err(AppError::Validation("外部编辑器不存在".into()));
    }
    Ok(())
}

pub fn reveal_local_file(path: &Path) -> AppResult<()> {
    if !path.is_file() {
        return Err(AppError::Validation("要显示的本地文件不存在".into()));
    }
    reveal_local_file_impl(path)
}

#[cfg(target_os = "macos")]
fn reveal_local_file_impl(path: &Path) -> AppResult<()> {
    let status = std::process::Command::new("/usr/bin/open")
        .arg("-R")
        .arg(path)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Unavailable("无法在 Finder 中显示文件".into()))
    }
}

#[cfg(target_os = "windows")]
fn reveal_local_file_impl(path: &Path) -> AppResult<()> {
    let status = std::process::Command::new("explorer.exe")
        .arg("/select,")
        .arg(path)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Unavailable(
            "无法在文件资源管理器中显示文件".into(),
        ))
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn reveal_local_file_impl(path: &Path) -> AppResult<()> {
    let parent = path.parent().unwrap_or(path);
    let status = std::process::Command::new("xdg-open")
        .arg(parent)
        .status()?;
    if status.success() {
        Ok(())
    } else {
        Err(AppError::Unavailable("无法在文件管理器中显示文件".into()))
    }
}

pub fn system_version() -> String {
    system_version_impl().unwrap_or_else(|| "unknown".into())
}

#[cfg(target_os = "macos")]
fn system_version_impl() -> Option<String> {
    command_version("/usr/bin/sw_vers", &["-productVersion"])
}

#[cfg(target_os = "windows")]
fn system_version_impl() -> Option<String> {
    command_version("cmd.exe", &["/D", "/C", "ver"])
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn system_version_impl() -> Option<String> {
    command_version("uname", &["-r"])
}

fn command_version(executable: &str, arguments: &[&str]) -> Option<String> {
    std::process::Command::new(executable)
        .args(arguments)
        .output()
        .ok()
        .filter(|output| output.status.success() && output.stdout.len() <= 4096)
        .and_then(|output| String::from_utf8(output.stdout).ok())
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capabilities_have_platform_neutral_labels_and_messages() {
        let value = capabilities();
        assert!(!value.display_name.is_empty());
        assert!(!value.credential_store_name.is_empty());
        assert!(!value.file_manager_name.is_empty());
        assert!(!value.rdp.message.is_empty());
        assert!(!value.serial.message.is_empty());
    }
}
