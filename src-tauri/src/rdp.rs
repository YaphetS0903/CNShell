use crate::{
    error::{AppError, AppResult},
    models::{ConnectionProfile, RdpPreflight, TerminalSession, TerminalStatus},
    ssh::load_credential,
};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::Arc,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct RdpManager {
    children: Arc<Mutex<HashMap<String, Arc<Mutex<Child>>>>>,
    closing: Arc<Mutex<HashSet<String>>>,
}

pub fn preflight() -> RdpPreflight {
    match helper_path() {
        Some(path) => RdpPreflight {
            available: true,
            executable: Some(path.to_string_lossy().into_owned()),
            message: format!(
                "已检测到 {}，可打开受管 RDP 窗口",
                path.file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("FreeRDP")
            ),
        },
        None => RdpPreflight {
            available: false,
            executable: None,
            message: "CNshell 内置的 FreeRDP 组件缺失或损坏，请重新安装 CNshell。".into(),
        },
    }
}

const HELPER_NAMES: [&str; 3] = ["sdl-freerdp", "xfreerdp", "wlfreerdp"];

fn bundled_helper_path_for(executable: &Path) -> Option<PathBuf> {
    executable
        .parent()?
        .parent()
        .map(|contents| contents.join("Resources/freerdp/sdl-freerdp"))
}

fn helper_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| bundled_helper_path_for(&executable))
        .filter(|path| path.is_file())
        .or_else(|| {
            std::env::var_os("CNSHELL_FREERDP_HELPER")
                .map(PathBuf::from)
                .filter(|path| path.is_file())
        })
        .or_else(|| {
            ["/opt/homebrew/bin", "/usr/local/bin"]
                .into_iter()
                .flat_map(|directory| {
                    HELPER_NAMES
                        .into_iter()
                        .map(move |name| Path::new(directory).join(name))
                })
                .find(|path| path.is_file())
        })
        .or_else(|| {
            HELPER_NAMES
                .into_iter()
                .find_map(|candidate| which::which(candidate).ok())
        })
}

impl RdpManager {
    pub fn open(&self, app: AppHandle, profile: ConnectionProfile) -> AppResult<TerminalSession> {
        let executable = helper_path().ok_or_else(|| AppError::Unavailable(preflight().message))?;
        let password = load_credential(&profile.id)?
            .ok_or_else(|| AppError::Authentication("Keychain 中没有保存 RDP 密码".into()))?;
        let args = arguments(&executable, &profile, true);
        let child = spawn_helper(&executable, &args, &password)?;
        let id = Uuid::new_v4().to_string();
        let shared = Arc::new(Mutex::new(child));
        self.children.lock().insert(id.clone(), shared.clone());
        let manager = self.clone();
        let event_id = id.clone();
        std::thread::spawn(move || {
            let status = loop {
                match shared.lock().try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => std::thread::sleep(std::time::Duration::from_millis(200)),
                    Err(error) => {
                        manager.children.lock().remove(&event_id);
                        let _ = app.emit(
                            "terminal-status",
                            TerminalStatus {
                                session_id: event_id,
                                status: "failed".into(),
                                last_error: Some(format!("FreeRDP 状态读取失败：{error}")),
                                attempt: None,
                            },
                        );
                        return;
                    }
                }
            };
            manager.children.lock().remove(&event_id);
            let requested_close = manager.closing.lock().remove(&event_id);
            let successful = status.success();
            let _ = app.emit(
                "terminal-status",
                TerminalStatus {
                    session_id: event_id,
                    status: if successful || requested_close {
                        "closed"
                    } else {
                        "failed"
                    }
                    .into(),
                    last_error: if successful || requested_close {
                        None
                    } else {
                        Some(format!("FreeRDP 异常退出：{status}"))
                    },
                    attempt: None,
                },
            );
        });
        Ok(TerminalSession {
            id,
            connection_id: profile.id,
            session_type: "rdp".into(),
            title: profile.name,
            status: "online".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
        })
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let child = self
            .children
            .lock()
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("RDP 会话 {id}")))?;
        self.closing.lock().insert(id.into());
        child
            .lock()
            .kill()
            .map_err(|error| AppError::Unavailable(format!("无法关闭 FreeRDP：{error}")))
    }

    pub fn close_all(&self) {
        let ids = self.children.lock().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let _ = self.close(&id);
        }
    }
}

