use crate::error::{AppError, AppResult};
use rand::{RngCore, rngs::OsRng};
use ssh2::{Channel, Session, X11ChannelReceiver};
#[cfg(unix)]
use std::os::unix::net::UnixStream;
#[cfg(target_os = "windows")]
use std::path::Path;
use std::{
    io::{ErrorKind, Read, Write},
    net::{IpAddr, SocketAddr, TcpStream},
    path::PathBuf,
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

const AUTH_PROTOCOL: &str = "MIT-MAGIC-COOKIE-1";
const COOKIE_BYTES: usize = 16;
const MAX_X11_CHANNELS: usize = 8;
const MAX_SETUP_BYTES: usize = 64 * 1024;
const SETUP_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Clone, Debug, Eq, PartialEq)]
enum DisplayEndpoint {
    #[cfg(unix)]
    Unix(PathBuf),
    Tcp(SocketAddr),
}

#[derive(Clone)]
pub struct X11Authorization {
    screen: i32,
    endpoint: DisplayEndpoint,
    real_cookie: [u8; COOKIE_BYTES],
    fake_cookie: [u8; COOKIE_BYTES],
}

impl X11Authorization {
    pub fn fake_cookie_hex(&self) -> String {
        hex(&self.fake_cookie)
    }

    pub fn screen(&self) -> i32 {
        self.screen
    }
}

pub struct X11Forwarder {
    stop: Arc<AtomicBool>,
    dispatcher: Option<JoinHandle<()>>,
}

impl Drop for X11Forwarder {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(dispatcher) = self.dispatcher.take() {
            let _ = dispatcher.join();
        }
    }
}

pub fn availability() -> Result<String, String> {
    let display = std::env::var("DISPLAY").map_err(|_| missing_display_message().to_string())?;
    let (_, endpoint) = parse_display(&display)?;
    validate_endpoint(&endpoint)?;
    let executable = xauth_path().ok_or_else(|| missing_xauth_message().to_string())?;
    Ok(executable.to_string_lossy().into_owned())
}

pub fn authorization() -> AppResult<X11Authorization> {
    let display = std::env::var("DISPLAY")
        .map_err(|_| AppError::Unavailable(missing_display_message().into()))?;
    let (screen, endpoint) = parse_display(&display).map_err(AppError::Unavailable)?;
    validate_endpoint(&endpoint).map_err(AppError::Unavailable)?;
    let executable =
        xauth_path().ok_or_else(|| AppError::Unavailable(missing_xauth_message().into()))?;
    let mut command = Command::new(&executable);
    command.args(["list", &display]).env_clear();
    configure_xauth_environment(&mut command, &executable, &display);
    let output = command
        .output()
        .map_err(|error| AppError::Unavailable(format!("无法读取 X11 授权：{error}")))?;
    if !output.status.success() || output.stdout.len() > 64 * 1024 {
        return Err(AppError::Unavailable(
            "本地 X Server 没有返回可用的 X11 授权 cookie".into(),
        ));
    }
    let text = String::from_utf8(output.stdout)
        .map_err(|_| AppError::Unavailable("X11 授权输出不是 UTF-8".into()))?;
    let real_cookie = parse_xauth(&text)?;
    let mut fake_cookie = [0_u8; COOKIE_BYTES];
    OsRng.fill_bytes(&mut fake_cookie);
    Ok(X11Authorization {
        screen,
        endpoint,
        real_cookie,
        fake_cookie,
    })
}

pub fn enable(session: &Session, channel: &mut Channel) -> AppResult<X11Forwarder> {
    let authorization = authorization()?;
    let receiver = session
        .x11_channel_receiver()
        .map_err(|error| AppError::Internal(format!("无法注册 X11 channel：{error}")))?;
    channel
        .request_x11(
            false,
            Some(AUTH_PROTOCOL),
            Some(&authorization.fake_cookie_hex()),
            authorization.screen(),
        )
        .map_err(|error| AppError::Remote(format!("服务端拒绝 X11 转发请求：{error}")))?;
    start(receiver, authorization)
}

