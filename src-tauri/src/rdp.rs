use crate::{
    bookmark::PrivateKeyAccess,
    error::{AppError, AppResult},
    models::{
        ConnectionProfile, RdpConnectionOptions, RdpDisplay, RdpPreflight, TerminalSession,
        TerminalStatus,
    },
    ssh::load_credential,
};
use parking_lot::Mutex;
use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
    thread::JoinHandle,
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter, Manager};
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
    runtime_event: Arc<AtomicU8>,
    _drive_access: Option<PrivateKeyAccess>,
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

pub fn default_options(connection_id: String) -> RdpConnectionOptions {
    RdpConnectionOptions {
        connection_id,
        display_mode: "window".into(),
        display_id: None,
        scale_mode: "dynamic".into(),
        quality: "auto".into(),
        clipboard: true,
        audio_mode: "off".into(),
        microphone: false,
        drive_path: None,
    }
}

pub fn validate_options(options: &RdpConnectionOptions) -> AppResult<()> {
    if options.connection_id.trim().is_empty() || options.connection_id.len() > 128 {
        return Err(AppError::Validation("RDP 设置的连接 ID 无效".into()));
    }
    if !["window", "fullscreen"].contains(&options.display_mode.as_str()) {
        return Err(AppError::Validation("RDP 显示模式无效".into()));
    }
    if !["dynamic", "fit", "native"].contains(&options.scale_mode.as_str()) {
        return Err(AppError::Validation("RDP 缩放模式无效".into()));
    }
    if !["auto", "lowBandwidth", "balanced", "highQuality"].contains(&options.quality.as_str()) {
        return Err(AppError::Validation("RDP 画质模式无效".into()));
    }
    if !["off", "local", "remote"].contains(&options.audio_mode.as_str()) {
        return Err(AppError::Validation("RDP 音频模式无效".into()));
    }
    if options.display_id.is_some_and(|id| id > 64) {
        return Err(AppError::Validation("RDP 显示器编号无效".into()));
    }
    if let Some(path) = options.drive_path.as_deref() {
        let path = Path::new(path);
        if !path.is_absolute()
            || !path.is_dir()
            || path.as_os_str().len() > 4096
            || path
                .to_string_lossy()
                .chars()
                .any(|character| character.is_control() || character == ',')
        {
            return Err(AppError::Validation(
                "RDP 映射目录必须是存在且不含逗号或控制字符的本地绝对文件夹".into(),
            ));
        }
    }
    Ok(())
}

pub fn displays() -> AppResult<Vec<RdpDisplay>> {
    let executable = helper_path().ok_or_else(|| AppError::Unavailable(preflight().message))?;
    let output = Command::new(executable)
        .arg("/list:monitor")
        .output()
        .map_err(|error| AppError::Unavailable(format!("无法读取 FreeRDP 显示器列表：{error}")))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));
    match parse_displays(&text) {
        Ok(displays) => Ok(displays),
        Err(error) if !output.status.success() => Err(AppError::Unavailable(format!(
            "FreeRDP 显示器检测失败：{}（{error}）",
            output.status
        ))),
        Err(error) => Err(error),
    }
}

fn parse_displays(output: &str) -> AppResult<Vec<RdpDisplay>> {
    let pattern = regex::Regex::new(
        r"(?m)^\s*(\*)?\s*\[(\d{1,2})\]\s*\[([^\]\r\n]{1,256})\]\s+(\d{1,6})x(\d{1,6})",
    )
    .map_err(|error| AppError::Internal(error.to_string()))?;
    let displays = pattern
        .captures_iter(output)
        .take(16)
        .filter_map(|capture| {
            Some(RdpDisplay {
                id: capture.get(2)?.as_str().parse().ok()?,
                name: capture.get(3)?.as_str().trim().to_owned(),
                width: capture.get(4)?.as_str().parse().ok()?,
                height: capture.get(5)?.as_str().parse().ok()?,
                primary: capture.get(1).is_some(),
            })
        })
        .collect::<Vec<_>>();
    if displays.is_empty() {
        return Err(AppError::Unavailable(
            "FreeRDP 没有返回可识别的本机显示器".into(),
        ));
    }
    Ok(displays)
}

