use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{ConnectionProfile, TerminalOutput, TerminalSession, TerminalStatus},
    session_log::SessionLogManager,
    ssh::{SessionManager, verified_connection},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use regex::Regex;
use std::{
    collections::{HashMap, HashSet},
    io::{ErrorKind, Read, Write},
    path::PathBuf,
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

const START_TIMEOUT: Duration = Duration::from_secs(20);
const MAX_START_OUTPUT: usize = 64 * 1024;
const DEFAULT_PORT_START: u16 = 60000;
const DEFAULT_PORT_END: u16 = 60010;
const RECONNECT_TAIL_BYTES: usize = 512;
const RECONNECT_RECOVERY_DELAY: Duration = Duration::from_secs(2);
const UNAVAILABLE_MESSAGES: [&[u8]; 6] = [
    b"Nothing received from server on UDP port",
    b"Timed out waiting for server",
    b"without contact",
    b"Last contact",
    b"Last reply",
    b"did not make a successful connection",
];
const UDP_WAITING_MESSAGE: &str =
    "Mosh 正在等待 UDP 响应；请确认本机 VPN/代理允许 UDP，并检查云安全组和服务器 UDP 端口";
const UDP_FAILED_MESSAGE: &str = "Mosh UDP 连接失败；请暂时关闭 VPN/代理或将该服务器设为直连，并确认云安全组放行所配置的 UDP 端口范围";

struct ManagedMosh {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    finished: bool,
}

#[derive(Clone, Default)]
pub struct MoshManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedMosh>>>>>,
    closing: Arc<Mutex<HashSet<String>>>,
}

#[derive(Eq, PartialEq)]
struct ServerConnection {
    port: u16,
    key: String,
}

#[derive(Default)]
struct ReconnectDetector {
    tail: Vec<u8>,
    last_unavailable: Option<Instant>,
}

impl ReconnectDetector {
    fn observe(&mut self, bytes: &[u8], now: Instant) -> ReconnectObservation {
        let previous_tail_len = self.tail.len();
        let mut combined = Vec::with_capacity(self.tail.len() + bytes.len());
        combined.extend_from_slice(&self.tail);
        combined.extend_from_slice(bytes);
        let unavailable = UNAVAILABLE_MESSAGES.iter().any(|message| {
            combined
                .windows(message.len())
                .enumerate()
                .any(|(start, window)| {
                    window == *message && start.saturating_add(message.len()) > previous_tail_len
                })
        });
        let keep = combined.len().min(RECONNECT_TAIL_BYTES);
        self.tail = combined[combined.len() - keep..].to_vec();
        if unavailable {
            self.last_unavailable = Some(now);
            ReconnectObservation::Unavailable
        } else if self
            .last_unavailable
            .is_some_and(|last| now.saturating_duration_since(last) >= RECONNECT_RECOVERY_DELAY)
        {
            self.last_unavailable = None;
            ReconnectObservation::Recovered
        } else {
            ReconnectObservation::Unchanged
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
enum ReconnectObservation {
    Unavailable,
    Recovered,
    Unchanged,
}

impl std::fmt::Debug for ServerConnection {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ServerConnection")
            .field("port", &self.port)
            .field("key", &"<redacted>")
            .finish()
    }
}

pub fn default_ports() -> (u16, u16) {
    (DEFAULT_PORT_START, DEFAULT_PORT_END)
}

pub fn validate_ports(start: u16, end: u16) -> AppResult<()> {
    if start < 1024 || end < start || end.saturating_sub(start) > 1000 {
        return Err(AppError::Validation(
            "Mosh UDP 端口必须在 1024–65535，且连续范围不能超过 1001 个端口".into(),
        ));
    }
    Ok(())
}

pub fn helper_path() -> Option<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|executable| bundled_helper_path(&executable))
        .filter(|path| path.is_file())
        .or_else(|| {
            std::env::var_os("CNSHELL_MOSH_CLIENT")
                .map(PathBuf::from)
                .filter(|path| path.is_file())
        })
        .or_else(|| {
            let name = if cfg!(target_os = "windows") {
                "mosh-client.exe"
            } else {
                "mosh-client"
            };
            let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("resources")
                .join("mosh")
                .join(name);
            path.is_file().then_some(path)
        })
        .or_else(|| {
            which::which(if cfg!(target_os = "windows") {
                "mosh-client.exe"
            } else {
                "mosh-client"
            })
            .ok()
        })
}