pub fn start(
    receiver: X11ChannelReceiver,
    authorization: X11Authorization,
) -> AppResult<X11Forwarder> {
    let stop = Arc::new(AtomicBool::new(false));
    let active = Arc::new(AtomicUsize::new(0));
    let dispatcher_stop = stop.clone();
    let dispatcher = thread::Builder::new()
        .name("cnshell-x11-dispatch".into())
        .spawn(move || {
            while !dispatcher_stop.load(Ordering::Acquire) {
                let Ok(channel) = receiver.recv_timeout(Duration::from_millis(100)) else {
                    break;
                };
                let Some(channel) = channel else {
                    continue;
                };
                if active.fetch_add(1, Ordering::AcqRel) >= MAX_X11_CHANNELS {
                    active.fetch_sub(1, Ordering::AcqRel);
                    drop(channel);
                    continue;
                }
                let bridge_stop = dispatcher_stop.clone();
                let bridge_active = active.clone();
                let bridge_authorization = authorization.clone();
                if thread::Builder::new()
                    .name("cnshell-x11-channel".into())
                    .spawn(move || {
                        let _guard = ActiveChannel(bridge_active);
                        let _ = bridge(channel, &bridge_authorization, &bridge_stop);
                    })
                    .is_err()
                {
                    active.fetch_sub(1, Ordering::AcqRel);
                }
            }
        })
        .map_err(|error| AppError::Internal(format!("无法启动 X11 转发线程：{error}")))?;
    Ok(X11Forwarder {
        stop,
        dispatcher: Some(dispatcher),
    })
}

struct ActiveChannel(Arc<AtomicUsize>);

impl Drop for ActiveChannel {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}

trait LocalStream: Read + Write + Send {
    fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()>;
}

#[cfg(unix)]
impl LocalStream for UnixStream {
    fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        UnixStream::set_nonblocking(self, nonblocking)
    }
}

impl LocalStream for TcpStream {
    fn set_nonblocking(&self, nonblocking: bool) -> std::io::Result<()> {
        TcpStream::set_nonblocking(self, nonblocking)
    }
}