impl RdpManager {
    pub fn open(
        &self,
        app: AppHandle,
        profile: ConnectionProfile,
        options: RdpConnectionOptions,
    ) -> AppResult<TerminalSession> {
        validate_options(&options)?;
        if options.connection_id != profile.id {
            return Err(AppError::Validation("RDP 设置与连接不匹配".into()));
        }
        let executable = helper_path().ok_or_else(|| AppError::Unavailable(preflight().message))?;
        let reports_online_marker = std::env::current_exe()
            .ok()
            .and_then(|current| bundled_helper_path_for(&current))
            .is_some_and(|bundled| bundled == executable);
        let password = zeroize::Zeroizing::new(
            load_credential(&profile.id)?
                .ok_or_else(|| AppError::Authentication("Keychain 中没有保存 RDP 密码".into()))?,
        );
        let drive_access = options
            .drive_path
            .as_deref()
            .map(|path| crate::bookmark::access_rdp_drive(&profile.id, Path::new(path)))
            .transpose()?;
        let position = app
            .get_webview_window("main")
            .and_then(|window| window.outer_position().ok())
            .map(|position| (position.x.saturating_add(36), position.y.saturating_add(36)));
        let args = arguments(
            &profile,
            &options,
            drive_access.as_ref().map(PrivateKeyAccess::path),
            position,
        )?;
        let child = spawn_helper(&executable, &args, password.as_str(), drive_access)?;
        let id = Uuid::new_v4().to_string();
        let shared = Arc::new(Mutex::new(child));
        let runtime_event = shared.lock().runtime_event.clone();
        self.children.lock().insert(id.clone(), shared.clone());
        let manager = self.clone();
        let event_id = id.clone();
        std::thread::spawn(move || {
            let started = Instant::now();
            let mut online_emitted = false;
            let mut runtime_status = "connecting";
            let status = loop {
                match shared.lock().child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        if !online_emitted
                            && !reports_online_marker
                            && started.elapsed() >= Duration::from_secs(10)
                        {
                            online_emitted = true;
                            let _ = app.emit(
                                "terminal-status",
                                TerminalStatus {
                                    session_id: event_id.clone(),
                                    status: "online".into(),
                                    last_error: None,
                                    attempt: None,
                                },
                            );
                            runtime_status = "online";
                        }
                        match runtime_event.swap(0, Ordering::AcqRel) {
                            1 if online_emitted && runtime_status != "reconnecting" => {
                                runtime_status = "reconnecting";
                                let _ = app.emit(
                                    "terminal-status",
                                    TerminalStatus {
                                        session_id: event_id.clone(),
                                        status: "reconnecting".into(),
                                        last_error: None,
                                        attempt: Some(1),
                                    },
                                );
                            }
                            2 if runtime_status != "online" => {
                                online_emitted = true;
                                runtime_status = "online";
                                let _ = app.emit(
                                    "terminal-status",
                                    TerminalStatus {
                                        session_id: event_id.clone(),
                                        status: "online".into(),
                                        last_error: None,
                                        attempt: None,
                                    },
                                );
                            }
                            _ => {}
                        }
                        std::thread::sleep(Duration::from_millis(200));
                    }
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
            let successful = status.success() || expected_window_close(&status);
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
            status: "connecting".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
        })
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let child = self
            .children
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("RDP 会话 {id}")))?;
        self.closing.lock().insert(id.into());
        match child.lock().child.kill() {
            Ok(()) => {
                self.children.lock().remove(id);
                Ok(())
            }
            Err(error) => {
                self.closing.lock().remove(id);
                Err(AppError::Unavailable(format!("无法关闭 FreeRDP：{error}")))
            }
        }
    }

    pub fn close_all(&self) {
        let ids = self.children.lock().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let _ = self.close(&id);
        }
    }

    pub fn focus(&self, id: &str) -> AppResult<()> {
        self.with_pid(id, focus_process)
    }

    pub fn hide(&self, id: &str) -> AppResult<()> {
        self.with_pid(id, hide_process)
    }

    fn with_pid(&self, id: &str, action: fn(u32) -> AppResult<()>) -> AppResult<()> {
        let child = self
            .children
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("RDP 会话 {id}")))?;
        action(child.lock().child.id())
    }
}