#[cfg(target_os = "macos")]
fn bundled_helper_path(executable: &std::path::Path) -> Option<PathBuf> {
    executable
        .parent()?
        .parent()
        .map(|contents| contents.join("Resources/mosh/mosh-client"))
}

#[cfg(target_os = "windows")]
fn bundled_helper_path(executable: &std::path::Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("mosh").join("mosh-client.exe"))
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn bundled_helper_path(executable: &std::path::Path) -> Option<PathBuf> {
    executable
        .parent()
        .map(|directory| directory.join("mosh").join("mosh-client"))
}

pub fn available() -> bool {
    helper_path().is_some()
}

fn connect_pattern() -> &'static Regex {
    static PATTERN: OnceLock<Regex> = OnceLock::new();
    PATTERN.get_or_init(|| {
        Regex::new(r"(?m)^MOSH CONNECT ([0-9]{1,5}) ([A-Za-z0-9/+]{22})\r?$")
            .expect("static Mosh connect pattern")
    })
}

fn parse_server_output(output: &str) -> AppResult<ServerConnection> {
    let connect = connect_pattern()
        .captures(output)
        .ok_or_else(|| remote_start_error(output))?;
    let port = connect[1]
        .parse::<u16>()
        .map_err(|_| AppError::Remote("Mosh 服务端返回的 UDP 端口无效".into()))?;
    if port < 1024 {
        return Err(AppError::Remote("Mosh 服务端返回了不安全的特权端口".into()));
    }
    Ok(ServerConnection {
        port,
        key: connect[2].to_owned(),
    })
}

fn remote_start_error(output: &str) -> AppError {
    let lower = output.to_ascii_lowercase();
    if lower.contains("command not found") || lower.contains("not found") {
        AppError::Unavailable("远端未安装 mosh-server，请先在服务器安装 Mosh".into())
    } else if lower.contains("bad udp port") || lower.contains("bind") {
        AppError::Remote("远端无法绑定指定 UDP 端口，请检查端口范围和防火墙".into())
    } else {
        AppError::Remote("Mosh 服务端启动失败，未收到有效握手".into())
    }
}

fn server_command(start: u16, end: u16) -> String {
    format!("exec mosh-server new -s -p {start}:{end} -c 256 -l LANG=C.UTF-8 2>&1")
}

