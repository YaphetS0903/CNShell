use crate::{
    error::{AppError, AppResult},
    models::{ConnectionProfile, RdpPreflight, TerminalSession, TerminalStatus},
    ssh::load_credential,
};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::Arc,
    thread::JoinHandle,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct RdpManager {
    children: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedRdpChild>>>>>,
    closing: Arc<Mutex<HashSet<String>>>,
}

const MAX_DIAGNOSTIC_BYTES: usize = 64 * 1024;

struct ManagedRdpChild {
    child: Child,
    diagnostics: Arc<Mutex<Vec<u8>>>,
    stderr_reader: Option<JoinHandle<()>>,
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
        let args = arguments(&profile);
        let child = spawn_helper(&executable, &args, &password)?;
        let id = Uuid::new_v4().to_string();
        let shared = Arc::new(Mutex::new(child));
        self.children.lock().insert(id.clone(), shared.clone());
        let manager = self.clone();
        let event_id = id.clone();
        std::thread::spawn(move || {
            let status = loop {
                match shared.lock().child.try_wait() {
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
            let stderr_reader = shared.lock().stderr_reader.take();
            if let Some(reader) = stderr_reader {
                let _ = reader.join();
            }
            let diagnostics = {
                let buffer = shared.lock().diagnostics.clone();
                let bytes = buffer.lock().clone();
                String::from_utf8_lossy(&bytes).into_owned()
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
                        Some(format!(
                            "FreeRDP 连接失败：{}",
                            diagnostic_message(&diagnostics, &status)
                        ))
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
            .child
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

fn spawn_helper(executable: &Path, args: &[String], password: &str) -> AppResult<ManagedRdpChild> {
    if password.contains(['\r', '\n']) || args.iter().any(|arg| arg.contains(['\r', '\n'])) {
        return Err(AppError::Validation(
            "RDP 连接参数和密码不能包含换行符".into(),
        ));
    }
    let mut child = Command::new(executable)
        .arg("/args-from:stdin")
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| AppError::Unavailable(format!("无法启动 FreeRDP：{error}")))?;
    let result = child
        .stdin
        .take()
        .ok_or_else(|| AppError::Unavailable("FreeRDP 未提供安全参数输入通道".into()))
        .and_then(|mut stdin| {
            for arg in args {
                writeln!(stdin, "{arg}").map_err(|error| {
                    AppError::Unavailable(format!("无法向 FreeRDP 安全传递连接参数：{error}"))
                })?;
            }
            writeln!(stdin, "/p:{password}").map_err(|error| {
                AppError::Unavailable(format!("无法向 FreeRDP 安全传递密码：{error}"))
            })
        });
    if let Err(error) = result {
        let _ = child.kill();
        return Err(error);
    }
    let stderr = child
        .stderr
        .take()
        .ok_or_else(|| AppError::Unavailable("FreeRDP 未提供诊断输出通道".into()))?;
    let diagnostics = Arc::new(Mutex::new(Vec::new()));
    let output = diagnostics.clone();
    let stderr_reader = std::thread::spawn(move || {
        let mut stderr = stderr;
        let mut chunk = [0_u8; 4096];
        while let Ok(size) = stderr.read(&mut chunk) {
            if size == 0 {
                break;
            }
            append_diagnostic(&output, &chunk[..size]);
        }
    });
    Ok(ManagedRdpChild {
        child,
        diagnostics,
        stderr_reader: Some(stderr_reader),
    })
}

fn append_diagnostic(output: &Mutex<Vec<u8>>, chunk: &[u8]) {
    let mut output = output.lock();
    if chunk.len() >= MAX_DIAGNOSTIC_BYTES {
        output.clear();
        output.extend_from_slice(&chunk[chunk.len() - MAX_DIAGNOSTIC_BYTES..]);
        return;
    }
    let overflow = output
        .len()
        .saturating_add(chunk.len())
        .saturating_sub(MAX_DIAGNOSTIC_BYTES);
    if overflow > 0 {
        output.drain(..overflow);
    }
    output.extend_from_slice(chunk);
}

fn diagnostic_message(diagnostics: &str, status: &ExitStatus) -> String {
    let known = [
        (
            "NTLM support not available",
            "Windows NLA/NTLM 认证组件不可用，请重新安装 CNshell",
        ),
        (
            "ERRCONNECT_LOGON_FAILURE",
            "Windows 拒绝登录，请检查用户名、密码和域",
        ),
        ("ERRCONNECT_ACCOUNT_LOCKED_OUT", "Windows 账户已被锁定"),
        ("ERRCONNECT_ACCOUNT_EXPIRED", "Windows 账户已过期"),
        ("ERRCONNECT_PASSWORD_EXPIRED", "Windows 密码已过期"),
        ("ERRCONNECT_DNS_NAME_NOT_FOUND", "无法解析 Windows 主机地址"),
        ("ERRCONNECT_TLS_CONNECT_FAILED", "无法建立安全的 RDP 连接"),
        (
            "ERRCONNECT_CONNECT_TRANSPORT_FAILED",
            "目标端口未返回有效的 RDP 协商响应，请确认 Windows 已开启远程桌面且端口正确",
        ),
    ];
    known
        .into_iter()
        .find_map(|(needle, message)| diagnostics.contains(needle).then_some(message.into()))
        .unwrap_or_else(|| format!("Helper 异常退出（{status}）"))
}

fn arguments(profile: &ConnectionProfile) -> Vec<String> {
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
    args.push("/log-level:WARN".into());
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
            certificate_path: None,
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
    fn freerdp_args_enable_dynamic_resolution_without_password() {
        let args = arguments(&profile());
        assert!(args.contains(&"+dynamic-resolution".into()));
        assert!(!args.iter().any(|arg| arg.starts_with("/p:")));
    }

    #[test]
    fn sdl_args_use_freerdp_3_syntax() {
        let args = arguments(&profile());
        assert!(args.contains(&"+clipboard".into()));
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
    fn helper_receives_all_arguments_only_through_stdin() {
        use std::os::unix::fs::PermissionsExt;
        let directory = tempfile::tempdir().unwrap();
        let script = directory.path().join("fake-freerdp");
        let args_file = directory.path().join("args.txt");
        let stdin_file = directory.path().join("stdin.txt");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\ncat > '{}'\n",
                args_file.display(),
                stdin_file.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&script).unwrap().permissions();
        permissions.set_mode(0o700);
        std::fs::set_permissions(&script, permissions).unwrap();
        let args = arguments(&profile());
        let mut child = spawn_helper(&script, &args, "top-secret-password").unwrap();
        assert!(child.child.wait().unwrap().success());
        child.stderr_reader.take().unwrap().join().unwrap();
        let recorded_args = std::fs::read_to_string(args_file).unwrap();
        assert!(!recorded_args.contains("top-secret-password"));
        assert_eq!(recorded_args.trim(), "/args-from:stdin");
        let recorded_stdin = std::fs::read_to_string(stdin_file).unwrap();
        assert!(recorded_stdin.contains("/v:host:3389\n"));
        assert!(recorded_stdin.contains("/p:top-secret-password\n"));
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
        let status = child.lock().child.wait().unwrap();
        assert!(!status.success());
        assert!(!manager.children.lock().contains_key("session"));
    }

    #[test]
    fn diagnostics_translate_authentication_and_transport_failures() {
        let status = Command::new("sh")
            .args(["-c", "exit 147"])
            .status()
            .unwrap();
        assert_eq!(
            diagnostic_message("ERRCONNECT_LOGON_FAILURE", &status),
            "Windows 拒绝登录，请检查用户名、密码和域"
        );
        assert_eq!(
            diagnostic_message("ERRCONNECT_CONNECT_TRANSPORT_FAILED", &status),
            "目标端口未返回有效的 RDP 协商响应，请确认 Windows 已开启远程桌面且端口正确"
        );
    }

    #[test]
    fn diagnostic_buffer_keeps_only_the_latest_bounded_output() {
        let output = Mutex::new(vec![b'a'; MAX_DIAGNOSTIC_BYTES - 2]);
        append_diagnostic(&output, b"tail");
        let output = output.into_inner();
        assert_eq!(output.len(), MAX_DIAGNOSTIC_BYTES);
        assert!(output.ends_with(b"tail"));
    }
}