fn spawn_helper(
    executable: &Path,
    args: &[String],
    password: &str,
    drive_access: Option<PrivateKeyAccess>,
) -> AppResult<ManagedRdpChild> {
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
    let runtime_event = Arc::new(AtomicU8::new(0));
    let event_output = runtime_event.clone();
    let stderr_reader = std::thread::spawn(move || {
        let mut stderr = stderr;
        let mut chunk = [0_u8; 4096];
        let mut pending = Vec::new();
        while let Ok(size) = stderr.read(&mut chunk) {
            if size == 0 {
                break;
            }
            append_diagnostic(&output, &chunk[..size]);
            pending.extend_from_slice(&chunk[..size]);
            while let Some(newline) = pending.iter().position(|byte| *byte == b'\n') {
                let line = String::from_utf8_lossy(&pending[..newline]);
                if let Some(event) = runtime_event_from_line(&line) {
                    event_output.store(event, Ordering::Release);
                }
                pending.drain(..=newline);
            }
            if pending.len() > 4096 {
                pending.drain(..pending.len() - 4096);
            }
        }
    });
    Ok(ManagedRdpChild {
        child,
        diagnostics,
        stderr_reader: Some(stderr_reader),
        runtime_event,
        _drive_access: drive_access,
    })
}

fn runtime_event_from_line(line: &str) -> Option<u8> {
    let line = line.to_ascii_lowercase();
    if line.contains("cnshell_rdp_state=online")
        || line.contains("connection_state_active")
        || line.contains("connection state active")
        || line.contains("connected to")
    {
        Some(2)
    } else if line.contains("auto-reconnect")
        || line.contains("attempting reconnect")
        || line.contains("reconnecting")
        || line.contains("transport failure")
    {
        Some(1)
    } else {
        None
    }
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

fn expected_window_close(status: &ExitStatus) -> bool {
    status.code() == Some(131)
}

fn arguments(
    profile: &ConnectionProfile,
    options: &RdpConnectionOptions,
    drive_path: Option<&Path>,
    window_position: Option<(i32, i32)>,
) -> AppResult<Vec<String>> {
    validate_options(options)?;
    let target = if profile.host.contains(':') && !profile.host.starts_with('[') {
        format!("[{}]:{}", profile.host, profile.port)
    } else {
        format!("{}:{}", profile.host, profile.port)
    };
    let mut args = vec![
        format!("/v:{target}"),
        format!("/u:{}", profile.username),
        "/cert:tofu".into(),
        "+auto-reconnect".into(),
    ];
    match options.scale_mode.as_str() {
        "dynamic" => args.push("+dynamic-resolution".into()),
        "fit" => args.push("/smart-sizing".into()),
        "native" => {}
        _ => unreachable!("validated RDP scale mode"),
    }
    if options.clipboard {
        args.push("/clipboard:direction-to:all,files-to:off".into());
    } else {
        args.push("-clipboard".into());
    }
    match options.display_mode.as_str() {
        "fullscreen" => {
            args.push("+f".into());
            args.push("/floatbar:sticky:on,default:visible,show:fullscreen".into());
            if let Some(display_id) = options.display_id {
                args.push(format!("/monitors:{display_id}"));
            }
        }
        "window" => {
            if let Some((x, y)) = window_position {
                args.push(format!("/window-position:{x}x{y}"));
            }
        }
        _ => unreachable!("validated RDP display mode"),
    }
    match options.quality.as_str() {
        "auto" => args.push("/network:auto".into()),
        "lowBandwidth" => {
            args.extend([
                "/network:modem".into(),
                "-wallpaper".into(),
                "-themes".into(),
                "-menu-anims".into(),
                "-window-drag".into(),
            ]);
        }
        "balanced" => args.push("/network:broadband-high".into()),
        "highQuality" => args.push("/network:lan".into()),
        _ => unreachable!("validated RDP quality mode"),
    }
    match options.audio_mode.as_str() {
        "off" => args.push("/audio-mode:none".into()),
        "local" => {
            args.push("/audio-mode:redirect".into());
            args.push("/sound".into());
        }
        "remote" => args.push("/audio-mode:server".into()),
        _ => unreachable!("validated RDP audio mode"),
    }
    if options.microphone {
        args.push("/microphone".into());
    }
    if let Some(path) = drive_path {
        if !path.is_absolute() || !path.is_dir() {
            return Err(AppError::Validation("RDP 映射目录授权已失效".into()));
        }
        args.push(format!("/drive:CNshell,{}", path.to_string_lossy()));
    }
    args.push("/log-level:INFO".into());
    Ok(args)
}

#[cfg(target_os = "macos")]
fn focus_process(pid: u32) -> AppResult<()> {
    use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication};
    let application = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
        .ok_or_else(|| AppError::Unavailable("FreeRDP 窗口尚未完成系统注册".into()))?;
    let _ = application.unhide();
    if !application.activateWithOptions(NSApplicationActivationOptions::ActivateAllWindows) {
        return Err(AppError::Unavailable("macOS 未能激活 FreeRDP 窗口".into()));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn focus_process(_pid: u32) -> AppResult<()> {
    Err(AppError::Unavailable("RDP 窗口联动仅支持 macOS".into()))
}

#[cfg(target_os = "macos")]
fn hide_process(pid: u32) -> AppResult<()> {
    use objc2_app_kit::NSRunningApplication;
    let application = NSRunningApplication::runningApplicationWithProcessIdentifier(pid as i32)
        .ok_or_else(|| AppError::Unavailable("FreeRDP 窗口尚未完成系统注册".into()))?;
    if !application.hide() {
        return Err(AppError::Unavailable("macOS 未能隐藏 FreeRDP 窗口".into()));
    }
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn hide_process(_pid: u32) -> AppResult<()> {
    Err(AppError::Unavailable("RDP 窗口联动仅支持 macOS".into()))
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

    fn options() -> RdpConnectionOptions {
        default_options("1".into())
    }

    #[test]
    fn freerdp_args_enable_dynamic_resolution_without_password() {
        let args = arguments(&profile(), &options(), None, None).unwrap();
        assert!(args.contains(&"+dynamic-resolution".into()));
        assert!(!args.iter().any(|arg| arg.starts_with("/p:")));
    }

    #[test]
    fn sdl_args_use_freerdp_3_syntax() {
        let args = arguments(&profile(), &options(), None, None).unwrap();
        assert!(args.contains(&"/clipboard:direction-to:all,files-to:off".into()));
        assert!(args.contains(&"/u:user".into()));
        assert!(args.contains(&"/v:host:3389".into()));
        assert!(!args.iter().any(|arg| arg.starts_with("--")));
    }

    #[test]
    fn advanced_options_map_to_bounded_freerdp_arguments() {
        let directory = tempfile::tempdir().unwrap();
        let mut options = options();
        options.display_mode = "fullscreen".into();
        options.display_id = Some(1);
        options.scale_mode = "fit".into();
        options.quality = "lowBandwidth".into();
        options.clipboard = false;
        options.audio_mode = "local".into();
        options.microphone = true;
        options.drive_path = Some(directory.path().to_string_lossy().into_owned());
        let args = arguments(&profile(), &options, Some(directory.path()), None).unwrap();
        for expected in [
            "+f",
            "/monitors:1",
            "/smart-sizing",
            "/network:modem",
            "-clipboard",
            "/audio-mode:redirect",
            "/sound",
            "/microphone",
        ] {
            assert!(args.contains(&expected.to_owned()), "missing {expected}");
        }
        assert!(args.iter().any(|arg| arg.starts_with("/drive:CNshell,")));
        assert!(!args.iter().any(|arg| arg.starts_with("/p:")));
    }

    #[test]
    fn display_parser_uses_freerdp_monitor_ids_and_primary_marker() {
        let displays = parse_displays(
            "listing 2 monitors:\n * [1] [Built-in Retina Display] 1352x878 +0+0\n   [2] [Studio Display] 2560x1440 +1352+0\n",
        )
        .unwrap();
        assert_eq!(displays.len(), 2);
        assert_eq!(displays[0].id, 1);
        assert!(displays[0].primary);
        assert_eq!(displays[1].name, "Studio Display");
        assert_eq!((displays[1].width, displays[1].height), (2560, 1440));
    }

    #[test]
    fn runtime_logs_distinguish_reconnecting_and_active_states() {
        assert_eq!(
            runtime_event_from_line("[WARN] transport failure, auto-reconnect"),
            Some(1)
        );
        assert_eq!(
            runtime_event_from_line("transition CONNECTION_STATE_ACTIVE"),
            Some(2)
        );
        assert_eq!(runtime_event_from_line("certificate accepted"), None);
    }

    #[test]
    fn drive_mapping_rejects_ambiguous_freerdp_delimiters() {
        let mut options = options();
        options.drive_path = Some("/tmp/folder,other".into());
        assert!(matches!(
            validate_options(&options),
            Err(AppError::Validation(_))
        ));
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
        let args = arguments(&profile(), &options(), None, None).unwrap();
        let mut child = spawn_helper(&script, &args, "top-secret-password", None).unwrap();
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
            spawn_helper(Path::new("/missing/helper"), &[], "line1\nline2", None),
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
        let child = Arc::new(Mutex::new(
            spawn_helper(&script, &[], "secret", None).unwrap(),
        ));
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
    fn sdl_manual_window_close_is_not_reported_as_a_crash() {
        let status = Command::new("sh")
            .args(["-c", "exit 131"])
            .status()
            .unwrap();
        assert!(expected_window_close(&status));
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