async fn start_remote(
    db: &Database,
    profile: &ConnectionProfile,
    start: u16,
    end: u16,
) -> AppResult<ServerConnection> {
    validate_ports(start, end)?;
    let connected = verified_connection(db, profile, false).await?;
    tokio::task::spawn_blocking(move || {
        let mut channel = connected.session.channel_session()?;
        channel.exec(&server_command(start, end))?;
        connected.session.set_blocking(false);
        let started = Instant::now();
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        loop {
            match channel.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    if bytes.len().saturating_add(size) > MAX_START_OUTPUT {
                        let _ = channel.close();
                        return Err(AppError::Remote("Mosh 服务端启动输出超过 64 KB".into()));
                    }
                    bytes.extend_from_slice(&buffer[..size]);
                    let output = String::from_utf8_lossy(&bytes);
                    if connect_pattern().is_match(&output) {
                        let parsed = parse_server_output(&output)?;
                        let _ = channel.close();
                        return Ok(parsed);
                    }
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
            if channel.eof() {
                return Err(remote_start_error(&String::from_utf8_lossy(&bytes)));
            }
            if started.elapsed() >= START_TIMEOUT {
                let _ = channel.close();
                return Err(AppError::Unavailable(
                    "Mosh 服务端启动超时，请检查远端安装和 UDP 防火墙".into(),
                ));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

impl MoshManager {
    pub async fn open(
        &self,
        app: AppHandle,
        db: Database,
        ssh_sessions: SessionManager,
        profile: ConnectionProfile,
        logs: SessionLogManager,
        cols: u32,
        rows: u32,
        port_start: u16,
        port_end: u16,
    ) -> AppResult<TerminalSession> {
        validate_size(cols, rows)?;
        let executable = helper_path().ok_or_else(|| {
            AppError::Unavailable(
                "CNshell 内置的 mosh-client 缺失或损坏，请重新安装 CNshell".into(),
            )
        })?;
        let server = start_remote(&db, &profile, port_start, port_end).await?;
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::Internal(format!("创建 Mosh PTY 失败：{error}")))?;
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|error| AppError::Internal(format!("读取 Mosh PTY 失败：{error}")))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|error| AppError::Internal(format!("写入 Mosh PTY 失败：{error}")))?;
        let mut command = CommandBuilder::new(executable);
        command.arg(&profile.host);
        command.arg(server.port.to_string());
        command.env("MOSH_KEY", &server.key);
        command.env("MOSH_PREDICTION_DISPLAY", "adaptive");
        command.env("TERM", "xterm-256color");
        command.env("LANG", "en_US.UTF-8");
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| AppError::Unavailable(format!("无法启动 mosh-client：{error}")))?;
        drop(pty.slave);
        let id = Uuid::new_v4().to_string();
        let managed = Arc::new(Mutex::new(ManagedMosh {
            child,
            master: pty.master,
            writer,
            finished: false,
        }));
        self.sessions.lock().insert(id.clone(), managed.clone());
        ssh_sessions.insert_external(id.clone(), profile.clone());
        if let Some(startup) = profile.startup_command.as_deref() {
            if !startup.is_empty() {
                let mut handle = managed.lock();
                let startup_result = handle
                    .writer
                    .write_all(startup.as_bytes())
                    .and_then(|_| handle.writer.write_all(b"\r"))
                    .and_then(|_| handle.writer.flush());
                if let Err(error) = startup_result {
                    let _ = handle.child.kill();
                    drop(handle);
                    self.sessions.lock().remove(&id);
                    ssh_sessions.remove_external(&id);
                    return Err(error.into());
                }
            }
        }
        spawn_reader(
            app,
            self.clone(),
            ssh_sessions,
            logs,
            id.clone(),
            managed,
            reader,
        );
        Ok(TerminalSession {
            id,
            connection_id: profile.id,
            session_type: "mosh".into(),
            title: format!("{} · Mosh", profile.name),
            status: "online".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
        })
    }

    pub fn contains(&self, id: &str) -> bool {
        self.sessions.lock().contains_key(id)
    }

    pub fn input(&self, id: &str, data: &str) -> AppResult<()> {
        if data.len() > 1024 * 1024 {
            return Err(AppError::Validation("单次终端输入不能超过 1 MB".into()));
        }
        let session = self
            .sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Mosh 会话 {id}")))?;
        let mut session = session.lock();
        if session.finished {
            return Err(AppError::Unavailable("Mosh 会话已结束，请重新连接".into()));
        }
        session.writer.write_all(data.as_bytes())?;
        session.writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, id: &str, cols: u32, rows: u32) -> AppResult<()> {
        validate_size(cols, rows)?;
        let session = self
            .sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Mosh 会话 {id}")))?;
        let session = session.lock();
        if session.finished {
            return Err(AppError::Unavailable("Mosh 会话已结束，请重新连接".into()));
        }
        session
            .master
            .resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::Internal(format!("调整 Mosh PTY 失败：{error}")))
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let session = self
            .sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("Mosh 会话 {id}")))?;
        if session.lock().finished {
            self.sessions.lock().remove(id);
            self.closing.lock().remove(id);
            return Ok(());
        }
        self.closing.lock().insert(id.to_owned());
        if let Err(error) = session.lock().child.kill() {
            self.closing.lock().remove(id);
            return Err(AppError::Unavailable(format!("无法关闭 Mosh：{error}")));
        }
        Ok(())
    }

    pub fn close_all(&self) {
        let ids = self.sessions.lock().keys().cloned().collect::<Vec<_>>();
        for id in ids {
            let _ = self.close(&id);
        }
    }
}