fn bridge(
    mut channel: Channel,
    authorization: &X11Authorization,
    stop: &AtomicBool,
) -> AppResult<()> {
    let mut local = connect_local(&authorization.endpoint)?;
    local.set_nonblocking(true)?;
    let setup = read_and_rewrite_setup(&mut channel, authorization, stop)?;
    write_local(&mut *local, &setup, stop)?;
    let mut from_ssh = [0_u8; 32 * 1024];
    let mut from_x = [0_u8; 32 * 1024];
    while !stop.load(Ordering::Acquire) {
        let mut progressed = false;
        match channel.read(&mut from_ssh) {
            Ok(0) if channel.eof() => break,
            Ok(0) => {}
            Ok(size) => {
                write_local(&mut *local, &from_ssh[..size], stop)?;
                progressed = true;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        match local.read(&mut from_x) {
            Ok(0) => break,
            Ok(size) => {
                write_channel(&mut channel, &from_x[..size], stop)?;
                progressed = true;
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        if !progressed {
            thread::sleep(Duration::from_millis(2));
        }
    }
    let _ = channel.send_eof();
    let _ = channel.close();
    Ok(())
}

fn read_and_rewrite_setup(
    channel: &mut Channel,
    authorization: &X11Authorization,
    stop: &AtomicBool,
) -> AppResult<Vec<u8>> {
    let deadline = Instant::now() + SETUP_TIMEOUT;
    let mut bytes = Vec::new();
    let mut buffer = [0_u8; 4096];
    loop {
        if stop.load(Ordering::Acquire) {
            return Err(AppError::Unavailable("X11 会话已关闭".into()));
        }
        match channel.read(&mut buffer) {
            Ok(0) if channel.eof() => {
                return Err(AppError::Remote("X11 channel 在授权前关闭".into()));
            }
            Ok(0) => {}
            Ok(size) => {
                if bytes.len().saturating_add(size) > MAX_SETUP_BYTES {
                    return Err(AppError::Validation("X11 初始化包超过 64 KB".into()));
                }
                bytes.extend_from_slice(&buffer[..size]);
                if rewrite_setup_cookie(
                    &mut bytes,
                    &authorization.fake_cookie,
                    &authorization.real_cookie,
                )? {
                    return Ok(bytes);
                }
            }
            Err(error) if error.kind() == ErrorKind::WouldBlock => {}
            Err(error) => return Err(error.into()),
        }
        if Instant::now() >= deadline {
            return Err(AppError::Unavailable("X11 初始化授权超时".into()));
        }
        thread::sleep(Duration::from_millis(2));
    }
}

fn rewrite_setup_cookie(
    bytes: &mut [u8],
    fake_cookie: &[u8; COOKIE_BYTES],
    real_cookie: &[u8; COOKIE_BYTES],
) -> AppResult<bool> {
    if bytes.len() < 12 {
        return Ok(false);
    }
    let little = match bytes[0] {
        b'l' => true,
        b'B' => false,
        _ => return Err(AppError::Validation("X11 初始化包字节序无效".into())),
    };
    let read_u16 = |offset: usize| {
        let value = [bytes[offset], bytes[offset + 1]];
        (if little {
            u16::from_le_bytes(value)
        } else {
            u16::from_be_bytes(value)
        }) as usize
    };
    let name_len = read_u16(6);
    let data_len = read_u16(8);
    let padded = |length: usize| (length + 3) & !3;
    let name_start = 12;
    let data_start = name_start + padded(name_len);
    let total = data_start.saturating_add(padded(data_len));
    if total > MAX_SETUP_BYTES {
        return Err(AppError::Validation("X11 初始化包长度无效".into()));
    }
    if bytes.len() < total {
        return Ok(false);
    }
    if name_len != AUTH_PROTOCOL.len()
        || &bytes[name_start..name_start + name_len] != AUTH_PROTOCOL.as_bytes()
        || data_len != COOKIE_BYTES
        || &bytes[data_start..data_start + data_len] != fake_cookie
    {
        return Err(AppError::Authentication(
            "X11 channel 未提供本会话的一次性授权 cookie".into(),
        ));
    }
    bytes[data_start..data_start + data_len].copy_from_slice(real_cookie);
    Ok(true)
}

fn connect_local(endpoint: &DisplayEndpoint) -> AppResult<Box<dyn LocalStream>> {
    match endpoint {
        #[cfg(unix)]
        DisplayEndpoint::Unix(path) => UnixStream::connect(path)
            .map(|stream| Box::new(stream) as Box<dyn LocalStream>)
            .map_err(|error| AppError::Unavailable(format!("无法连接本地 X11 socket：{error}"))),
        DisplayEndpoint::Tcp(address) => {
            TcpStream::connect_timeout(address, Duration::from_secs(3))
                .map(|stream| Box::new(stream) as Box<dyn LocalStream>)
                .map_err(|error| AppError::Unavailable(format!("无法连接本机 X11 端口：{error}")))
        }
    }
}

fn write_local(stream: &mut dyn LocalStream, bytes: &[u8], stop: &AtomicBool) -> AppResult<()> {
    write_nonblocking(bytes, stop, |remaining| stream.write(remaining))?;
    Ok(())
}

fn write_channel(channel: &mut Channel, bytes: &[u8], stop: &AtomicBool) -> AppResult<()> {
    write_nonblocking(bytes, stop, |remaining| channel.write(remaining))?;
    Ok(())
}

fn write_nonblocking(
    bytes: &[u8],
    stop: &AtomicBool,
    mut write: impl FnMut(&[u8]) -> std::io::Result<usize>,
) -> std::io::Result<()> {
    let mut offset = 0;
    while offset < bytes.len() && !stop.load(Ordering::Acquire) {
        match write(&bytes[offset..]) {
            Ok(0) => return Err(std::io::Error::new(ErrorKind::WriteZero, "X11 write zero")),
            Ok(size) => offset += size,
            Err(error) if error.kind() == ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(2));
            }
            Err(error) => return Err(error),
        }
    }
    Ok(())
}

fn parse_xauth(output: &str) -> AppResult<[u8; COOKIE_BYTES]> {
    for line in output.lines() {
        let fields = line.split_whitespace().collect::<Vec<_>>();
        if fields.len() >= 3 && fields[1] == AUTH_PROTOCOL {
            let bytes = decode_hex(fields[2])?;
            return bytes.try_into().map_err(|_| {
                AppError::Validation("X11 MIT-MAGIC-COOKIE-1 长度不是 16 字节".into())
            });
        }
    }
    Err(AppError::Unavailable(
        "本地 X Server 没有 MIT-MAGIC-COOKIE-1 授权".into(),
    ))
}

fn decode_hex(value: &str) -> AppResult<Vec<u8>> {
    if !value.len().is_multiple_of(2) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(AppError::Validation("X11 cookie 不是有效十六进制".into()));
    }
    (0..value.len())
        .step_by(2)
        .map(|index| {
            u8::from_str_radix(&value[index..index + 2], 16)
                .map_err(|_| AppError::Validation("X11 cookie 不是有效十六进制".into()))
        })
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn xauth_path() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let mut candidates = Vec::new();
        for root in [
            std::env::var_os("ProgramFiles"),
            std::env::var_os("ProgramFiles(x86)"),
        ]
        .into_iter()
        .flatten()
        {
            candidates.push(PathBuf::from(&root).join("VcXsrv").join("xauth.exe"));
            candidates.push(PathBuf::from(&root).join("Xming").join("xauth.exe"));
        }
        candidates
            .into_iter()
            .find(|path| path.is_file())
            .or_else(|| which::which("xauth.exe").ok())
    }
    #[cfg(not(target_os = "windows"))]
    ["/opt/X11/bin/xauth", "/usr/X11/bin/xauth"]
        .into_iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
        .or_else(|| which::which("xauth").ok())
}

