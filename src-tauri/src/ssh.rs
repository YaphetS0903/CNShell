use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        ConnectionDiagnostic, ConnectionProfile, TerminalOutput, TerminalSession, TerminalStatus,
        ZmodemEvent,
    },
    session_log::SessionLogManager,
    zmodem::{
        ActiveTransfer, AwaitingTransfer, CANCEL_SEQUENCE, Direction as ZmodemDirection,
        FinishingTransfer, SessionState as ZmodemState,
    },
};
use base64::{
    Engine,
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
};
use parking_lot::Mutex;
use sha2::{Digest, Sha256};
use socket2::{SockRef, TcpKeepalive};
use ssh2::{Channel, HostKeyType, Session};
use std::{
    collections::HashMap,
    io::{ErrorKind, Read, Write},
    net::{TcpListener, TcpStream, ToSocketAddrs},
    path::Path,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tauri::{AppHandle, Emitter};
use tokio::time::sleep;
use uuid::Uuid;

const KEYCHAIN_SERVICE: &str = "com.cnshell.desktop";
const AUTHENTICATION_TIMEOUT: Duration = Duration::from_secs(30);
const DIAGNOSTIC_SHELL_TIMEOUT: Duration = Duration::from_secs(30);
const KEEPALIVE_INTERVAL_SECONDS: u32 = 30;
const TCP_KEEPALIVE_IDLE: Duration = Duration::from_secs(45);
const TCP_KEEPALIVE_INTERVAL: Duration = Duration::from_secs(10);
const TCP_KEEPALIVE_RETRIES: u32 = 3;
static KEYCHAIN_ACCESS: OnceLock<Mutex<()>> = OnceLock::new();

fn keychain_access() -> parking_lot::MutexGuard<'static, ()> {
    KEYCHAIN_ACCESS.get_or_init(|| Mutex::new(())).lock()
}

async fn blocking_with_timeout<T, F>(
    operation: &'static str,
    duration: Duration,
    work: F,
) -> AppResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> AppResult<T> + Send + 'static,
{
    match tokio::time::timeout(duration, tokio::task::spawn_blocking(work)).await {
        Ok(Ok(result)) => result,
        Ok(Err(error)) => Err(AppError::Internal(error.to_string())),
        Err(_) => Err(AppError::Unavailable(format!(
            "{operation}超时，请检查系统凭据授权或网络状态后重试"
        ))),
    }
}

pub struct ConnectedSsh {
    pub session: Session,
    pub fingerprint: String,
    pub algorithm: String,
    transport: TcpStream,
}

const MAX_IDLE_TRANSPORTS_PER_PROFILE: usize = 2;
const MAX_IDLE_TRANSPORT_AGE: Duration = Duration::from_secs(20);

struct IdleTransport {
    connected: ConnectedSsh,
    idle_since: Instant,
}

#[derive(Clone, Default)]
pub struct TransportPool {
    idle: Arc<Mutex<HashMap<String, Vec<IdleTransport>>>>,
    created: Arc<AtomicUsize>,
}

pub struct TransportLease {
    pool: TransportPool,
    key: String,
    connected: Option<ConnectedSsh>,
    reusable: bool,
}

impl TransportPool {
    pub async fn acquire(
        &self,
        db: &Database,
        profile: &ConnectionProfile,
        reusable: bool,
    ) -> AppResult<TransportLease> {
        let key = transport_pool_key(profile);
        let connected = if reusable {
            let mut idle = self.idle.lock();
            idle.get_mut(&key)
                .and_then(|transports| {
                    transports.retain(|transport| {
                        transport.idle_since.elapsed() <= MAX_IDLE_TRANSPORT_AGE
                    });
                    transports.pop()
                })
                .map(|idle| idle.connected)
                .filter(|connected| {
                    connected.session.authenticated() && connected.session.keepalive_send().is_ok()
                })
        } else {
            None
        };
        let connected = match connected {
            Some(connected) => connected,
            None => {
                self.created.fetch_add(1, Ordering::Relaxed);
                verified_connection(db, profile, false).await?
            }
        };
        Ok(TransportLease {
            pool: self.clone(),
            key,
            connected: Some(connected),
            reusable,
        })
    }

    pub fn invalidate(&self, connection_id: &str) {
        self.idle
            .lock()
            .retain(|key, _| !key.starts_with(&format!("{connection_id}:")));
    }

    pub fn clear(&self) {
        self.idle.lock().clear();
    }

    #[cfg(test)]
    fn created(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }
}

impl TransportLease {
    pub fn connected(&self) -> &ConnectedSsh {
        self.connected.as_ref().expect("transport lease is active")
    }

    pub fn discard(&mut self) {
        self.reusable = false;
    }
}

impl Drop for TransportLease {
    fn drop(&mut self) {
        let Some(connected) = self.connected.take() else {
            return;
        };
        if !self.reusable || !connected.session.authenticated() {
            return;
        }
        let mut idle = self.pool.idle.lock();
        let transports = idle.entry(self.key.clone()).or_default();
        if transports.len() < MAX_IDLE_TRANSPORTS_PER_PROFILE {
            transports.push(IdleTransport {
                connected,
                idle_since: Instant::now(),
            });
        }
    }
}

fn transport_pool_key(profile: &ConnectionProfile) -> String {
    format!("{}:{}", profile.id, profile.updated_at)
}

pub struct TerminalHandle {
    pub profile: ConnectionProfile,
    #[allow(dead_code)]
    pub session: Session,
    pub channel: Channel,
    pub closed: bool,
    pub cols: u32,
    pub rows: u32,
    pub agent_forwarding: bool,
    pub x11_enabled: bool,
    pub zmodem: ZmodemState,
    _x11_forwarder: Option<crate::x11::X11Forwarder>,
    transport: Option<TransportLease>,
}

#[derive(Clone, Default)]
pub struct SessionManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<TerminalHandle>>>>>,
    external_profiles: Arc<Mutex<HashMap<String, ConnectionProfile>>>,
    transports: TransportPool,
}

impl SessionManager {
    pub fn insert(&self, id: String, handle: TerminalHandle) -> Arc<Mutex<TerminalHandle>> {
        let handle = Arc::new(Mutex::new(handle));
        self.sessions.lock().insert(id, handle.clone());
        handle
    }

    pub fn get(&self, id: &str) -> AppResult<Arc<Mutex<TerminalHandle>>> {
        self.sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("会话 {id}")))
    }

    pub fn profile(&self, id: &str) -> AppResult<ConnectionProfile> {
        if let Ok(handle) = self.get(id) {
            return Ok(handle.lock().profile.clone());
        }
        self.external_profiles
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("会话 {id}")))
    }

    pub fn insert_external(&self, id: String, profile: ConnectionProfile) {
        self.external_profiles.lock().insert(id, profile);
    }

    pub fn remove_external(&self, id: &str) {
        self.external_profiles.lock().remove(id);
    }

    pub fn remove(&self, id: &str) -> Option<Arc<Mutex<TerminalHandle>>> {
        self.sessions.lock().remove(id)
    }

    pub async fn acquire_transport(
        &self,
        db: &Database,
        profile: &ConnectionProfile,
        reusable: bool,
    ) -> AppResult<TransportLease> {
        self.transports.acquire(db, profile, reusable).await
    }

    pub fn invalidate_transport(&self, connection_id: &str) {
        self.transports.invalidate(connection_id);
    }

    pub fn clear_transports(&self) {
        self.transports.clear();
    }
}

pub fn credential_ref(connection_id: &str) -> String {
    format!("connection:{connection_id}")
}

pub fn save_credential(connection_id: &str, secret: &str) -> AppResult<String> {
    let _access = keychain_access();
    let reference = credential_ref(connection_id);
    keyring::Entry::new(KEYCHAIN_SERVICE, &reference)
        .map_err(|error| AppError::Storage(error.to_string()))?
        .set_password(secret)
        .map_err(|error| AppError::Storage(error.to_string()))?;
    Ok(reference)
}

pub fn delete_credential(connection_id: &str) -> AppResult<()> {
    let _access = keychain_access();
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_ref(connection_id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(AppError::Storage(format!(
            "{}中的凭据清理失败（{}）：{error}",
            crate::platform::credential_store_name(),
            credential_ref(connection_id)
        ))),
    }
}

pub fn load_credential(connection_id: &str) -> AppResult<Option<String>> {
    let _access = keychain_access();
    let entry = keyring::Entry::new(KEYCHAIN_SERVICE, &credential_ref(connection_id))
        .map_err(|error| AppError::Storage(error.to_string()))?;
    match entry.get_password() {
        Ok(secret) => Ok(Some(secret)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(AppError::Storage(error.to_string())),
    }
}

fn host_key_algorithm(kind: HostKeyType) -> &'static str {
    match kind {
        HostKeyType::Rsa => "ssh-rsa",
        HostKeyType::Dss => "ssh-dss",
        HostKeyType::Ecdsa256 => "ecdsa-sha2-nistp256",
        HostKeyType::Ecdsa384 => "ecdsa-sha2-nistp384",
        HostKeyType::Ecdsa521 => "ecdsa-sha2-nistp521",
        HostKeyType::Ed25519 => "ssh-ed25519",
        HostKeyType::Unknown => "unknown",
    }
}