fn validate_size(cols: u32, rows: u32) -> AppResult<()> {
    if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(AppError::Validation("PTY 尺寸超出允许范围".into()));
    }
    Ok(())
}

fn spawn_reader(
    app: AppHandle,
    manager: MoshManager,
    ssh_sessions: SessionManager,
    logs: SessionLogManager,
    id: String,
    session: Arc<Mutex<ManagedMosh>>,
    mut reader: Box<dyn Read + Send>,
) {
    std::thread::Builder::new()
        .name(format!("cnshell-mosh-{}", &id[..id.len().min(8)]))
        .spawn(move || {
            let mut buffer = [0_u8; 32 * 1024];
            let mut reconnecting = false;
            let mut detector = ReconnectDetector::default();
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let observation = detector.observe(&buffer[..size], Instant::now());
                        if observation == ReconnectObservation::Unavailable && !reconnecting {
                            reconnecting = true;
                            let _ = app.emit(
                                "terminal-status",
                                TerminalStatus {
                                    session_id: id.clone(),
                                    status: "reconnecting".into(),
                                    last_error: Some(UDP_WAITING_MESSAGE.into()),
                                    attempt: None,
                                },
                            );
                        } else if reconnecting && observation == ReconnectObservation::Recovered {
                            reconnecting = false;
                            let _ = app.emit(
                                "terminal-status",
                                TerminalStatus {
                                    session_id: id.clone(),
                                    status: "online".into(),
                                    last_error: None,
                                    attempt: None,
                                },
                            );
                        }
                        logs.write_output(&id, &buffer[..size]);
                        let _ = app.emit(
                            "terminal-output",
                            TerminalOutput {
                                session_id: id.clone(),
                                data_base64: STANDARD.encode(&buffer[..size]),
                            },
                        );
                    }
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            let status = {
                let mut session = session.lock();
                let status = session.child.wait();
                session.finished = true;
                status
            };
            let _ = logs.stop(&id);
            let requested = manager.closing.lock().remove(&id);
            cleanup_after_process_exit(&manager, &ssh_sessions, &id, requested);
            let successful = status.as_ref().is_ok_and(|status| status.success());
            let error = if requested || successful {
                None
            } else if reconnecting {
                Some(UDP_FAILED_MESSAGE.into())
            } else {
                Some(match status {
                    Ok(status) => format!("Mosh 已异常退出：{status}"),
                    Err(error) => format!("读取 Mosh 退出状态失败：{error}"),
                })
            };
            let _ = app.emit(
                "terminal-status",
                TerminalStatus {
                    session_id: id,
                    status: if requested || successful {
                        "closed"
                    } else {
                        "failed"
                    }
                    .into(),
                    last_error: error,
                    attempt: None,
                },
            );
        })
        .expect("Mosh reader thread starts");
}