#[cfg(target_os = "windows")]
fn configure_xauth_environment(command: &mut Command, executable: &Path, display: &str) {
    let system_root = std::env::var_os("SystemRoot").unwrap_or_else(|| "C:\\Windows".into());
    let mut paths = vec![
        executable
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf(),
    ];
    paths.push(PathBuf::from(system_root).join("System32"));
    if let Ok(path) = std::env::join_paths(paths) {
        command.env("PATH", path);
    }
    command.env("DISPLAY", display).env(
        "USERPROFILE",
        std::env::var_os("USERPROFILE").unwrap_or_default(),
    );
}

#[cfg(not(target_os = "windows"))]
fn configure_xauth_environment(command: &mut Command, _executable: &PathBuf, display: &str) {
    command
        .env("PATH", "/opt/X11/bin:/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DISPLAY", display)
        .env("HOME", std::env::var("HOME").unwrap_or_default());
}

#[cfg(target_os = "windows")]
fn missing_display_message() -> &'static str {
    "未检测到 DISPLAY；请先启动 VcXsrv、Xming 或其他本地 X Server"
}

#[cfg(not(target_os = "windows"))]
fn missing_display_message() -> &'static str {
    "未检测到 DISPLAY；请先启动 XQuartz"
}

#[cfg(target_os = "windows")]
fn missing_xauth_message() -> &'static str {
    "未检测到本地 X Server 的 xauth.exe"
}

#[cfg(not(target_os = "windows"))]
fn missing_xauth_message() -> &'static str {
    "未检测到 XQuartz xauth"
}

fn parse_display(value: &str) -> Result<(i32, DisplayEndpoint), String> {
    let (host, display) = value
        .rsplit_once(':')
        .ok_or_else(|| "DISPLAY 格式无效".to_string())?;
    let display_number = display
        .split('.')
        .next()
        .ok_or_else(|| "DISPLAY 缺少显示编号".to_string())?
        .parse::<u16>()
        .map_err(|_| "DISPLAY 显示编号无效".to_string())?;
    let screen = display
        .split_once('.')
        .map(|(_, screen)| screen.parse::<i32>())
        .transpose()
        .map_err(|_| "DISPLAY 屏幕编号无效".to_string())?
        .unwrap_or(0);
    if !(0..=255).contains(&screen) {
        return Err("DISPLAY 屏幕编号超出范围".into());
    }
    let endpoint = if host.is_empty() || host == "unix" {
        #[cfg(unix)]
        {
            DisplayEndpoint::Unix(PathBuf::from(format!("/tmp/.X11-unix/X{display_number}")))
        }
        #[cfg(not(unix))]
        {
            let port = 6000_u16
                .checked_add(display_number)
                .ok_or_else(|| "DISPLAY 端口超出范围".to_string())?;
            DisplayEndpoint::Tcp(SocketAddr::new(
                IpAddr::V4(std::net::Ipv4Addr::LOCALHOST),
                port,
            ))
        }
    } else if host.starts_with('/') {
        #[cfg(unix)]
        {
            DisplayEndpoint::Unix(PathBuf::from(host))
        }
        #[cfg(not(unix))]
        {
            return Err("Windows X11 DISPLAY 必须使用本机 TCP 地址".into());
        }
    } else {
        let ip = host
            .trim_matches(['[', ']'])
            .parse::<IpAddr>()
            .map_err(|_| "只允许本机 X11 display".to_string())?;
        if !ip.is_loopback() {
            return Err("只允许本机 X11 display".into());
        }
        let port = 6000_u16
            .checked_add(display_number)
            .ok_or_else(|| "DISPLAY 端口超出范围".to_string())?;
        DisplayEndpoint::Tcp(SocketAddr::new(ip, port))
    };
    Ok((screen, endpoint))
}