fn spawn_helper(executable: &Path, args: &[String], password: &str) -> AppResult<Child> {
    if password.contains(['\r', '\n']) {
        return Err(AppError::Validation("RDP 密码不能包含换行符".into()));
    }
    let mut child = Command::new(executable)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| AppError::Unavailable(format!("无法启动 FreeRDP：{error}")))?;
    let result = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Unavailable("FreeRDP 未提供密码输入通道".into()))
        .and_then(|mut stdin| {
            stdin
                .write_all(format!("{password}\n").as_bytes())
                .map_err(|error| {
                    AppError::Unavailable(format!("无法向 FreeRDP 安全传递密码：{error}"))
                })
        });
    if let Err(error) = result {
        let _ = child.kill();
        return Err(error);
    }
    Ok(child)
}

fn arguments(executable: &Path, profile: &ConnectionProfile, password_stdin: bool) -> Vec<String> {
    let _ = executable;
    let target = if profile.host.contains(':') && !profile.host.starts_with('[') {
        format!("[{}]:{}", profile.host, profile.port)
    } else {
        format!("{}:{}", profile.host, profile.port)
    };
    let mut args = vec![
        format!("/v:{target}"),
        format!("/u:{}", profile.username),
        "/cert:tofu".into(),
        "+dynamic-resolution".into(),
        "+clipboard".into(),
        "+auto-reconnect".into(),
    ];
    if password_stdin {
        args.push("/from-stdin".into());
    }
    args
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> ConnectionProfile {
        ConnectionProfile {
            id: "1".into(),
            folder_id: None,
            protocol: "rdp".into(),
            name: "win".into(),
            host: "host".into(),
            port: 3389,
            username: "user".into(),
            auth_type: "password".into(),
            private_key_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: true,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        }
    }

    #[test]
    fn xfreerdp_args_enable_dynamic_resolution_and_password_stdin() {
        let args = arguments(Path::new("/usr/local/bin/xfreerdp"), &profile(), true);
        assert!(args.contains(&"+dynamic-resolution".into()));
        assert!(args.contains(&"/from-stdin".into()));
        assert!(!args.iter().any(|arg| arg.starts_with("/p:")));
    }

    #[test]
    fn sdl_args_use_freerdp_3_syntax_and_password_stdin() {
        let args = arguments(Path::new("sdl-freerdp"), &profile(), true);
        assert!(args.contains(&"+clipboard".into()));
        assert!(args.contains(&"/from-stdin".into()));
        assert!(args.contains(&"/u:user".into()));
        assert!(args.contains(&"/v:host:3389".into()));
        assert!(!args.iter().any(|arg| arg.starts_with("--")));
    }

    #[test]
    fn native_sdl_helper_is_preferred_over_x11_and_wayland() {
        assert_eq!(HELPER_NAMES, ["sdl-freerdp", "xfreerdp", "wlfreerdp"]);
    }

    #[test]
    fn bundled_helper_uses_the_macos_resource_directory() {
        let executable = Path::new("/Applications/CNshell.app/Contents/MacOS/cnshell");
        assert_eq!(
            bundled_helper_path_for(executable),
            Some(PathBuf::from(
                "/Applications/CNshell.app/Contents/Resources/freerdp/sdl-freerdp"
            ))
        );
    }

    #[cfg(unix)]
    #[test]
    fn helper_receives_password_only_through_stdin() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-freerdp");
        let args_file = directory.path().join("args.txt");
        let stdin_file = directory.path().join("stdin.txt");
        std::fs::write(&script, format!("#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nIFS= read -r secret\nprintf '%s' \"$secret\" > '{}'\n", args_file.display(), stdin_file.display())).unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        let args = arguments(Path::new("xfreerdp"), &profile(), true);
        let mut child = spawn_helper(&script, &args, "top-secret-password").unwrap();
        assert!(child.wait().unwrap().success());
        let recorded_args = std::fs::read_to_string(args_file).unwrap();
        assert!(!recorded_args.contains("top-secret-password"));
        assert_eq!(
            std::fs::read_to_string(stdin_file).unwrap(),
            "top-secret-password"
        );
    }

    #[test]
    fn helper_rejects_multiline_passwords_before_spawn() {
        assert!(matches!(
            spawn_helper(Path::new("/missing/helper"), &[], "line1\nline2"),
            Err(AppError::Validation(_))
        ));
    }

    #[cfg(unix)]
    #[test]
    fn manager_close_terminates_a_managed_helper() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("sleep-helper");
        std::fs::write(&script, "#!/bin/sh\nread secret\nsleep 30\n").unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        let child = Arc::new(Mutex::new(spawn_helper(&script, &[], "secret").unwrap()));
        let manager = RdpManager::default();
        manager
            .children
            .lock()
            .insert("session".into(), child.clone());
        manager.close("session").unwrap();
        let status = child.lock().wait().unwrap();
        assert!(!status.success());
        assert!(!manager.children.lock().contains_key("session"));
    }
}