fn direct_tcp(host: &str, port: i64) -> AppResult<TcpStream> {
    let address = host_port(host, port);
    let addresses = address
        .to_socket_addrs()
        .map_err(|error| AppError::Remote(format!("DNS 解析失败：{error}")))?;
    let mut last_error = None;
    for socket in addresses {
        match TcpStream::connect_timeout(&socket, Duration::from_secs(10)) {
            Ok(stream) => {
                stream.set_read_timeout(Some(Duration::from_secs(20)))?;
                stream.set_write_timeout(Some(Duration::from_secs(20)))?;
                stream.set_nodelay(true)?;
                configure_tcp_keepalive(&stream)?;
                return Ok(stream);
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(AppError::Remote(format!(
        "TCP 连接失败：{}",
        last_error
            .map(|e| e.to_string())
            .unwrap_or_else(|| "没有可用地址".into())
    )))
}

fn configure_tcp_keepalive(stream: &TcpStream) -> std::io::Result<()> {
    SockRef::from(stream).set_tcp_keepalive(
        &TcpKeepalive::new()
            .with_time(TCP_KEEPALIVE_IDLE)
            .with_interval(TCP_KEEPALIVE_INTERVAL)
            .with_retries(TCP_KEEPALIVE_RETRIES),
    )
}

fn host_port(host: &str, port: i64) -> String {
    if host.contains(':') && !host.starts_with('[') {
        format!("[{host}]:{port}")
    } else {
        format!("{host}:{port}")
    }
}

fn socks5_tcp(host: &str, port: i64, proxy: &crate::models::ProxyProfile) -> AppResult<TcpStream> {
    let mut stream = direct_tcp(&proxy.host, proxy.port)?;
    let secret = load_credential(&format!("proxy:{}", proxy.id))?;
    let auth = secret.is_some() && proxy.username.is_some();
    stream.write_all(if auth { &[5, 1, 2] } else { &[5, 1, 0] })?;
    let mut response = [0_u8; 2];
    stream.read_exact(&mut response)?;
    if response[0] != 5 || response[1] == 0xff {
        return Err(AppError::Remote("SOCKS5 代理拒绝协商".into()));
    }
    if response[1] == 2 {
        if !auth {
            return Err(AppError::Authentication(
                "SOCKS5 代理要求用户名和密码".into(),
            ));
        }
        let username = proxy.username.as_deref().unwrap_or("").as_bytes();
        let password = secret.as_deref().unwrap_or("").as_bytes();
        if username.len() > 255 || password.len() > 255 {
            return Err(AppError::Validation("代理用户名或密码过长".into()));
        }
        let mut request = vec![1, username.len() as u8];
        request.extend_from_slice(username);
        request.push(password.len() as u8);
        request.extend_from_slice(password);
        stream.write_all(&request)?;
        stream.read_exact(&mut response)?;
        if response[0] != 1 || response[1] != 0 {
            return Err(AppError::Authentication("SOCKS5 代理认证失败".into()));
        }
    }
    let mut request = vec![5, 1, 0];
    if let Ok(address) = host.parse::<std::net::IpAddr>() {
        match address {
            std::net::IpAddr::V4(address) => {
                request.push(1);
                request.extend_from_slice(&address.octets());
            }
            std::net::IpAddr::V6(address) => {
                request.push(4);
                request.extend_from_slice(&address.octets());
            }
        }
    } else {
        let host_bytes = host.as_bytes();
        if host_bytes.len() > 255 {
            return Err(AppError::Validation("目标主机名过长".into()));
        }
        request.extend_from_slice(&[3, host_bytes.len() as u8]);
        request.extend_from_slice(host_bytes);
    }
    request.extend_from_slice(&(port as u16).to_be_bytes());
    stream.write_all(&request)?;
    let mut head = [0_u8; 4];
    stream.read_exact(&mut head)?;
    if head[0] != 5 || head[1] != 0 {
        return Err(AppError::Remote(format!(
            "SOCKS5 CONNECT 失败，代码 {}",
            head[1]
        )));
    }
    let tail = match head[3] {
        1 => 6,
        4 => 18,
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length)?;
            length[0] as usize + 2
        }
        _ => return Err(AppError::Remote("SOCKS5 响应地址类型无效".into())),
    };
    let mut discard = vec![0_u8; tail];
    stream.read_exact(&mut discard)?;
    Ok(stream)
}

fn http_proxy_tcp(
    host: &str,
    port: i64,
    proxy: &crate::models::ProxyProfile,
) -> AppResult<TcpStream> {
    let mut stream = direct_tcp(&proxy.host, proxy.port)?;
    let target = host_port(host, port);
    let mut request =
        format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\nProxy-Connection: Keep-Alive\r\n");
    if let (Some(username), Some(password)) = (
        proxy.username.as_deref(),
        load_credential(&format!("proxy:{}", proxy.id))?.as_deref(),
    ) {
        request.push_str(&format!(
            "Proxy-Authorization: Basic {}\r\n",
            base64::engine::general_purpose::STANDARD.encode(format!("{username}:{password}"))
        ));
    }
    request.push_str("\r\n");
    stream.write_all(request.as_bytes())?;
    let mut response = Vec::new();
    let mut byte = [0_u8; 1];
    while response.len() < 16 * 1024 {
        stream.read_exact(&mut byte)?;
        response.push(byte[0]);
        if response.ends_with(b"\r\n\r\n") {
            break;
        }
    }
    if !response.ends_with(b"\r\n\r\n") {
        return Err(AppError::Remote(
            "HTTP 代理响应头超过 16 KB 或不完整".into(),
        ));
    }
    let status = String::from_utf8_lossy(&response);
    if !status.starts_with("HTTP/1.1 200 ") && !status.starts_with("HTTP/1.0 200 ") {
        return Err(AppError::Remote(format!(
            "HTTP 代理 CONNECT 失败：{}",
            status.lines().next().unwrap_or("无响应")
        )));
    }
    Ok(stream)
}

fn bridge_jump(jump: ConnectedSsh, host: String, port: i64) -> AppResult<TcpStream> {
    jump.session.set_timeout(20);
    let channel = jump
        .session
        .channel_direct_tcpip(&host, port as u16, None)?;
    let listener = TcpListener::bind("127.0.0.1:0")?;
    let address = listener.local_addr()?;
    let client = TcpStream::connect(address)?;
    let (client_side, _) = listener.accept()?;
    client_side.set_nodelay(true)?;
    crate::tunnel::bridge(client_side, channel);
    Ok(client)
}

fn handshake_stream(stream: TcpStream) -> AppResult<ConnectedSsh> {
    let transport = stream.try_clone()?;
    let mut session = Session::new().map_err(|error| AppError::Remote(error.to_string()))?;
    session.set_tcp_stream(stream);
    session
        .handshake()
        .map_err(|error| AppError::Remote(format!("SSH 握手失败：{error}")))?;
    let (key, kind) = session
        .host_key()
        .ok_or_else(|| AppError::Remote("服务端未提供主机密钥".into()))?;
    let fingerprint = format!("SHA256:{}", STANDARD_NO_PAD.encode(Sha256::digest(key)));
    Ok(ConnectedSsh {
        session,
        fingerprint,
        algorithm: host_key_algorithm(kind).into(),
        transport,
    })
}

pub fn handshake(
    profile: &ConnectionProfile,
    proxy: Option<&crate::models::ProxyProfile>,
    jump: Option<ConnectedSsh>,
) -> AppResult<ConnectedSsh> {
    let stream = match (proxy, jump) {
        (Some(proxy), _) if proxy.proxy_type == "socks5" => {
            socks5_tcp(&profile.host, profile.port, proxy)?
        }
        (Some(proxy), _) if proxy.proxy_type == "http" => {
            http_proxy_tcp(&profile.host, profile.port, proxy)?
        }
        (Some(proxy), Some(jump)) if proxy.proxy_type == "sshJump" => {
            bridge_jump(jump, profile.host.clone(), profile.port)?
        }
        _ => direct_tcp(&profile.host, profile.port)?,
    };
    handshake_stream(stream)
}

pub fn authenticate(
    connected: ConnectedSsh,
    profile: &ConnectionProfile,
) -> AppResult<ConnectedSsh> {
    let secret = load_credential(&profile.id)?;
    match profile.auth_type.as_str() {
        "password" => {
            let password = secret.ok_or_else(|| {
                AppError::Authentication(format!(
                    "{}中没有保存密码",
                    crate::platform::credential_store_name()
                ))
            })?;
            connected
                .session
                .userauth_password(&profile.username, &password)
                .map_err(|error| AppError::Authentication(error.to_string()))?;
        }
        "privateKey" => {
            let fallback = profile
                .private_key_path
                .as_deref()
                .ok_or_else(|| AppError::Authentication("未选择私钥".into()))?;
            let access = crate::bookmark::access(&profile.id, Path::new(fallback))?;
            connected
                .session
                .userauth_pubkey_file(&profile.username, None, access.path(), secret.as_deref())
                .map_err(|error| AppError::Authentication(error.to_string()))?;
        }
        "sshCertificate" => {
            let private_fallback = profile
                .private_key_path
                .as_deref()
                .ok_or_else(|| AppError::Authentication("未选择证书对应的私钥".into()))?;
            let certificate_fallback = profile
                .certificate_path
                .as_deref()
                .ok_or_else(|| AppError::Authentication("未选择 SSH Certificate".into()))?;
            let private_access = crate::bookmark::access(&profile.id, Path::new(private_fallback))?;
            let certificate_access =
                crate::bookmark::access_certificate(&profile.id, Path::new(certificate_fallback))?;
            let info = crate::certificate::inspect(certificate_access.path())?;
            if !info.valid_now {
                return Err(AppError::Authentication(match info.status.as_str() {
                    "expired" => "SSH Certificate 已过期".into(),
                    "notYetValid" => "SSH Certificate 尚未生效".into(),
                    _ => "SSH Certificate 当前无效".into(),
                }));
            }
            crate::certificate::validate_for_username(&info, &profile.username)?;
            connected
                .session
                .userauth_pubkey_file(
                    &profile.username,
                    Some(certificate_access.path()),
                    private_access.path(),
                    secret.as_deref(),
                )
                .map_err(|error| AppError::Authentication(error.to_string()))?;
        }
        "sshAgent" => {
            let mut agent = connected
                .session
                .agent()
                .map_err(|error| AppError::Authentication(error.to_string()))?;
            agent
                .connect()
                .map_err(|error| AppError::Authentication(error.to_string()))?;
            agent
                .list_identities()
                .map_err(|error| AppError::Authentication(error.to_string()))?;
            let mut authenticated = false;
            for identity in agent
                .identities()
                .map_err(|error| AppError::Authentication(error.to_string()))?
            {
                if agent.userauth(&profile.username, &identity).is_ok() {
                    authenticated = true;
                    break;
                }
            }
            if !authenticated {
                return Err(AppError::Authentication("SSH Agent 中没有可用身份".into()));
            }
        }
        "fido2Agent" => {
            authenticate_fido2_agent(&connected.session, &profile.username)?;
        }
        other => {
            return Err(AppError::Authentication(format!(
                "不支持的认证方式：{other}"
            )));
        }
    }
    if !connected.session.authenticated() {
        return Err(AppError::Authentication("服务端拒绝认证".into()));
    }
    Ok(connected)
}

const FIDO2_KEY_TYPES: [&str; 4] = [
    "sk-ssh-ed25519@openssh.com",
    "sk-ecdsa-sha2-nistp256@openssh.com",
    "sk-ssh-ed25519-cert-v01@openssh.com",
    "sk-ecdsa-sha2-nistp256-cert-v01@openssh.com",
];

fn public_key_type(blob: &[u8]) -> Option<&str> {
    let length = u32::from_be_bytes(blob.get(..4)?.try_into().ok()?) as usize;
    let end = 4usize.checked_add(length)?;
    std::str::from_utf8(blob.get(4..end)?).ok()
}

fn is_fido2_key(blob: &[u8]) -> bool {
    public_key_type(blob).is_some_and(|key_type| FIDO2_KEY_TYPES.contains(&key_type))
}

fn public_key_fingerprint(blob: &[u8]) -> String {
    use base64::Engine as _;
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(blob);
    format!(
        "SHA256:{}",
        base64::engine::general_purpose::STANDARD_NO_PAD.encode(digest)
    )
}

fn fido2_identity(identity: &ssh2::PublicKey) -> Option<crate::models::Fido2Identity> {
    let key_type = public_key_type(identity.blob())?;
    if !FIDO2_KEY_TYPES.contains(&key_type) {
        return None;
    }
    Some(crate::models::Fido2Identity {
        key_type: key_type.to_owned(),
        comment: identity.comment().to_owned(),
        fingerprint: public_key_fingerprint(identity.blob()),
    })
}

fn connected_agent(session: &ssh2::Session) -> AppResult<ssh2::Agent> {
    #[cfg(not(target_os = "windows"))]
    if std::env::var_os("SSH_AUTH_SOCK").is_none() {
        return Err(AppError::Unavailable(
            "未检测到 SSH Agent。请先将 FIDO2 硬件密钥加入系统 OpenSSH Agent".into(),
        ));
    }
    let mut agent = session
        .agent()
        .map_err(|error| AppError::Unavailable(format!("无法初始化 SSH Agent：{error}")))?;
    agent
        .connect()
        .map_err(|error| AppError::Unavailable(format!("无法连接 SSH Agent：{error}")))?;
    agent
        .list_identities()
        .map_err(|error| AppError::Unavailable(format!("无法读取 SSH Agent 身份：{error}")))?;
    Ok(agent)
}

pub fn list_fido2_identities() -> AppResult<Vec<crate::models::Fido2Identity>> {
    let session = ssh2::Session::new()
        .map_err(|error| AppError::Unavailable(format!("无法初始化 SSH：{error}")))?;
    let agent = connected_agent(&session)?;
    let identities = agent
        .identities()
        .map_err(|error| AppError::Unavailable(format!("无法读取 SSH Agent 身份：{error}")))?;
    Ok(identities.iter().filter_map(fido2_identity).collect())
}

fn authenticate_fido2_agent(session: &ssh2::Session, username: &str) -> AppResult<()> {
    let agent = connected_agent(session)?;
    let identities = agent
        .identities()
        .map_err(|error| AppError::Authentication(format!("无法读取 SSH Agent 身份：{error}")))?;
    let hardware_identities = identities
        .iter()
        .filter(|identity| is_fido2_key(identity.blob()))
        .collect::<Vec<_>>();
    if hardware_identities.is_empty() {
        return Err(AppError::Authentication(
            "SSH Agent 中没有 FIDO2 硬件身份。请插入安全密钥，并用 ssh-add 加载 sk-ssh-ed25519 或 sk-ecdsa-sha2-nistp256 密钥".into(),
        ));
    }
    let mut last_error = None;
    for identity in hardware_identities {
        match agent.userauth(username, identity) {
            Ok(()) if session.authenticated() => return Ok(()),
            Ok(()) => last_error = Some("服务端拒绝该硬件身份".to_owned()),
            Err(error) => last_error = Some(error.to_string()),
        }
    }
    Err(AppError::Authentication(fido2_failure_message(
        last_error.as_deref(),
    )))
}

fn fido2_failure_message(agent_error: Option<&str>) -> String {
    let detail = agent_error.unwrap_or_default();
    let normalized = detail.to_ascii_lowercase();
    let guidance = if normalized.contains("cancel") || normalized.contains("canceled") {
        "已取消硬件密钥验证；请重新连接并完成系统提示"
    } else if normalized.contains("pin") {
        "硬件密钥需要 PIN，或 PIN 验证失败；请在系统提示中输入正确 PIN"
    } else if normalized.contains("touch") || normalized.contains("presence") {
        "未完成触摸确认；请在硬件密钥闪烁时触摸设备"
    } else if normalized.contains("removed")
        || normalized.contains("no such device")
        || normalized.contains("device not found")
    {
        "硬件密钥已拔出或不可用；请重新插入设备后连接"
    } else {
        "请确认设备仍已插入，并按系统提示触摸密钥或输入 PIN；如果刚刚取消了提示，请重新连接"
    };
    if detail.is_empty() {
        format!("FIDO2 硬件密钥认证失败：{guidance}")
    } else {
        format!("FIDO2 硬件密钥认证失败：{guidance}。Agent 返回：{detail}")
    }
}

async fn transport_connection_with_chain(
    db: &Database,
    profile: &ConnectionProfile,
    chain: &mut Vec<String>,
) -> AppResult<ConnectedSsh> {
    if chain.iter().any(|id| id == &profile.id) {
        return Err(AppError::Validation(format!(
            "SSH 跳板连接形成循环：{} → {}",
            chain.join(" → "),
            profile.id
        )));
    }
    chain.push(profile.id.clone());
    let proxy = if let Some(id) = profile.proxy_id.as_deref() {
        Some(db.get_proxy(id).await?)
    } else {
        None
    };
    let jump = if let Some(proxy) = proxy.as_ref().filter(|item| item.proxy_type == "sshJump") {
        let jump_id = proxy
            .jump_connection_id
            .as_deref()
            .ok_or_else(|| AppError::Validation("跳板机代理未选择连接".into()))?;
        let jump_profile = db.get_connection(jump_id).await?;
        Some(
            Box::pin(verified_connection_with_chain(
                db,
                &jump_profile,
                false,
                chain,
            ))
            .await?,
        )
    } else {
        None
    };
    let profile_clone = profile.clone();
    tokio::task::spawn_blocking(move || handshake(&profile_clone, proxy.as_ref(), jump))
        .await
        .map_err(|error| AppError::Internal(error.to_string()))?
}

async fn transport_connection(
    db: &Database,
    profile: &ConnectionProfile,
) -> AppResult<ConnectedSsh> {
    transport_connection_with_chain(db, profile, &mut Vec::new()).await
}

pub async fn verified_connection(
    db: &Database,
    profile: &ConnectionProfile,
    permit_unknown: bool,
) -> AppResult<ConnectedSsh> {
    verified_connection_with_chain(db, profile, permit_unknown, &mut Vec::new()).await
}

async fn verified_connection_with_chain(
    db: &Database,
    profile: &ConnectionProfile,
    permit_unknown: bool,
    chain: &mut Vec<String>,
) -> AppResult<ConnectedSsh> {
    let connected = transport_connection_with_chain(db, profile, chain).await?;
    let known = db.known_host(&profile.host, profile.port).await?;
    let accept_new = profile.host_key_policy == "acceptNew";
    verify_host_identity(
        known.clone(),
        &connected.algorithm,
        &connected.fingerprint,
        permit_unknown || accept_new,
    )?;
    if known.is_none() && accept_new {
        db.trust_host(
            &profile.host,
            profile.port,
            &connected.algorithm,
            &connected.fingerprint,
        )
        .await?;
    }
    let profile_clone = profile.clone();
    blocking_with_timeout("SSH 认证", AUTHENTICATION_TIMEOUT, move || {
        authenticate(connected, &profile_clone)
    })
    .await
}

fn verify_host_identity(
    known: Option<(String, String)>,
    algorithm: &str,
    fingerprint: &str,
    permit_unknown: bool,
) -> AppResult<()> {
    match known {
        Some((expected_algorithm, expected_fingerprint))
            if expected_algorithm != algorithm || expected_fingerprint != fingerprint =>
        {
            Err(AppError::HostKeyChanged {
                expected: format!("{expected_algorithm} {expected_fingerprint}"),
                actual: format!("{algorithm} {fingerprint}"),
            })
        }
        None if !permit_unknown => Err(AppError::HostKeyUnknown {
            fingerprint: fingerprint.into(),
            algorithm: algorithm.into(),
        }),
        _ => Ok(()),
    }
}

pub async fn diagnose(db: &Database, profile: &ConnectionProfile) -> Vec<ConnectionDiagnostic> {
    let started = Instant::now();
    let (diagnostic_host, diagnostic_port, proxy_label) =
        match diagnostic_first_hop(db, profile).await {
            Ok(value) => value,
            Err(error) => return vec![diagnostic_failure("proxy", error)],
        };
    let dns_started = Instant::now();
    let endpoint = format!("{diagnostic_host}:{diagnostic_port}");
    let dns_result = tokio::task::spawn_blocking(move || {
        endpoint
            .to_socket_addrs()
            .map(|mut addresses| addresses.next())
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))
    .and_then(|result| result.map_err(|error| AppError::Remote(format!("DNS 解析失败：{error}"))))
    .and_then(|address| address.ok_or_else(|| AppError::Remote("DNS 解析没有返回可用地址".into())));
    let mut diagnostics = match dns_result {
        Ok(address) => vec![ConnectionDiagnostic {
            stage: "dns".into(),
            ok: true,
            message: format!("首跳 {diagnostic_host} 解析为 {}", address.ip()),
            latency_ms: Some(dns_started.elapsed().as_millis()),
            fingerprint: None,
            algorithm: None,
        }],
        Err(error) => return vec![diagnostic_failure("dns", error)],
    };
    let connected = match transport_connection(db, profile).await {
        Ok(value) => value,
        Err(error) => {
            diagnostics.push(diagnostic_failure(
                if proxy_label.is_some() {
                    "proxy"
                } else {
                    "tcp"
                },
                error,
            ));
            return diagnostics;
        }
    };
    if let Some(label) = proxy_label {
        diagnostics.push(ConnectionDiagnostic {
            stage: "proxy".into(),
            ok: true,
            message: format!("{label} 代理链已建立"),
            latency_ms: Some(started.elapsed().as_millis()),
            fingerprint: None,
            algorithm: None,
        });
    }
    diagnostics.push(ConnectionDiagnostic {
        stage: "tcp".into(),
        ok: true,
        message: "目标 TCP 与 SSH 握手成功".into(),
        latency_ms: Some(started.elapsed().as_millis()),
        fingerprint: None,
        algorithm: None,
    });
    match db.known_host(&profile.host, profile.port).await {
        Ok(Some((algorithm, expected)))
            if expected == connected.fingerprint && algorithm == connected.algorithm =>
        {
            diagnostics.push(ConnectionDiagnostic {
                stage: "hostKey".into(),
                ok: true,
                message: "主机指纹与算法均与已保存记录一致".into(),
                latency_ms: None,
                fingerprint: Some(connected.fingerprint.clone()),
                algorithm: Some(connected.algorithm.clone()),
            })
        }
        Ok(Some((algorithm, expected))) => {
            diagnostics.push(ConnectionDiagnostic {
                stage: "hostKey".into(),
                ok: false,
                message: format!(
                    "主机身份变化：原 {algorithm} {expected}，当前 {} {}",
                    connected.algorithm, connected.fingerprint
                ),
                latency_ms: None,
                fingerprint: Some(connected.fingerprint),
                algorithm: Some(connected.algorithm),
            });
            return diagnostics;
        }
        Ok(None) if profile.host_key_policy == "acceptNew" => {
            diagnostics.push(ConnectionDiagnostic {
                stage: "hostKey".into(),
                ok: true,
                message: "首次密钥将在真实连接时自动记录；请仅在可信网络使用".into(),
                latency_ms: None,
                fingerprint: Some(connected.fingerprint.clone()),
                algorithm: Some(connected.algorithm.clone()),
            })
        }
        Ok(None) => {
            diagnostics.push(ConnectionDiagnostic {
                stage: "hostKey".into(),
                ok: false,
                message: "首次连接，请核对并信任主机指纹".into(),
                latency_ms: None,
                fingerprint: Some(connected.fingerprint),
                algorithm: Some(connected.algorithm),
            });
            return diagnostics;
        }
        Err(error) => {
            diagnostics.push(ConnectionDiagnostic {
                stage: "hostKey".into(),
                ok: false,
                message: error.to_string(),
                latency_ms: None,
                fingerprint: None,
                algorithm: None,
            });
            return diagnostics;
        }
    }
    let profile_clone = profile.clone();
    let authenticated =
        match blocking_with_timeout("SSH 认证", AUTHENTICATION_TIMEOUT, move || {
            authenticate(connected, &profile_clone)
        })
        .await
        {
            Ok(value) => {
                diagnostics.push(ConnectionDiagnostic {
                    stage: "authentication".into(),
                    ok: true,
                    message: "认证成功".into(),
                    latency_ms: None,
                    fingerprint: None,
                    algorithm: None,
                });
                value
            }
            Err(error) => {
                diagnostics.push(ConnectionDiagnostic {
                    stage: "authentication".into(),
                    ok: false,
                    message: error.to_string(),
                    latency_ms: None,
                    fingerprint: None,
                    algorithm: None,
                });
                return diagnostics;
            }
        };
    let shell = blocking_with_timeout(
        "远端 Shell 检查",
        DIAGNOSTIC_SHELL_TIMEOUT,
        move || -> AppResult<()> {
            let mut channel = authenticated.session.channel_session()?;
            channel.exec("true")?;
            let mut output = Vec::new();
            channel.read_to_end(&mut output)?;
            channel.wait_close()?;
            if channel.exit_status()? != 0 {
                return Err(AppError::Remote("远端 Exec Channel 返回非零状态".into()));
            }
            Ok(())
        },
    )
    .await;
    match shell {
        Ok(()) => {
            diagnostics.push(ConnectionDiagnostic {
                stage: "shell".into(),
                ok: true,
                message: "远端 Shell/Exec Channel 可用".into(),
                latency_ms: None,
                fingerprint: None,
                algorithm: None,
            });
            diagnostics.push(ConnectionDiagnostic {
                stage: "complete".into(),
                ok: true,
                message: "连接诊断全部通过".into(),
                latency_ms: Some(started.elapsed().as_millis()),
                fingerprint: None,
                algorithm: None,
            });
        }
        Err(error) => diagnostics.push(ConnectionDiagnostic {
            stage: "shell".into(),
            ok: false,
            message: error.to_string(),
            latency_ms: None,
            fingerprint: None,
            algorithm: None,
        }),
    };
    diagnostics
}

fn diagnostic_failure(stage: &str, error: AppError) -> ConnectionDiagnostic {
    ConnectionDiagnostic {
        stage: stage.into(),
        ok: false,
        message: error.to_string(),
        latency_ms: None,
        fingerprint: None,
        algorithm: None,
    }
}

async fn diagnostic_first_hop(
    db: &Database,
    profile: &ConnectionProfile,
) -> AppResult<(String, i64, Option<String>)> {
    let mut current = profile.clone();
    let mut visited = std::collections::HashSet::new();
    let mut labels = Vec::new();
    loop {
        if !visited.insert(current.id.clone()) {
            return Err(AppError::Validation("SSH 跳板连接形成循环".into()));
        }
        let Some(proxy_id) = current.proxy_id.as_deref() else {
            return Ok((
                current.host,
                current.port,
                (!labels.is_empty()).then(|| labels.join(" → ")),
            ));
        };
        let proxy = db.get_proxy(proxy_id).await?;
        labels.push(proxy.name.clone());
        if proxy.proxy_type == "sshJump" {
            let jump_id = proxy
                .jump_connection_id
                .as_deref()
                .ok_or_else(|| AppError::Validation("跳板机代理未选择连接".into()))?;
            current = db.get_connection(jump_id).await?;
        } else {
            return Ok((proxy.host, proxy.port, Some(labels.join(" → "))));
        }
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "Terminal startup crosses the application, database, session, PTY, logging, and forwarding boundary"
)]
pub async fn open_terminal(
    app: AppHandle,
    db: Database,
    manager: SessionManager,
    profile: ConnectionProfile,
    cols: u32,
    rows: u32,
    logs: SessionLogManager,
    agent_forwarding: bool,
    x11_enabled: bool,
) -> AppResult<TerminalSession> {
    validate_terminal_size(cols, rows)?;
    let transport = manager.acquire_transport(&db, &profile, false).await?;
    let profile_clone = profile.clone();
    let handle = tokio::task::spawn_blocking(move || {
        open_pty(
            transport,
            profile_clone,
            cols,
            rows,
            true,
            agent_forwarding,
            x11_enabled,
        )
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))??;
    let id = Uuid::new_v4().to_string();
    let shared = manager.insert(id.clone(), handle);
    spawn_reader(app, db, manager, id.clone(), shared, logs);
    Ok(TerminalSession {
        id,
        connection_id: profile.id,
        session_type: "terminal".into(),
        title: profile.name,
        status: "online".into(),
        started_at: chrono::Utc::now().to_rfc3339(),
        last_error: None,
    })
}

fn open_pty(
    transport: TransportLease,
    profile: ConnectionProfile,
    cols: u32,
    rows: u32,
    run_startup: bool,
    agent_forwarding: bool,
    x11_enabled: bool,
) -> AppResult<TerminalHandle> {
    let connected = transport.connected();
    let mut channel = connected.session.channel_session()?;
    if agent_forwarding {
        channel
            .request_auth_agent_forwarding()
            .map_err(|error| AppError::Remote(format!("Agent 转发请求被拒绝：{error}")))?;
    }
    channel.request_pty("xterm-256color", None, Some((cols, rows, 0, 0)))?;
    for (key, value) in &profile.environment {
        if crate::serial::is_option_key(key) {
            continue;
        }
        channel.setenv(key, value)?;
    }
    let x11_forwarder = if x11_enabled {
        Some(crate::x11::enable(&connected.session, &mut channel)?)
    } else {
        None
    };
    channel.shell()?;
    if run_startup && let Some(command) = profile.startup_command.as_deref() {
        channel.write_all(command.as_bytes())?;
        channel.write_all(b"\n")?;
    }
    connected
        .session
        .set_keepalive(true, KEEPALIVE_INTERVAL_SECONDS);
    connected.transport.set_nonblocking(true)?;
    connected.session.set_blocking(false);
    Ok(TerminalHandle {
        profile,
        session: connected.session.clone(),
        channel,
        closed: false,
        cols,
        rows,
        agent_forwarding,
        x11_enabled,
        zmodem: ZmodemState::default(),
        _x11_forwarder: x11_forwarder,
        transport: Some(transport),
    })
}

const RECONNECT_DELAYS: [u64; 5] = [1, 2, 5, 10, 30];
fn retryable(error: &AppError) -> bool {
    !matches!(
        error,
        AppError::Authentication(_)
            | AppError::HostKeyChanged { .. }
            | AppError::HostKeyUnknown { .. }
            | AppError::Validation(_)
    )
}

fn keepalive_is_due(last_sent: Instant, now: Instant) -> bool {
    now.duration_since(last_sent) >= Duration::from_secs(KEEPALIVE_INTERVAL_SECONDS.into())
}

fn spawn_reader(
    app: AppHandle,
    db: Database,
    manager: SessionManager,
    session_id: String,
    handle: Arc<Mutex<TerminalHandle>>,
    logs: SessionLogManager,
) {
    tauri::async_runtime::spawn(async move {
        let mut buffer = vec![0_u8; 32 * 1024];
        let mut read_failure: Option<(Instant, String)> = None;
        let mut last_keepalive = Instant::now();
        loop {
            let (read, closed, read_error) = {
                let mut terminal = handle.lock();
                let (mut error, mut disconnected) = (None, false);
                let read = match terminal.channel.read(&mut buffer) {
                    Ok(size) => {
                        read_failure = None;
                        size
                    }
                    Err(read_error) if read_error.kind() == ErrorKind::WouldBlock => {
                        read_failure = None;
                        0
                    }
                    Err(read_error) => {
                        let message = read_error.to_string();
                        let (first, _) =
                            read_failure.get_or_insert_with(|| (Instant::now(), message.clone()));
                        disconnected =
                            terminal.channel.eof() || first.elapsed() >= Duration::from_secs(2);
                        if disconnected {
                            error = Some(message);
                        }
                        0
                    }
                };
                let now = Instant::now();
                if keepalive_is_due(last_keepalive, now) {
                    match terminal.session.keepalive_send() {
                        Ok(_) => {
                            last_keepalive = now;
                        }
                        Err(keepalive_error) => {
                            disconnected = true;
                            error = Some(format!("SSH keepalive 失败：{keepalive_error}"));
                        }
                    }
                }
                (
                    read,
                    terminal.closed || disconnected || terminal.channel.eof(),
                    error,
                )
            };
            if read > 0 {
                route_terminal_bytes(&app, &logs, &session_id, &handle, &buffer[..read]);
            } else {
                expire_zmodem_authorization(&app, &session_id, &handle);
            }
            if closed {
                if manager.get(&session_id).is_err() {
                    break;
                }
                let (profile, cols, rows, agent_forwarding, x11_enabled) = {
                    let terminal = handle.lock();
                    (
                        terminal.profile.clone(),
                        terminal.cols,
                        terminal.rows,
                        terminal.agent_forwarding,
                        terminal.x11_enabled,
                    )
                };
                fail_zmodem_for_disconnect(&app, &session_id, &handle);
                let mut recovered = false;
                let mut last_error = read_error.unwrap_or_else(|| "SSH 服务端已关闭会话".into());
                for (attempt, delay) in RECONNECT_DELAYS.iter().enumerate() {
                    let _ = app.emit(
                        "terminal-status",
                        TerminalStatus {
                            session_id: session_id.clone(),
                            status: "reconnecting".into(),
                            last_error: Some(format!(
                                "{last_error}；{} 秒后进行第 {} 次重连",
                                delay,
                                attempt + 1
                            )),
                            attempt: Some((attempt + 1) as u8),
                        },
                    );
                    sleep(Duration::from_secs(*delay)).await;
                    if manager.get(&session_id).is_err() {
                        return;
                    }
                    match manager.acquire_transport(&db, &profile, false).await {
                        Ok(transport) => {
                            let profile_clone = profile.clone();
                            match tokio::task::spawn_blocking(move || {
                                open_pty(
                                    transport,
                                    profile_clone,
                                    cols,
                                    rows,
                                    false,
                                    agent_forwarding,
                                    x11_enabled,
                                )
                            })
                            .await
                            {
                                Ok(Ok(replacement)) => {
                                    *handle.lock() = replacement;
                                    let _ = app.emit(
                                        "terminal-output",
                                        TerminalOutput {
                                            session_id: session_id.clone(),
                                            data_base64: STANDARD.encode(
                                                "\r\n\x1b[32m[CNshell 已重新连接]\x1b[0m\r\n",
                                            ),
                                        },
                                    );
                                    let _ = app.emit(
                                        "terminal-status",
                                        TerminalStatus {
                                            session_id: session_id.clone(),
                                            status: "online".into(),
                                            last_error: None,
                                            attempt: None,
                                        },
                                    );
                                    recovered = true;
                                    break;
                                }
                                Ok(Err(error)) => last_error = error.to_string(),
                                Err(error) => last_error = error.to_string(),
                            }
                        }
                        Err(error) => {
                            last_error = error.to_string();
                            if !retryable(&error) {
                                break;
                            }
                        }
                    }
                }
                if recovered {
                    read_failure = None;
                    last_keepalive = Instant::now();
                    continue;
                }
                manager.remove(&session_id);
                let _ = logs.stop(&session_id);
                let _ = app.emit(
                    "terminal-status",
                    TerminalStatus {
                        session_id: session_id.clone(),
                        status: "failed".into(),
                        last_error: Some(last_error),
                        attempt: None,
                    },
                );
                break;
            }
            sleep(Duration::from_millis(if read > 0 { 5 } else { 16 })).await;
        }
    });
}

pub async fn terminal_input(
    manager: SessionManager,
    session_id: String,
    data: String,
) -> AppResult<()> {
    if data.len() > 1024 * 1024 {
        return Err(AppError::Validation("单次终端输入不能超过 1 MB".into()));
    }
    let handle = manager.get(&session_id)?;
    tokio::task::spawn_blocking(move || {
        let mut terminal = handle.lock();
        if !matches!(terminal.zmodem, ZmodemState::Detecting(_)) {
            return Err(AppError::Unavailable(
                "Zmodem 正在等待授权或传输文件，普通终端输入已暂停".into(),
            ));
        }
        write_channel_input(&mut terminal.channel, data.as_bytes())
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn emit_terminal_bytes(app: &AppHandle, logs: &SessionLogManager, session_id: &str, bytes: &[u8]) {
    if bytes.is_empty() {
        return;
    }
    logs.write_output(session_id, bytes);
    let _ = app.emit(
        "terminal-output",
        TerminalOutput {
            session_id: session_id.to_owned(),
            data_base64: STANDARD.encode(bytes),
        },
    );
}

fn zmodem_event(
    id: &str,
    session_id: &str,
    direction: ZmodemDirection,
    status: &str,
    progress: Option<crate::zmodem::TransferProgress>,
    error: Option<String>,
) -> ZmodemEvent {
    let progress = progress.unwrap_or(crate::zmodem::TransferProgress {
        file_name: None,
        total_bytes: None,
        transferred_bytes: 0,
    });
    ZmodemEvent {
        id: id.to_owned(),
        session_id: session_id.to_owned(),
        direction: direction.as_str().into(),
        status: status.into(),
        file_name: progress.file_name,
        total_bytes: progress.total_bytes,
        transferred_bytes: progress.transferred_bytes,
        error,
    }
}

fn route_terminal_bytes(
    app: &AppHandle,
    logs: &SessionLogManager,
    session_id: &str,
    handle: &Arc<Mutex<TerminalHandle>>,
    bytes: &[u8],
) {
    let mut terminal_bytes = Vec::new();
    let mut event = None;
    let mut terminal = handle.lock();
    let state = std::mem::take(&mut terminal.zmodem);
    terminal.zmodem = match state {
        ZmodemState::Detecting(mut detector) => {
            let output = detector.feed(bytes);
            terminal_bytes = output.terminal_bytes;
            if let Some(detection) = output.detection {
                let id = Uuid::new_v4().to_string();
                event = Some(zmodem_event(
                    &id,
                    session_id,
                    detection.direction,
                    "awaitingAuthorization",
                    None,
                    None,
                ));
                ZmodemState::Awaiting(AwaitingTransfer {
                    id,
                    direction: detection.direction,
                    protocol_bytes: detection.protocol_bytes,
                    detected_at: Instant::now(),
                })
            } else {
                ZmodemState::Detecting(detector)
            }
        }
        ZmodemState::Awaiting(mut awaiting) => match awaiting.append(bytes) {
            Ok(()) => ZmodemState::Awaiting(awaiting),
            Err(error) => {
                let _ = write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE);
                event = Some(zmodem_event(
                    &awaiting.id,
                    session_id,
                    awaiting.direction,
                    "failed",
                    None,
                    Some(error.to_string()),
                ));
                ZmodemState::default()
            }
        },
        ZmodemState::Active(mut active) => {
            let direction = active.direction();
            let id = active_id(&active);
            let result = active
                .append_wire(bytes)
                .and_then(|()| active.drive(&mut terminal.channel));
            match result {
                Ok(result) => {
                    terminal_bytes = result.terminal_bytes;
                    let status = result.status.unwrap_or("running");
                    event = Some(zmodem_event(
                        &id,
                        session_id,
                        direction,
                        status,
                        Some(result.progress),
                        None,
                    ));
                    if result.status == Some("completed") && direction == ZmodemDirection::Download
                    {
                        let mut finishing = FinishingTransfer::new(terminal_bytes.split_off(0));
                        if let Some(bytes) = finishing.feed(&[]) {
                            terminal_bytes = bytes;
                            ZmodemState::default()
                        } else {
                            ZmodemState::Finishing(finishing)
                        }
                    } else if result.status.is_some() {
                        ZmodemState::default()
                    } else {
                        ZmodemState::Active(active)
                    }
                }
                Err(error) => {
                    let progress = active.progress();
                    let _ = write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE);
                    event = Some(zmodem_event(
                        &id,
                        session_id,
                        direction,
                        "failed",
                        Some(progress),
                        Some(error.to_string()),
                    ));
                    ZmodemState::default()
                }
            }
        }
        ZmodemState::Finishing(mut finishing) => {
            if let Some(bytes) = finishing.feed(bytes) {
                terminal_bytes = bytes;
                ZmodemState::default()
            } else {
                ZmodemState::Finishing(finishing)
            }
        }
    };
    drop(terminal);
    emit_terminal_bytes(app, logs, session_id, &terminal_bytes);
    if let Some(event) = event {
        let _ = app.emit("zmodem-event", event);
    }
}

fn active_id(active: &ActiveTransfer) -> String {
    match active {
        ActiveTransfer::Download(transfer) => transfer.id.clone(),
        ActiveTransfer::Upload(transfer) => transfer.id.clone(),
    }
}

fn expire_zmodem_authorization(
    app: &AppHandle,
    session_id: &str,
    handle: &Arc<Mutex<TerminalHandle>>,
) {
    let mut terminal = handle.lock();
    let state = std::mem::take(&mut terminal.zmodem);
    match state {
        ZmodemState::Awaiting(awaiting) if awaiting.expired() => {
            let _ = write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE);
            let _ = app.emit(
                "zmodem-event",
                zmodem_event(
                    &awaiting.id,
                    session_id,
                    awaiting.direction,
                    "cancelled",
                    None,
                    Some("等待文件授权超时，已取消 Zmodem 传输".into()),
                ),
            );
        }
        ZmodemState::Finishing(finishing) if finishing.expired() => {}
        other => terminal.zmodem = other,
    }
}

fn fail_zmodem_for_disconnect(
    app: &AppHandle,
    session_id: &str,
    handle: &Arc<Mutex<TerminalHandle>>,
) {
    let mut terminal = handle.lock();
    let state = std::mem::take(&mut terminal.zmodem);
    let event = match state {
        ZmodemState::Detecting(detector) => {
            terminal.zmodem = ZmodemState::Detecting(detector);
            None
        }
        ZmodemState::Awaiting(awaiting) => Some(zmodem_event(
            &awaiting.id,
            session_id,
            awaiting.direction,
            "failed",
            None,
            Some("SSH 连接已中断，Zmodem 传输无法恢复".into()),
        )),
        ZmodemState::Active(active) => Some(zmodem_event(
            &active_id(&active),
            session_id,
            active.direction(),
            "failed",
            Some(active.progress()),
            Some("SSH 连接已中断，Zmodem 传输无法恢复".into()),
        )),
        ZmodemState::Finishing(_) => None,
    };
    drop(terminal);
    if let Some(event) = event {
        let _ = app.emit("zmodem-event", event);
    }
}

pub async fn zmodem_start(
    app: AppHandle,
    manager: SessionManager,
    session_id: String,
    transfer_id: String,
    paths: Vec<String>,
) -> AppResult<ZmodemEvent> {
    let handle = manager.get(&session_id)?;
    tokio::task::spawn_blocking(move || {
        let mut terminal = handle.lock();
        let state = std::mem::take(&mut terminal.zmodem);
        let awaiting = match state {
            ZmodemState::Awaiting(awaiting) if awaiting.id == transfer_id => awaiting,
            other => {
                terminal.zmodem = other;
                return Err(AppError::Unavailable(
                    "Zmodem 授权请求已过期或不匹配".into(),
                ));
            }
        };
        let active_result = match awaiting.direction {
            ZmodemDirection::Download if paths.len() == 1 => ActiveTransfer::download(
                Path::new(&paths[0]).to_path_buf(),
                awaiting.protocol_bytes,
                awaiting.id.clone(),
            ),
            ZmodemDirection::Download => {
                Err(AppError::Validation("Zmodem 下载需要选择一个目录".into()))
            }
            ZmodemDirection::Upload => ActiveTransfer::upload(
                paths
                    .iter()
                    .map(|path| Path::new(path).to_path_buf())
                    .collect(),
                awaiting.protocol_bytes,
                awaiting.id.clone(),
            ),
        };
        let mut active = match active_result {
            Ok(active) => active,
            Err(error) => {
                let _ = write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE);
                let event = zmodem_event(
                    &awaiting.id,
                    &session_id,
                    awaiting.direction,
                    "failed",
                    None,
                    Some(error.to_string()),
                );
                let _ = app.emit("zmodem-event", event);
                return Err(error);
            }
        };
        let drive = match active.drive(&mut terminal.channel) {
            Ok(drive) => drive,
            Err(error) => {
                let progress = active.progress();
                let _ = write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE);
                let event = zmodem_event(
                    &awaiting.id,
                    &session_id,
                    awaiting.direction,
                    "failed",
                    Some(progress),
                    Some(error.to_string()),
                );
                let _ = app.emit("zmodem-event", event);
                return Err(error);
            }
        };
        let event = zmodem_event(
            &awaiting.id,
            &session_id,
            awaiting.direction,
            drive.status.unwrap_or("running"),
            Some(drive.progress),
            None,
        );
        terminal.zmodem = if drive.status == Some("completed")
            && awaiting.direction == ZmodemDirection::Download
        {
            let mut finishing = FinishingTransfer::new(drive.terminal_bytes);
            if let Some(bytes) = finishing.feed(&[]) {
                if !bytes.is_empty() {
                    let _ = app.emit(
                        "terminal-output",
                        TerminalOutput {
                            session_id: session_id.clone(),
                            data_base64: STANDARD.encode(bytes),
                        },
                    );
                }
                ZmodemState::default()
            } else {
                ZmodemState::Finishing(finishing)
            }
        } else if drive.status.is_some() {
            ZmodemState::default()
        } else {
            ZmodemState::Active(Box::new(active))
        };
        let _ = app.emit("zmodem-event", event.clone());
        Ok(event)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

pub async fn zmodem_cancel(
    app: AppHandle,
    manager: SessionManager,
    session_id: String,
    transfer_id: String,
) -> AppResult<ZmodemEvent> {
    let handle = manager.get(&session_id)?;
    tokio::task::spawn_blocking(move || {
        let mut terminal = handle.lock();
        let state = std::mem::take(&mut terminal.zmodem);
        let (id, direction, progress) = match state {
            ZmodemState::Awaiting(awaiting) if awaiting.id == transfer_id => {
                (awaiting.id, awaiting.direction, None)
            }
            ZmodemState::Active(active) if active_id(&active) == transfer_id => {
                let direction = active.direction();
                let progress = active.progress();
                (transfer_id, direction, Some(progress))
            }
            other => {
                terminal.zmodem = other;
                return Err(AppError::Unavailable("Zmodem 传输已结束或不匹配".into()));
            }
        };
        write_channel_input(&mut terminal.channel, CANCEL_SEQUENCE)?;
        let event = zmodem_event(&id, &session_id, direction, "cancelled", progress, None);
        let _ = app.emit("zmodem-event", event.clone());
        Ok(event)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

pub struct RemoteCommandResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: i32,
    pub truncated: bool,
}

pub async fn execute_profile_command(
    db: &Database,
    profile: &ConnectionProfile,
    command: &str,
    cancelled: Arc<std::sync::atomic::AtomicBool>,
    timeout: Duration,
) -> AppResult<RemoteCommandResult> {
    let connected = verified_connection(db, profile, false).await?;
    let command = command.to_owned();
    tokio::task::spawn_blocking(move || {
        let mut channel = connected.session.channel_session()?;
        channel.exec(&command)?;
        connected.transport.set_nonblocking(true)?;
        connected.session.set_blocking(false);
        let mut stdout = Vec::new();
        let mut stderr_bytes = Vec::new();
        let mut truncated = false;
        let started = Instant::now();
        let mut buffer = [0_u8; 32 * 1024];
        loop {
            if cancelled.load(std::sync::atomic::Ordering::Acquire) {
                let _ = channel.close();
                return Err(AppError::Unavailable("批量命令已取消".into()));
            }
            if started.elapsed() > timeout {
                let _ = channel.close();
                return Err(AppError::Unavailable("批量命令执行超时".into()));
            }
            let mut progress = false;
            match channel.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    append_limited(&mut stdout, &buffer[..size], &mut truncated);
                    progress = true;
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
            let mut stderr = channel.stderr();
            match stderr.read(&mut buffer) {
                Ok(size) if size > 0 => {
                    append_limited(&mut stderr_bytes, &buffer[..size], &mut truncated);
                    progress = true;
                }
                Ok(_) => {}
                Err(error) if error.kind() == ErrorKind::WouldBlock => {}
                Err(error) => return Err(error.into()),
            }
            if channel.eof() {
                break;
            }
            if !progress {
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        connected.transport.set_nonblocking(false)?;
        connected.session.set_blocking(true);
        channel.wait_close()?;
        let exit_code = channel.exit_status()?;
        Ok(RemoteCommandResult {
            stdout: String::from_utf8_lossy(&stdout).into_owned(),
            stderr: String::from_utf8_lossy(&stderr_bytes).into_owned(),
            exit_code,
            truncated,
        })
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn append_limited(target: &mut Vec<u8>, data: &[u8], truncated: &mut bool) {
    const LIMIT: usize = 5 * 1024 * 1024;
    let remaining = LIMIT.saturating_sub(target.len());
    if remaining > 0 {
        target.extend_from_slice(&data[..data.len().min(remaining)]);
    }
    if data.len() > remaining {
        *truncated = true;
    }
}

fn write_channel_input(channel: &mut Channel, data: &[u8]) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut offset = 0;
    while offset < data.len() {
        match channel.write(&data[offset..]) {
            Ok(0) => return Err(AppError::Remote("SSH 终端写入返回 0 字节".into())),
            Ok(written) => offset += written,
            Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
    loop {
        match channel.flush() {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
}

pub async fn terminal_resize(
    manager: SessionManager,
    session_id: String,
    cols: u32,
    rows: u32,
) -> AppResult<()> {
    validate_terminal_size(cols, rows)?;
    let handle = manager.get(&session_id)?;
    tokio::task::spawn_blocking(move || {
        let mut terminal = handle.lock();
        terminal.cols = cols;
        terminal.rows = rows;
        terminal
            .channel
            .request_pty_size(cols, rows, None, None)
            .map_err(AppError::from)
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

fn validate_terminal_size(cols: u32, rows: u32) -> AppResult<()> {
    if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(AppError::Validation("PTY 尺寸超出允许范围".into()));
    }
    Ok(())
}

pub async fn terminal_close(manager: SessionManager, session_id: String) -> AppResult<()> {
    let handle = manager
        .remove(&session_id)
        .ok_or(AppError::NotFound(session_id))?;
    tokio::task::spawn_blocking(move || {
        let mut terminal = handle.lock();
        terminal.closed = true;
        let _ = terminal.channel.send_eof();
        let _ = terminal.channel.close();
        terminal.transport.take();
        Ok(())
    })
    .await
    .map_err(|error| AppError::Internal(error.to_string()))?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ssh_blob(key_type: &str) -> Vec<u8> {
        let mut blob = (key_type.len() as u32).to_be_bytes().to_vec();
        blob.extend_from_slice(key_type.as_bytes());
        blob.extend_from_slice(b"fixture-public-key-body");
        blob
    }

    #[test]
    fn fido2_identity_filter_accepts_only_openssh_security_key_algorithms() {
        for key_type in FIDO2_KEY_TYPES {
            let blob = ssh_blob(key_type);
            assert_eq!(public_key_type(&blob), Some(key_type));
            assert!(is_fido2_key(&blob));
        }
        assert!(!is_fido2_key(&ssh_blob("ssh-ed25519")));
        assert!(!is_fido2_key(&ssh_blob("ecdsa-sha2-nistp256")));
        assert!(!is_fido2_key(&[0, 0, 0, 40, b's', b'k']));
    }

    #[test]
    fn fido2_fingerprint_uses_openssh_sha256_format() {
        assert_eq!(
            public_key_fingerprint(b""),
            "SHA256:47DEQpj8HBSa+/TImW+5JCeuQeRkm5NMpJWZG3hSuFU"
        );
        assert!(!public_key_fingerprint(b"fixture").contains('='));
    }

    #[test]
    fn fido2_agent_failures_keep_distinct_actionable_categories() {
        assert!(fido2_failure_message(Some("user canceled")).contains("已取消"));
        assert!(fido2_failure_message(Some("PIN invalid")).contains("PIN"));
        assert!(fido2_failure_message(Some("touch required")).contains("触摸"));
        assert!(fido2_failure_message(Some("device removed")).contains("拔出"));
        assert!(fido2_failure_message(Some("agent refused operation")).contains("仍已插入"));
    }

    #[tokio::test]
    async fn blocking_operation_timeout_returns_a_recoverable_error() {
        let started = Instant::now();
        let result = blocking_with_timeout("SSH 认证", Duration::from_millis(20), || {
            std::thread::sleep(Duration::from_millis(150));
            Ok(())
        })
        .await;
        assert!(
            matches!(result, Err(AppError::Unavailable(message)) if message.contains("系统凭据授权") && message.contains("重试"))
        );
        assert!(started.elapsed() < Duration::from_millis(100));
    }

    #[test]
    fn keepalive_schedule_fires_at_the_configured_interval() {
        let now = Instant::now();
        let interval = Duration::from_secs(KEEPALIVE_INTERVAL_SECONDS.into());
        assert!(!keepalive_is_due(
            now - interval + Duration::from_millis(1),
            now
        ));
        assert!(keepalive_is_due(now - interval, now));
    }

    #[test]
    fn tcp_keepalive_configuration_is_supported_on_the_test_socket() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).unwrap();
        let (_server, _) = listener.accept().unwrap();
        configure_tcp_keepalive(&client).unwrap();
    }

    fn tcp_bridge(mut left: TcpStream, mut right: TcpStream) {
        let mut left_read = left.try_clone().unwrap();
        let mut right_read = right.try_clone().unwrap();
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut left_read, &mut right);
        });
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut right_read, &mut left);
        });
    }
    fn controlled_tcp_proxy(
        target_port: u16,
        delay: Duration,
    ) -> (u16, Arc<std::sync::atomic::AtomicBool>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let running = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let proxy_running = running.clone();
        std::thread::spawn(move || {
            let (client, _) = listener.accept().unwrap();
            let upstream = TcpStream::connect(("127.0.0.1", target_port)).unwrap();
            client
                .set_read_timeout(Some(Duration::from_millis(50)))
                .unwrap();
            upstream
                .set_read_timeout(Some(Duration::from_millis(50)))
                .unwrap();
            let client_read = client.try_clone().unwrap();
            let upstream_read = upstream.try_clone().unwrap();
            let reverse_running = proxy_running.clone();
            let reverse = std::thread::spawn(move || {
                delayed_copy(upstream_read, client, reverse_running, delay);
            });
            delayed_copy(client_read, upstream, proxy_running, delay);
            let _ = reverse.join();
        });
        (port, running)
    }

    fn delayed_copy(
        mut source: TcpStream,
        mut destination: TcpStream,
        running: Arc<std::sync::atomic::AtomicBool>,
        delay: Duration,
    ) {
        use std::sync::atomic::Ordering as AtomicOrdering;
        let mut buffer = [0_u8; 32 * 1024];
        while running.load(AtomicOrdering::Relaxed) {
            match source.read(&mut buffer) {
                Ok(0) => break,
                Ok(read) => {
                    std::thread::sleep(delay);
                    if destination.write_all(&buffer[..read]).is_err() {
                        break;
                    }
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
        let _ = source.shutdown(std::net::Shutdown::Both);
        let _ = destination.shutdown(std::net::Shutdown::Both);
    }
    fn socks_proxy(target_port: u16) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            let mut greeting = [0_u8; 2];
            client.read_exact(&mut greeting).unwrap();
            let mut methods = vec![0_u8; greeting[1] as usize];
            client.read_exact(&mut methods).unwrap();
            client.write_all(&[5, 0]).unwrap();
            let mut head = [0_u8; 4];
            client.read_exact(&mut head).unwrap();
            match head[3] {
                1 => {
                    let mut address = [0_u8; 4];
                    client.read_exact(&mut address).unwrap();
                }
                3 => {
                    let mut length = [0_u8; 1];
                    client.read_exact(&mut length).unwrap();
                    let mut address = vec![0_u8; length[0] as usize];
                    client.read_exact(&mut address).unwrap();
                }
                4 => {
                    let mut address = [0_u8; 16];
                    client.read_exact(&mut address).unwrap();
                }
                _ => panic!("invalid SOCKS address"),
            };
            let mut requested_port = [0_u8; 2];
            client.read_exact(&mut requested_port).unwrap();
            assert_eq!(u16::from_be_bytes(requested_port), target_port);
            let upstream = TcpStream::connect(("127.0.0.1", target_port)).unwrap();
            client.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
            tcp_bridge(client, upstream);
        });
        port
    }
    fn http_proxy(target_port: u16) -> u16 {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            let (mut client, _) = listener.accept().unwrap();
            let mut request = Vec::new();
            let mut byte = [0_u8; 1];
            while !request.ends_with(b"\r\n\r\n") {
                client.read_exact(&mut byte).unwrap();
                request.push(byte[0]);
            }
            assert!(
                String::from_utf8_lossy(&request)
                    .starts_with(&format!("CONNECT 127.0.0.1:{target_port}"))
            );
            let upstream = TcpStream::connect(("127.0.0.1", target_port)).unwrap();
            client
                .write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n")
                .unwrap();
            tcp_bridge(client, upstream);
        });
        port
    }
    #[test]
    fn credential_reference_never_contains_secret() {
        assert_eq!(credential_ref("abc"), "connection:abc");
    }
    #[cfg(target_os = "windows")]
    #[test]
    fn windows_credential_manager_round_trips_connection_secret() {
        let id = format!("windows-credential-test-{}", Uuid::new_v4());
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_credential(&self.0);
            }
        }
        let _cleanup = Cleanup(id.clone());
        assert_eq!(
            save_credential(&id, "CNshell-Windows-秘密").unwrap(),
            credential_ref(&id)
        );
        assert_eq!(
            load_credential(&id).unwrap().as_deref(),
            Some("CNshell-Windows-秘密")
        );
        delete_credential(&id).unwrap();
        assert!(load_credential(&id).unwrap().is_none());
    }
    #[test]
    fn host_algorithm_names_are_stable() {
        assert_eq!(host_key_algorithm(HostKeyType::Ed25519), "ssh-ed25519");
    }
    #[test]
    fn host_and_port_formats_ipv4_dns_and_ipv6() {
        assert_eq!(host_port("example.test", 22), "example.test:22");
        assert_eq!(host_port("127.0.0.1", 22), "127.0.0.1:22");
        assert_eq!(host_port("::1", 22), "[::1]:22");
    }
    #[test]
    fn host_identity_requires_algorithm_and_fingerprint() {
        assert!(
            verify_host_identity(
                Some(("ssh-ed25519".into(), "SHA256:same".into())),
                "ssh-ed25519",
                "SHA256:same",
                false
            )
            .is_ok()
        );
        assert!(matches!(
            verify_host_identity(
                Some(("ssh-rsa".into(), "SHA256:same".into())),
                "ssh-ed25519",
                "SHA256:same",
                false
            ),
            Err(AppError::HostKeyChanged { .. })
        ));
        assert!(matches!(
            verify_host_identity(None, "ssh-ed25519", "SHA256:new", false),
            Err(AppError::HostKeyUnknown { .. })
        ));
        assert!(verify_host_identity(None, "ssh-ed25519", "SHA256:new", true).is_ok());
    }
    #[test]
    fn socks_request_has_domain_and_big_endian_port() {
        let host = b"example.com";
        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host);
        request.extend_from_slice(&22_u16.to_be_bytes());
        assert_eq!(&request[request.len() - 2..], &[0, 22]);
        assert_eq!(
            "::1".parse::<std::net::IpAddr>().unwrap(),
            std::net::IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        );
    }
    #[test]
    fn reconnect_policy_has_required_backoff_and_stops_on_security_errors() {
        assert_eq!(RECONNECT_DELAYS, [1, 2, 5, 10, 30]);
        assert!(!retryable(&AppError::Authentication("bad password".into())));
        assert!(!retryable(&AppError::HostKeyChanged {
            expected: "a".into(),
            actual: "b".into()
        }));
        assert!(retryable(&AppError::Remote("connection reset".into())));
    }
    #[tokio::test]
    async fn ssh_jump_cycles_are_rejected_before_network_access() {
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("cycle.sqlite"))
            .await
            .unwrap();
        let mut input = crate::models::SaveConnectionInput {
            id: "a".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "A".into(),
            host: "a.invalid".into(),
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
        input.id = "b".into();
        input.name = "B".into();
        input.host = "b.invalid".into();
        db.save_connection(&input, None).await.unwrap();
        let proxy_a = crate::models::SaveProxyInput {
            id: "proxy-a".into(),
            name: "via B".into(),
            proxy_type: "sshJump".into(),
            host: "".into(),
            port: 0,
            username: None,
            jump_connection_id: Some("b".into()),
            credential: None,
        };
        let proxy_b = crate::models::SaveProxyInput {
            id: "proxy-b".into(),
            name: "via A".into(),
            proxy_type: "sshJump".into(),
            host: "".into(),
            port: 0,
            username: None,
            jump_connection_id: Some("a".into()),
            credential: None,
        };
        db.save_proxy(&proxy_a, None).await.unwrap();
        db.save_proxy(&proxy_b, None).await.unwrap();
        input.id = "a".into();
        input.name = "A".into();
        input.host = "a.invalid".into();
        input.proxy_id = Some("proxy-a".into());
        db.save_connection(&input, None).await.unwrap();
        input.id = "b".into();
        input.name = "B".into();
        input.host = "b.invalid".into();
        input.proxy_id = Some("proxy-b".into());
        db.save_connection(&input, None).await.unwrap();
        let profile = db.get_connection("a").await.unwrap();
        assert!(
            matches!(transport_connection(&db,&profile).await,Err(AppError::Validation(message))if message.contains("循环"))
        );
    }
    #[test]
    fn terminal_size_limits_reject_ipc_abuse() {
        assert!(validate_terminal_size(0, 24).is_err());
        assert!(validate_terminal_size(80, 24).is_ok());
        assert!(validate_terminal_size(10_000, 24).is_err());
    }
    #[tokio::test]
    async fn live_ssh_openssh_exec_sftp_and_large_output() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let bad_key = std::env::var("CNSHELL_TEST_SSH_BAD_KEY").expect("CNSHELL_TEST_SSH_BAD_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: "integration".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "local sshd".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("integration.sqlite"))
            .await
            .unwrap();
        let input = crate::models::SaveConnectionInput {
            id: profile.id.clone(),
            folder_id: profile.folder_id.clone(),
            protocol: profile.protocol.clone(),
            name: profile.name.clone(),
            host: profile.host.clone(),
            port: profile.port,
            username: profile.username.clone(),
            auth_type: profile.auth_type.clone(),
            private_key_path: profile.private_key_path.clone(),
            certificate_path: profile.certificate_path.clone(),
            host_key_policy: profile.host_key_policy.clone(),
            note: profile.note.clone(),
            tags: profile.tags.clone(),
            encoding: profile.encoding.clone(),
            startup_command: profile.startup_command.clone(),
            proxy_id: profile.proxy_id.clone(),
            environment: profile.environment.clone(),
            credential: None,
        };
        db.save_connection(&input, None).await.unwrap();
        let first = match verified_connection(&db, &profile, false).await {
            Ok(_) => panic!("expected unknown host key"),
            Err(error) => error,
        };
        let (fingerprint, algorithm) = match first {
            AppError::HostKeyUnknown {
                fingerprint,
                algorithm,
            } => (fingerprint, algorithm),
            other => panic!("expected unknown host key, got {other}"),
        };
        db.trust_host(&profile.host, profile.port, &algorithm, &fingerprint)
            .await
            .unwrap();
        let mut wrong_profile = profile.clone();
        wrong_profile.private_key_path = Some(bad_key);
        assert!(matches!(
            verified_connection(&db, &wrong_profile, false).await,
            Err(AppError::Authentication(_))
        ));
        let wrong_diagnostics = diagnose(&db, &wrong_profile).await;
        assert_eq!(wrong_diagnostics.last().unwrap().stage, "authentication");
        assert!(!wrong_diagnostics.last().unwrap().ok);
        let diagnostics = diagnose(&db, &profile).await;
        assert_eq!(
            diagnostics
                .iter()
                .map(|item| item.stage.as_str())
                .collect::<Vec<_>>(),
            vec![
                "dns",
                "tcp",
                "hostKey",
                "authentication",
                "shell",
                "complete"
            ]
        );
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        let mut channel = connected.session.channel_session().unwrap();
        channel
            .exec("printf cnshell-ok; head -c 1048576 /dev/zero | tr '\\0' x")
            .unwrap();
        let mut output = Vec::new();
        channel.read_to_end(&mut output).unwrap();
        assert!(output.starts_with(b"cnshell-ok"));
        assert_eq!(output.len(), 1048586);
        let terminal_pool = TransportPool::default();
        let transport = terminal_pool.acquire(&db, &profile, false).await.unwrap();
        let mut pty = open_pty(transport, profile.clone(), 120, 36, false, false, false).unwrap();
        write_channel_input(&mut pty.channel, b"printf 'CNSHELL_PTY_INPUT_OK\\n'\n").unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        let mut pty_output = Vec::new();
        let mut pty_buffer = [0_u8; 4096];
        while !String::from_utf8_lossy(&pty_output).contains("CNSHELL_PTY_INPUT_OK") {
            match pty.channel.read(&mut pty_buffer) {
                Ok(0) if pty.channel.eof() => panic!("PTY closed before command output"),
                Ok(read) => pty_output.extend_from_slice(&pty_buffer[..read]),
                Err(error)
                    if error.kind() == ErrorKind::WouldBlock && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(2))
                }
                Err(error) => panic!("PTY read failed after input: {error}"),
            }
            assert!(Instant::now() < deadline, "PTY command output timed out");
        }
        write_channel_input(&mut pty.channel, b"printf 'CNSHELL_PTY_STILL_ONLINE\\n'\n").unwrap();
        let _ = pty.channel.send_eof();
        let _ = pty.channel.close();
        let disconnected = verified_connection(&db, &profile, false).await.unwrap();
        let mut closing = disconnected.session.channel_session().unwrap();
        closing.exec("kill -HUP $$").unwrap();
        let mut ignored = Vec::new();
        let _ = closing.read_to_end(&mut ignored);
        closing.wait_close().unwrap();
        drop(closing);
        drop(disconnected);
        let recovered = verified_connection(&db, &profile, false).await.unwrap();
        let mut recovery = recovered.session.channel_session().unwrap();
        recovery.exec("printf CNSHELL_RECONNECTED").unwrap();
        let mut recovery_output = String::new();
        recovery.read_to_string(&mut recovery_output).unwrap();
        recovery.wait_close().unwrap();
        assert_eq!(recovery_output, "CNSHELL_RECONNECTED");
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        let sftp = connected.session.sftp().unwrap();
        let remote = std::path::Path::new("/tmp/cnshell-integration.txt");
        {
            let mut file = sftp.create(remote).unwrap();
            file.write_all("中文-SFTP".as_bytes()).unwrap();
            file.fsync().unwrap();
        }
        {
            let mut file = sftp.open(remote).unwrap();
            let mut text = String::new();
            file.read_to_string(&mut text).unwrap();
            assert_eq!(text, "中文-SFTP");
        }
        sftp.unlink(remote).unwrap();
        let empty = std::path::Path::new("/tmp/cnshell-empty-dir");
        let _ = sftp.rmdir(empty);
        sftp.mkdir(empty, 0o755).unwrap();
        assert!(sftp.readdir(empty).unwrap().is_empty());
        sftp.rmdir(empty).unwrap();
        let special = std::path::Path::new("/tmp/CNshell 中文 空格-'-$ 文件.txt");
        {
            let mut file = sftp.create(special).unwrap();
            file.write_all(b"special").unwrap();
        }
        assert_eq!(sftp.stat(special).unwrap().size, Some(7));
        let link = std::path::Path::new("/tmp/cnshell-symbolic-link");
        let _ = sftp.unlink(link);
        sftp.symlink(special, link).unwrap();
        assert_eq!(sftp.readlink(link).unwrap(), special);
        sftp.unlink(link).unwrap();
        sftp.unlink(special).unwrap();
        let denied = std::path::Path::new("/tmp/cnshell-denied-dir");
        let _ = sftp.rmdir(denied);
        sftp.mkdir(denied, 0o700).unwrap();
        sftp.setstat(
            denied,
            ssh2::FileStat {
                size: None,
                uid: None,
                gid: None,
                perm: Some(0),
                atime: None,
                mtime: None,
            },
        )
        .unwrap();
        assert!(sftp.readdir(denied).is_err());
        sftp.setstat(
            denied,
            ssh2::FileStat {
                size: None,
                uid: None,
                gid: None,
                perm: Some(0o700),
                atime: None,
                mtime: None,
            },
        )
        .unwrap();
        sftp.rmdir(denied).unwrap();
        let many = std::path::Path::new("/tmp/cnshell-100k-files");
        {
            let mut channel = connected.session.channel_session().unwrap();
            channel.exec("rm -rf /tmp/cnshell-100k-files; mkdir /tmp/cnshell-100k-files; python3 -c 'import os; p=\"/tmp/cnshell-100k-files\"; [open(os.path.join(p, str(i)), \"wb\").close() for i in range(100000)]'").unwrap();
            let mut stderr = String::new();
            channel.stderr().read_to_string(&mut stderr).unwrap();
            channel.wait_close().unwrap();
            assert_eq!(channel.exit_status().unwrap(), 0, "{stderr}");
        }
        assert_eq!(sftp.readdir(many).unwrap().len(), 100_000);
        {
            let mut channel = connected.session.channel_session().unwrap();
            channel.exec("rm -rf /tmp/cnshell-100k-files").unwrap();
            let mut output = Vec::new();
            channel.read_to_end(&mut output).unwrap();
            channel.wait_close().unwrap();
        }
        let ssh_port = profile.port as u16;
        for (proxy_type, proxy_port) in [
            ("socks5", socks_proxy(ssh_port)),
            ("http", http_proxy(ssh_port)),
        ] {
            let proxy_id = format!("{proxy_type}-integration");
            let proxy_input = crate::models::SaveProxyInput {
                id: proxy_id.clone(),
                name: proxy_type.into(),
                proxy_type: proxy_type.into(),
                host: "127.0.0.1".into(),
                port: proxy_port as i64,
                username: None,
                jump_connection_id: None,
                credential: None,
            };
            db.save_proxy(&proxy_input, None).await.unwrap();
            let mut proxied = profile.clone();
            proxied.proxy_id = Some(proxy_id);
            assert!(verified_connection(&db, &proxied, false).await.is_ok());
        }
        let mut jump_input = input;
        jump_input.id = "jump-integration".into();
        jump_input.name = "jump".into();
        db.save_connection(&jump_input, None).await.unwrap();
        let jump_proxy = crate::models::SaveProxyInput {
            id: "jump-proxy".into(),
            name: "jump".into(),
            proxy_type: "sshJump".into(),
            host: "127.0.0.1".into(),
            port: profile.port,
            username: None,
            jump_connection_id: Some(jump_input.id),
            credential: None,
        };
        db.save_proxy(&jump_proxy, None).await.unwrap();
        let mut jumped = profile.clone();
        jumped.proxy_id = Some("jump-proxy".into());
        assert!(verified_connection(&db, &jumped, false).await.is_ok());
        let interrupted = std::path::Path::new("/tmp/cnshell-1gb.bin.cnshell-part-test");
        let completed = std::path::Path::new("/tmp/cnshell-1gb.bin");
        let _ = sftp.unlink(interrupted);
        let _ = sftp.unlink(completed);
        {
            let mut partial = sftp.create(interrupted).unwrap();
            partial.write_all(&vec![0x5a; 1024 * 1024]).unwrap();
            partial.fsync().unwrap();
        }
        assert!(sftp.stat(completed).is_err());
        sftp.unlink(interrupted).unwrap();
        let chunk = vec![0x5a; 1024 * 1024];
        let mut expected = Sha256::new();
        {
            let mut upload = sftp.create(interrupted).unwrap();
            for _ in 0..1024 {
                upload.write_all(&chunk).unwrap();
                expected.update(&chunk);
            }
            upload.fsync().unwrap();
        }
        assert_eq!(
            sftp.stat(interrupted).unwrap().size,
            Some(1024 * 1024 * 1024)
        );
        sftp.rename(
            interrupted,
            completed,
            Some(ssh2::RenameFlags::OVERWRITE | ssh2::RenameFlags::ATOMIC),
        )
        .unwrap();
        let mut actual = Sha256::new();
        {
            let mut download = sftp.open(completed).unwrap();
            let mut buffer = vec![0_u8; 256 * 1024];
            loop {
                let read = download.read(&mut buffer).unwrap();
                if read == 0 {
                    break;
                }
                actual.update(&buffer[..read]);
            }
        }
        assert_eq!(expected.finalize(), actual.finalize());
        sftp.unlink(completed).unwrap();
        let echo_listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let echo_port = echo_listener.local_addr().unwrap().port();
        std::thread::spawn(move || {
            for incoming in echo_listener.incoming().take(3) {
                let mut stream = incoming.unwrap();
                let mut bytes = [0_u8; 64];
                let read = stream.read(&mut bytes).unwrap();
                stream.write_all(&bytes[..read]).unwrap();
            }
        });
        let forward = crate::models::PortForward {
            id: "local-test".into(),
            connection_id: profile.id.clone(),
            forward_type: "local".into(),
            bind_host: "127.0.0.1".into(),
            bind_port: 0,
            destination_host: Some("127.0.0.1".into()),
            destination_port: Some(echo_port as i64),
            auto_start: false,
            status: None,
            error: None,
        };
        let manager = crate::tunnel::TunnelManager::default();
        let mut forward = forward;
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        forward.bind_port = probe.local_addr().unwrap().port() as i64;
        drop(probe);
        crate::tunnel::start(
            db.clone(),
            SessionManager::default(),
            manager.clone(),
            forward.clone(),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut stream = TcpStream::connect(("127.0.0.1", forward.bind_port as u16)).unwrap();
        stream.write_all(b"local-forward").unwrap();
        let mut reply = [0_u8; 13];
        stream.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"local-forward");
        manager.stop(&forward.id).unwrap();
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let remote_port = probe.local_addr().unwrap().port();
        drop(probe);
        let remote_forward = crate::models::PortForward {
            id: "remote-test".into(),
            connection_id: profile.id.clone(),
            forward_type: "remote".into(),
            bind_host: "127.0.0.1".into(),
            bind_port: remote_port as i64,
            destination_host: Some("127.0.0.1".into()),
            destination_port: Some(echo_port as i64),
            auto_start: false,
            status: None,
            error: None,
        };
        crate::tunnel::start(
            db.clone(),
            SessionManager::default(),
            manager.clone(),
            remote_forward.clone(),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut remote_stream = TcpStream::connect(("127.0.0.1", remote_port)).unwrap();
        remote_stream.write_all(b"remote-forward").unwrap();
        let mut remote_reply = [0_u8; 14];
        remote_stream.read_exact(&mut remote_reply).unwrap();
        assert_eq!(&remote_reply, b"remote-forward");
        manager.stop(&remote_forward.id).unwrap();
        let probe = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let dynamic_port = probe.local_addr().unwrap().port();
        drop(probe);
        let dynamic = crate::models::PortForward {
            id: "dynamic-test".into(),
            connection_id: profile.id.clone(),
            forward_type: "dynamic".into(),
            bind_host: "127.0.0.1".into(),
            bind_port: dynamic_port as i64,
            destination_host: None,
            destination_port: None,
            auto_start: false,
            status: None,
            error: None,
        };
        crate::tunnel::start(
            db.clone(),
            SessionManager::default(),
            manager.clone(),
            dynamic.clone(),
        )
        .await
        .unwrap();
        tokio::time::sleep(Duration::from_millis(100)).await;
        let mut socks = TcpStream::connect(("127.0.0.1", dynamic_port)).unwrap();
        socks.write_all(&[5, 1, 0]).unwrap();
        let mut method = [0_u8; 2];
        socks.read_exact(&mut method).unwrap();
        assert_eq!(method, [5, 0]);
        let mut request = vec![5, 1, 0, 1, 127, 0, 0, 1];
        request.extend_from_slice(&echo_port.to_be_bytes());
        socks.write_all(&request).unwrap();
        let mut accepted = [0_u8; 10];
        socks.read_exact(&mut accepted).unwrap();
        if accepted[1] != 0 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            panic!(
                "SOCKS5 CONNECT failed: response={accepted:?}, tunnel={:?}",
                manager.status(&dynamic.id)
            );
        }
        socks.write_all(b"dynamic-forward").unwrap();
        let mut reply = [0_u8; 15];
        socks.read_exact(&mut reply).unwrap();
        assert_eq!(&reply, b"dynamic-forward");
        manager.stop(&dynamic.id).unwrap();
        db.trust_host(
            &profile.host,
            profile.port,
            &algorithm,
            "SHA256:changed-host-key",
        )
        .await
        .unwrap();
        assert!(matches!(
            verified_connection(&db, &profile, false).await,
            Err(AppError::HostKeyChanged { .. })
        ));
    }

    #[tokio::test]
    async fn live_ssh_certificate_authentication_and_command() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let certificate = std::env::var("CNSHELL_TEST_SSH_CERT").expect("CNSHELL_TEST_SSH_CERT");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: format!("certificate-auth-{}", Uuid::new_v4()),
            folder_id: None,
            protocol: "ssh".into(),
            name: "certificate auth".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "sshCertificate".into(),
            private_key_path: Some(key),
            certificate_path: Some(certificate),
            host_key_policy: "acceptNew".into(),
            note: String::new(),
            tags: Vec::new(),
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: String::new(),
            updated_at: String::new(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("certificate.sqlite"))
            .await
            .unwrap();
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        let mut channel = connected.session.channel_session().unwrap();
        channel.exec("printf CNSHELL_CERTIFICATE_OK").unwrap();
        let mut output = String::new();
        channel.read_to_string(&mut output).unwrap();
        channel.wait_close().unwrap();
        assert_eq!(output, "CNSHELL_CERTIFICATE_OK");
        let info =
            crate::certificate::inspect(Path::new(profile.certificate_path.as_deref().unwrap()))
                .unwrap();
        assert!(info.valid_now);
        assert_eq!(info.key_id, "cnshell-certificate-test");
        assert!(info.principals.contains(&profile.username));
    }

    #[tokio::test]
    async fn live_ssh_x11_request_sets_remote_display() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: format!("x11-request-{}", Uuid::new_v4()),
            folder_id: None,
            protocol: "ssh".into(),
            name: "x11 request".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: String::new(),
            tags: Vec::new(),
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: String::new(),
            updated_at: String::new(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("x11-request.sqlite"))
            .await
            .unwrap();
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        let receiver = connected.session.x11_channel_receiver().unwrap();
        let mut channel = connected.session.channel_session().unwrap();
        channel
            .request_pty("xterm-256color", None, Some((80, 24, 0, 0)))
            .unwrap();
        channel
            .request_x11(
                false,
                Some("MIT-MAGIC-COOKIE-1"),
                Some("00112233445566778899aabbccddeeff"),
                0,
            )
            .unwrap();
        channel
            .exec("test -n \"$DISPLAY\" && printf CNSHELL_X11_REQUEST_OK")
            .unwrap();
        let mut output = String::new();
        channel.read_to_string(&mut output).unwrap();
        channel.wait_close().unwrap();
        assert_eq!(output, "CNSHELL_X11_REQUEST_OK");
        drop(receiver);
    }
    #[tokio::test]
    async fn live_ssh_password_authentication_and_rejection() {
        let Ok(port) = std::env::var("CNSHELL_TEST_PASSWORD_SSH_PORT") else {
            return;
        };
        let id = "integration-password";
        let _ = delete_credential(id);
        struct Cleanup(&'static str);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = delete_credential(self.0);
            }
        }
        let _cleanup = Cleanup(id);
        let profile = ConnectionProfile {
            id: id.into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "password ssh".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username: "cnshell".into(),
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
            has_credential: false,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("password.sqlite"))
            .await
            .unwrap();
        let unknown = match verified_connection(&db, &profile, false).await {
            Ok(_) => panic!("expected unknown key"),
            Err(error) => error,
        };
        let (fingerprint, algorithm) = match unknown {
            AppError::HostKeyUnknown {
                fingerprint,
                algorithm,
            } => (fingerprint, algorithm),
            other => panic!("expected unknown key, got {other}"),
        };
        db.trust_host(&profile.host, profile.port, &algorithm, &fingerprint)
            .await
            .unwrap();
        save_credential(id, "cnshell-test-password").unwrap();
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        let mut channel = connected.session.channel_session().unwrap();
        channel.exec("printf ignored").unwrap();
        let mut output = String::new();
        channel.read_to_string(&mut output).unwrap();
        assert_eq!(output, "cnshell-password-ok");
        save_credential(id, "wrong-password").unwrap();
        assert!(matches!(
            verified_connection(&db, &profile, false).await,
            Err(AppError::Authentication(_))
        ));
    }
    #[tokio::test]
    async fn live_ssh_user_space_network_delay_interrupt_and_recovery() {
        use std::sync::atomic::Ordering as AtomicOrdering;
        let Ok(target_port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let target_port: u16 = target_port.parse().unwrap();
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let (delayed_port, delayed_running) =
            controlled_tcp_proxy(target_port, Duration::from_millis(80));
        let mut profile = ConnectionProfile {
            id: "user-space-network".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "user-space network".into(),
            host: "127.0.0.1".into(),
            port: delayed_port.into(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "delay".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("user-space-network.sqlite"))
            .await
            .unwrap();
        let started = Instant::now();
        let delayed = verified_connection(&db, &profile, false).await.unwrap();
        let mut channel = delayed.session.channel_session().unwrap();
        channel.exec("printf CNSHELL_DELAY_OK").unwrap();
        let mut output = String::new();
        channel.read_to_string(&mut output).unwrap();
        channel.wait_close().unwrap();
        assert_eq!(output, "CNSHELL_DELAY_OK");
        assert!(started.elapsed() >= Duration::from_millis(160));
        delayed_running.store(false, AtomicOrdering::Relaxed);
        drop(channel);
        drop(delayed);

        let (interrupt_port, interrupt_running) = controlled_tcp_proxy(target_port, Duration::ZERO);
        profile.port = interrupt_port.into();
        profile.updated_at = "interrupt".into();
        let interrupted = verified_connection(&db, &profile, false).await.unwrap();
        let mut interrupted_channel = interrupted.session.channel_session().unwrap();
        interrupted_channel.exec("cat").unwrap();
        interrupt_running.store(false, AtomicOrdering::Relaxed);
        std::thread::sleep(Duration::from_millis(150));
        let mut buffer = [0_u8; 32];
        assert!(
            interrupted_channel.read(&mut buffer).is_err()
                || interrupted_channel.eof()
                || !interrupted.session.authenticated()
        );
        drop(interrupted_channel);
        drop(interrupted);

        let (recovery_port, recovery_running) = controlled_tcp_proxy(target_port, Duration::ZERO);
        profile.port = recovery_port.into();
        profile.updated_at = "recovery".into();
        let recovered = verified_connection(&db, &profile, false).await.unwrap();
        let mut recovery_channel = recovered.session.channel_session().unwrap();
        recovery_channel.exec("printf CNSHELL_RECOVERY_OK").unwrap();
        let mut recovery_output = String::new();
        recovery_channel
            .read_to_string(&mut recovery_output)
            .unwrap();
        recovery_channel.wait_close().unwrap();
        assert_eq!(recovery_output, "CNSHELL_RECOVERY_OK");
        recovery_running.store(false, AtomicOrdering::Relaxed);
    }
    #[tokio::test]
    async fn live_ssh_directory_transfer_round_trip_and_cleanup() {
        use std::sync::atomic::AtomicBool;
        fn directory_archives() -> std::collections::HashSet<std::ffi::OsString> {
            std::env::temp_dir()
                .read_dir()
                .unwrap()
                .filter_map(|entry| {
                    let name = entry.ok()?.file_name();
                    let text = name.to_string_lossy();
                    (text.starts_with("cnshell-directory-") && text.ends_with(".tar.gz"))
                        .then_some(name)
                })
                .collect()
        }
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: "directory-transfer".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "directory transfer".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "directory-transfer".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let archives_before = directory_archives();
        let source = directory.path().join("source-folder");
        std::fs::create_dir_all(source.join("nested")).unwrap();
        std::fs::write(source.join("root.txt"), b"root-content").unwrap();
        std::fs::write(source.join("nested/中文.txt"), "目录往返".as_bytes()).unwrap();
        let db = Database::open(&directory.path().join("directory-transfer.sqlite"))
            .await
            .unwrap();
        let manager = SessionManager::default();
        let transport = manager
            .acquire_transport(&db, &profile, false)
            .await
            .unwrap();
        let handle = open_pty(transport, profile.clone(), 80, 24, false, false, false).unwrap();
        let session_id = "directory-transfer-session".to_string();
        manager.insert(session_id.clone(), handle);
        let created_file = format!("/tmp/cnshell-created-{}.txt", Uuid::new_v4());
        crate::sftp::create_text(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            created_file.clone(),
        )
        .await
        .unwrap();
        assert!(
            crate::sftp::create_text(
                db.clone(),
                manager.clone(),
                session_id.clone(),
                created_file.clone(),
            )
            .await
            .is_err()
        );
        crate::sftp::delete(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            created_file,
            false,
        )
        .await
        .unwrap();
        let remote = format!("/tmp/cnshell-directory-roundtrip-{}", Uuid::new_v4());

        crate::sftp::transfer_directory(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            "upload".into(),
            source.to_string_lossy().into_owned(),
            remote.clone(),
            "overwrite".into(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        std::fs::write(source.join("replacement.txt"), b"replacement").unwrap();
        crate::sftp::transfer_directory(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            "upload".into(),
            source.to_string_lossy().into_owned(),
            remote.clone(),
            "overwrite".into(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        let downloaded = directory.path().join("downloaded-folder");
        std::fs::create_dir(&downloaded).unwrap();
        std::fs::write(downloaded.join("old.txt"), b"old").unwrap();
        crate::sftp::transfer_directory(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            "download".into(),
            remote.clone(),
            downloaded.to_string_lossy().into_owned(),
            "overwrite".into(),
            Arc::new(AtomicBool::new(false)),
        )
        .await
        .unwrap();
        assert_eq!(
            std::fs::read(downloaded.join("root.txt")).unwrap(),
            b"root-content"
        );
        assert_eq!(
            std::fs::read_to_string(downloaded.join("nested/中文.txt")).unwrap(),
            "目录往返"
        );
        assert_eq!(
            std::fs::read(downloaded.join("replacement.txt")).unwrap(),
            b"replacement"
        );
        assert!(!downloaded.join("old.txt").exists());

        let leftovers = crate::sftp::list(
            db.clone(),
            manager.clone(),
            session_id.clone(),
            "/tmp".into(),
            true,
        )
        .await
        .unwrap();
        assert!(
            !leftovers
                .iter()
                .any(|item| item.name.starts_with(".cnshell-directory-"))
        );
        crate::sftp::delete(db, manager, session_id, remote, true)
            .await
            .unwrap();
        assert_eq!(directory_archives(), archives_before);
    }
    #[tokio::test]
    async fn live_ssh_diagnostics_reports_dns_proxy_and_target_stages() {
        let Ok(target_port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let proxy_port = socks_proxy(target_port.parse().unwrap());
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("diagnostic-proxy.sqlite"))
            .await
            .unwrap();
        db.save_proxy(
            &crate::models::SaveProxyInput {
                id: "diagnostic-socks".into(),
                name: "SOCKS5 test".into(),
                proxy_type: "socks5".into(),
                host: "localhost".into(),
                port: proxy_port.into(),
                username: None,
                jump_connection_id: None,
                credential: None,
            },
            None,
        )
        .await
        .unwrap();
        let profile = ConnectionProfile {
            id: "diagnostic-proxy".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "diagnostic proxy".into(),
            host: "127.0.0.1".into(),
            port: target_port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: Some("diagnostic-socks".into()),
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "diagnostic".into(),
            last_connected_at: None,
        };
        let diagnostics = diagnose(&db, &profile).await;
        assert_eq!(
            diagnostics
                .iter()
                .map(|item| item.stage.as_str())
                .collect::<Vec<_>>(),
            vec![
                "dns",
                "proxy",
                "tcp",
                "hostKey",
                "authentication",
                "shell",
                "complete"
            ]
        );
        assert!(diagnostics.iter().all(|item| item.ok));
        assert!(diagnostics[0].message.contains("localhost"));
        assert!(diagnostics[1].message.contains("SOCKS5 test"));
    }
    #[tokio::test]
    async fn live_ssh_transport_pool_reuses_idle_and_expands_when_busy() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: "transport-pool".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "pool".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "v1".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("pool.sqlite"))
            .await
            .unwrap();
        let pool = TransportPool::default();
        {
            let lease = pool.acquire(&db, &profile, true).await.unwrap();
            let mut channel = lease.connected().session.channel_session().unwrap();
            channel.exec("true").unwrap();
            let mut output = Vec::new();
            channel.read_to_end(&mut output).unwrap();
            channel.wait_close().unwrap();
        }
        assert_eq!(pool.created(), 1);
        let first = pool.acquire(&db, &profile, true).await.unwrap();
        assert_eq!(pool.created(), 1);
        let second = pool.acquire(&db, &profile, true).await.unwrap();
        assert_eq!(pool.created(), 2);
        drop(first);
        drop(second);
        pool.invalidate(&profile.id);
        let third = pool.acquire(&db, &profile, true).await.unwrap();
        assert_eq!(pool.created(), 3);
        drop(third);
        let key = transport_pool_key(&profile);
        pool.idle.lock().get_mut(&key).unwrap()[0].idle_since =
            Instant::now() - MAX_IDLE_TRANSPORT_AGE - Duration::from_secs(1);
        let fourth = pool.acquire(&db, &profile, true).await.unwrap();
        assert_eq!(pool.created(), 4);
        drop(fourth);
        let exclusive = pool.acquire(&db, &profile, false).await.unwrap();
        assert_eq!(pool.created(), 5);
        let mut first_terminal =
            open_pty(exclusive, profile.clone(), 80, 24, false, false, false).unwrap();
        let _ = first_terminal.channel.send_eof();
        let _ = first_terminal.channel.close();
        drop(first_terminal);
        let next_exclusive = pool.acquire(&db, &profile, false).await.unwrap();
        assert_eq!(pool.created(), 6);
        let mut next_terminal =
            open_pty(next_exclusive, profile.clone(), 80, 24, false, false, false).unwrap();
        write_channel_input(&mut next_terminal.channel, b"exit\n").unwrap();
        let _ = next_terminal.channel.send_eof();
        let _ = next_terminal.channel.close();
    }

    #[tokio::test]
    async fn live_ssh_accept_new_records_once_and_rejects_changes() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let profile = ConnectionProfile {
            id: "accept-new".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "accept new".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("accept-new.sqlite"))
            .await
            .unwrap();
        assert!(
            db.known_host(&profile.host, profile.port)
                .await
                .unwrap()
                .is_none()
        );
        let first = verified_connection(&db, &profile, false).await.unwrap();
        let fingerprint = first.fingerprint.clone();
        let algorithm = first.algorithm.clone();
        drop(first);
        assert_eq!(
            db.known_host(&profile.host, profile.port).await.unwrap(),
            Some((algorithm.clone(), fingerprint.clone()))
        );
        assert!(verified_connection(&db, &profile, false).await.is_ok());
        db.trust_host(&profile.host, profile.port, "ssh-rsa", &fingerprint)
            .await
            .unwrap();
        assert!(matches!(
            verified_connection(&db, &profile, false).await,
            Err(AppError::HostKeyChanged { .. })
        ));
    }
    #[tokio::test]
    async fn live_ssh_private_key_authentication_uses_saved_bookmark() {
        let Ok(port) = std::env::var("CNSHELL_TEST_SSH_PORT") else {
            return;
        };
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").expect("CNSHELL_TEST_SSH_KEY");
        let username = std::env::var("CNSHELL_TEST_SSH_USER").expect("CNSHELL_TEST_SSH_USER");
        let id = format!("bookmark-auth-{}", Uuid::new_v4());
        struct Cleanup(String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = crate::bookmark::delete(&self.0);
            }
        }
        let _cleanup = Cleanup(id.clone());
        crate::bookmark::save(&id, Path::new(&key)).unwrap();
        let profile = ConnectionProfile {
            id,
            folder_id: None,
            protocol: "ssh".into(),
            name: "bookmark auth".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some("/missing/fallback/private-key".into()),
            certificate_path: None,
            host_key_policy: "acceptNew".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("bookmark-auth.sqlite"))
            .await
            .unwrap();
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        assert!(connected.session.authenticated());
    }
    #[tokio::test]
    async fn live_ssh_soak() {
        let Ok(seconds) = std::env::var("CNSHELL_SOAK_SECONDS") else {
            return;
        };
        let seconds: u64 = seconds.parse().expect("CNSHELL_SOAK_SECONDS");
        let port = std::env::var("CNSHELL_TEST_SSH_PORT").unwrap();
        let key = std::env::var("CNSHELL_TEST_SSH_KEY").unwrap();
        let username = std::env::var("CNSHELL_TEST_SSH_USER").unwrap();
        let profile = ConnectionProfile {
            id: "soak".into(),
            folder_id: None,
            protocol: "ssh".into(),
            name: "soak".into(),
            host: "127.0.0.1".into(),
            port: port.parse().unwrap(),
            username,
            auth_type: "privateKey".into(),
            private_key_path: Some(key),
            certificate_path: None,
            host_key_policy: "strict".into(),
            note: "".into(),
            tags: vec![],
            encoding: "UTF-8".into(),
            startup_command: None,
            proxy_id: None,
            environment: Default::default(),
            has_credential: false,
            created_at: "".into(),
            updated_at: "".into(),
            last_connected_at: None,
        };
        let directory = tempfile::tempdir().unwrap();
        let db = Database::open(&directory.path().join("soak.sqlite"))
            .await
            .unwrap();
        let unknown = match transport_connection(&db, &profile).await {
            Ok(value) => value,
            Err(error) => panic!("transport failed: {error}"),
        };
        db.trust_host(
            &profile.host,
            profile.port,
            &unknown.algorithm,
            &unknown.fingerprint,
        )
        .await
        .unwrap();
        drop(unknown);
        let connected = verified_connection(&db, &profile, false).await.unwrap();
        connected.session.set_keepalive(true, 30);
        let mut terminal = connected.session.channel_session().unwrap();
        terminal
            .request_pty("xterm-256color", None, Some((120, 36, 0, 0)))
            .unwrap();
        terminal.shell().unwrap();
        let marker = "CNSHELL_SOAK_READY";
        let latency_started = Instant::now();
        terminal
            .write_all(format!("printf '{marker}\\n'\n").as_bytes())
            .unwrap();
        terminal.flush().unwrap();
        let mut initial = Vec::new();
        let mut buffer = [0_u8; 4096];
        while !String::from_utf8_lossy(&initial).contains(marker) {
            let read = terminal.read(&mut buffer).unwrap();
            assert!(read > 0);
            initial.extend_from_slice(&buffer[..read]);
        }
        let latency_ms = latency_started.elapsed().as_millis();
        let rss = || -> u64 {
            let output = std::process::Command::new("ps")
                .args(["-o", "rss=", "-p", &std::process::id().to_string()])
                .output()
                .unwrap();
            String::from_utf8_lossy(&output.stdout)
                .trim()
                .parse()
                .unwrap_or(0)
        };
        let start_rss = rss();
        let mut peak_rss = start_rss;
        let started = Instant::now();
        let mut operations = 0_u64;
        while started.elapsed() < Duration::from_secs(seconds) {
            let mut channel = connected.session.channel_session().unwrap();
            channel
                .exec("printf __MONITOR__; uptime >/dev/null")
                .unwrap();
            let mut output = String::new();
            channel.read_to_string(&mut output).unwrap();
            assert_eq!(output, "__MONITOR__");
            channel.wait_close().unwrap();
            let _ = connected.session.keepalive_send();
            operations += 1;
            peak_rss = peak_rss.max(rss());
            tokio::time::sleep(
                Duration::from_secs(2)
                    .min(Duration::from_secs(seconds).saturating_sub(started.elapsed())),
            )
            .await;
        }
        let end_rss = rss();
        let _ = terminal.send_eof();
        let _ = terminal.close();
        assert!(operations > 0);
        assert!(
            end_rss.saturating_sub(start_rss) < 100 * 1024,
            "RSS grew by more than 100 MB"
        );
        println!(
            "CNSHELL_SOAK_REPORT duration_seconds={} operations={} pty_roundtrip_ms={} rss_start_kb={} rss_end_kb={} rss_peak_kb={}",
            started.elapsed().as_secs(),
            operations,
            latency_ms,
            start_rss,
            end_rss,
            peak_rss
        );
    }
}