fn validate_endpoint(endpoint: &DisplayEndpoint) -> Result<(), String> {
    match endpoint {
        #[cfg(unix)]
        DisplayEndpoint::Unix(path) if path.exists() => Ok(()),
        #[cfg(unix)]
        DisplayEndpoint::Unix(_) => Err("XQuartz socket 不存在；请确认 XQuartz 正在运行".into()),
        DisplayEndpoint::Tcp(address) => {
            TcpStream::connect_timeout(address, Duration::from_millis(300))
                .map(|_| ())
                .map_err(|_| "本机 X11 端口不可用；请确认本地 X Server 正在运行".into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn setup_packet(order: u8, cookie: [u8; COOKIE_BYTES]) -> Vec<u8> {
        let little = order == b'l';
        let encode = |value: u16| {
            if little {
                value.to_le_bytes()
            } else {
                value.to_be_bytes()
            }
        };
        let mut packet = vec![order, 0];
        packet.extend_from_slice(&encode(11));
        packet.extend_from_slice(&encode(0));
        packet.extend_from_slice(&encode(AUTH_PROTOCOL.len() as u16));
        packet.extend_from_slice(&encode(COOKIE_BYTES as u16));
        packet.extend_from_slice(&[0, 0]);
        packet.extend_from_slice(AUTH_PROTOCOL.as_bytes());
        while packet.len() % 4 != 0 {
            packet.push(0);
        }
        packet.extend_from_slice(&cookie);
        packet
    }

    #[test]
    fn rewrites_only_the_expected_one_time_cookie_for_both_byte_orders() {
        let fake = [0x11; COOKIE_BYTES];
        let real = [0x22; COOKIE_BYTES];
        for order in [b'l', b'B'] {
            let mut packet = setup_packet(order, fake);
            assert!(rewrite_setup_cookie(&mut packet, &fake, &real).unwrap());
            assert!(packet.ends_with(&real));
            let mut rejected = setup_packet(order, [0x33; COOKIE_BYTES]);
            assert!(rewrite_setup_cookie(&mut rejected, &fake, &real).is_err());
        }
    }

    #[test]
    fn waits_for_complete_setup_and_rejects_oversized_lengths() {
        let fake = [0x11; COOKIE_BYTES];
        let real = [0x22; COOKIE_BYTES];
        let packet = setup_packet(b'l', fake);
        assert!(!rewrite_setup_cookie(&mut packet[..10].to_vec(), &fake, &real).unwrap());
        let mut invalid = packet;
        invalid[6..8].copy_from_slice(&u16::MAX.to_le_bytes());
        assert!(rewrite_setup_cookie(&mut invalid, &fake, &real).is_err());
    }

    #[test]
    fn parses_only_local_displays_and_strict_xauth_cookie() {
        #[cfg(unix)]
        assert_eq!(
            parse_display(":0").unwrap(),
            (0, DisplayEndpoint::Unix(PathBuf::from("/tmp/.X11-unix/X0")))
        );
        #[cfg(not(unix))]
        assert_eq!(
            parse_display(":0").unwrap(),
            (
                0,
                DisplayEndpoint::Tcp(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6000,)),
            )
        );
        #[cfg(unix)]
        assert_eq!(
            parse_display("/private/tmp/xquartz:1.2").unwrap(),
            (
                2,
                DisplayEndpoint::Unix(PathBuf::from("/private/tmp/xquartz"))
            )
        );
        #[cfg(not(unix))]
        assert!(parse_display("/private/tmp/xquartz:1.2").is_err());
        assert_eq!(
            parse_display("127.0.0.1:3").unwrap(),
            (
                0,
                DisplayEndpoint::Tcp(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), 6003))
            )
        );
        assert!(parse_display("192.0.2.10:0").is_err());
        let cookie =
            parse_xauth("host/unix:0 MIT-MAGIC-COOKIE-1 00112233445566778899aabbccddeeff\n")
                .unwrap();
        assert_eq!(
            cookie,
            [
                0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd,
                0xee, 0xff
            ]
        );
    }
}