fn cleanup_after_process_exit(
    manager: &MoshManager,
    ssh_sessions: &SessionManager,
    id: &str,
    requested: bool,
) {
    if requested {
        manager.sessions.lock().remove(id);
        ssh_sessions.remove_external(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ConnectionProtocolOptions;
    use std::collections::BTreeMap;
    #[cfg(target_os = "macos")]
    use std::path::Path;

    fn profile(id: &str) -> ConnectionProfile {
        ConnectionProfile {
            id: id.into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "Mosh test".into(),
            host: "127.0.0.1".into(),
            port: 22,
            username: "tester".into(),
            auth_type: "sshAgent".into(),
            private_key_path: None,
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: String::new(),
            tags: Vec::new(),
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: BTreeMap::new(),
            has_credential: false,
            created_at: String::new(),
            updated_at: String::new(),
            last_connected_at: None,
        }
    }

    #[test]
    fn parses_strict_server_handshake_without_exposing_key() {
        let output = "notice\nMOSH CONNECT 60003 ABCDEFGHIJKLMNOPQRSTUV\n";
        let parsed = parse_server_output(output).unwrap();
        assert_eq!(parsed.port, 60003);
        assert_eq!(parsed.key.len(), 22);
        let debug = format!("{parsed:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("ABCDEFGHIJKLMNOPQRSTUV"));
    }

    #[test]
    fn rejects_malformed_handshakes_and_bad_ports() {
        assert!(parse_server_output("MOSH CONNECT 22 ABCDEFGHIJKLMNOPQRSTUV\n").is_err());
        assert!(parse_server_output("MOSH CONNECT 60000 bad\n").is_err());
        assert!(validate_ports(1023, 60000).is_err());
        assert!(validate_ports(61000, 60000).is_err());
        assert!(validate_ports(60000, 61001).is_err());
        assert!(validate_ports(60000, 60010).is_ok());
    }

    #[test]
    fn server_command_contains_only_validated_numeric_ports() {
        let command = server_command(60000, 60010);
        assert!(command.contains("-p 60000:60010"));
        assert!(!command.contains('\n'));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn bundled_path_resolves_inside_app_resources() {
        let executable = Path::new("/Applications/CNshell.app/Contents/MacOS/cnshell");
        let path = executable
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("Resources/mosh/mosh-client");
        assert_eq!(
            path,
            Path::new("/Applications/CNshell.app/Contents/Resources/mosh/mosh-client")
        );
    }

    #[test]
    fn failed_session_keeps_external_profile_until_explicit_close() {
        let mosh = MoshManager::default();
        let manager = SessionManager::default();
        manager.insert_external("mosh-session".into(), profile("server"));

        cleanup_after_process_exit(&mosh, &manager, "mosh-session", false);
        assert_eq!(manager.profile("mosh-session").unwrap().id, "server");

        cleanup_after_process_exit(&mosh, &manager, "mosh-session", true);
        assert!(manager.profile("mosh-session").is_err());
    }

    #[test]
    fn legacy_protocol_options_receive_safe_mosh_defaults() {
        let options: ConnectionProtocolOptions = serde_json::from_value(serde_json::json!({
            "connectionId": "legacy",
            "agentForwarding": false
        }))
        .unwrap();
        assert!(!options.mosh_enabled);
        assert!(!options.x11_enabled);
        assert_eq!(options.mosh_port_start, DEFAULT_PORT_START);
        assert_eq!(options.mosh_port_end, DEFAULT_PORT_END);
    }

    #[test]
    fn reconnect_detection_handles_split_overlay_and_small_recovery_redraw() {
        let started = Instant::now();
        let mut detector = ReconnectDetector::default();
        assert_eq!(
            detector.observe(b"Nothing received from server on UDP ", started),
            ReconnectObservation::Unchanged
        );
        assert_eq!(
            detector.observe(b"port 60000", started + Duration::from_millis(10)),
            ReconnectObservation::Unavailable
        );
        assert_eq!(
            detector.observe(b"screen redraw", started + Duration::from_secs(1)),
            ReconnectObservation::Unchanged
        );
        assert_eq!(
            detector.observe(b"root@server:~# ", started + Duration::from_secs(3)),
            ReconnectObservation::Recovered
        );
    }

    #[test]
    fn reconnect_detection_recognizes_bundled_client_timeout_text() {
        let started = Instant::now();
        let mut detector = ReconnectDetector::default();
        assert_eq!(
            detector.observe(b"mosh: Last rep", started),
            ReconnectObservation::Unchanged
        );
        assert_eq!(
            detector.observe(b"ly 9 seconds ago.", started + Duration::from_millis(10)),
            ReconnectObservation::Unavailable
        );
        assert_eq!(
            detector.observe(
                b"mosh did not make a successful connection",
                started + Duration::from_secs(1)
            ),
            ReconnectObservation::Unavailable
        );
    }

    #[test]
    fn udp_failure_guidance_covers_local_and_remote_filters() {
        assert!(UDP_WAITING_MESSAGE.contains("VPN/代理"));
        assert!(UDP_WAITING_MESSAGE.contains("云安全组"));
        assert!(UDP_FAILED_MESSAGE.contains("直连"));
        assert!(UDP_FAILED_MESSAGE.contains("UDP 端口范围"));
    }
}
