use crate::error::{AppError, AppResult};
use ssh2::Channel;
use std::{
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
    time::{Duration, Instant},
};
use zmodem2::{Action, Event, FileInfo, Position, Receiver, Sender};

const ZHEX_PREFIX: &[u8; 4] = b"**\x18B";
const ZHEX_HEADER_LEN: usize = 18;
const MAX_PENDING_WIRE_BYTES: usize = 1024 * 1024;
const MAX_FILE_SIZE: u64 = u32::MAX as u64;
const AUTHORIZATION_TIMEOUT: Duration = Duration::from_secs(120);
const FINISH_TIMEOUT: Duration = Duration::from_secs(2);
pub const CANCEL_SEQUENCE: &[u8] =
    b"\x18\x18\x18\x18\x18\x18\x18\x18\x08\x08\x08\x08\x08\x08\x08\x08";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    Download,
    Upload,
}

impl Direction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Download => "download",
            Self::Upload => "upload",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
pub struct Detection {
    pub direction: Direction,
    pub protocol_bytes: Vec<u8>,
}

#[derive(Debug, Default, Eq, PartialEq)]
pub struct DetectionOutput {
    pub terminal_bytes: Vec<u8>,
    pub detection: Option<Detection>,
}

#[derive(Default)]
pub struct HandshakeDetector {
    pending: Vec<u8>,
}

pub struct AwaitingTransfer {
    pub id: String,
    pub direction: Direction,
    pub protocol_bytes: Vec<u8>,
    pub detected_at: Instant,
}

impl AwaitingTransfer {
    pub fn expired(&self) -> bool {
        self.detected_at.elapsed() >= AUTHORIZATION_TIMEOUT
    }

    pub fn append(&mut self, bytes: &[u8]) -> AppResult<()> {
        if self.protocol_bytes.len().saturating_add(bytes.len()) > MAX_PENDING_WIRE_BYTES {
            return Err(AppError::Unavailable(
                "Zmodem 等待授权期间收到的数据过多，传输已取消".into(),
            ));
        }
        self.protocol_bytes.extend_from_slice(bytes);
        Ok(())
    }
}

pub enum SessionState {
    Detecting(HandshakeDetector),
    Awaiting(AwaitingTransfer),
    Active(Box<ActiveTransfer>),
    Finishing(FinishingTransfer),
}

pub struct FinishingTransfer {
    pending: Vec<u8>,
    started_at: Instant,
}

impl FinishingTransfer {
    pub fn new(initial: Vec<u8>) -> Self {
        Self {
            pending: initial,
            started_at: Instant::now(),
        }
    }

    pub fn feed(&mut self, bytes: &[u8]) -> Option<Vec<u8>> {
        self.pending.extend_from_slice(bytes);
        match self.pending.as_slice() {
            [b'O'] => None,
            [b'O', b'O', rest @ ..] => Some(rest.to_vec()),
            _ => Some(std::mem::take(&mut self.pending)),
        }
    }

    pub fn expired(&self) -> bool {
        self.started_at.elapsed() >= FINISH_TIMEOUT
    }
}

impl Default for SessionState {
    fn default() -> Self {
        Self::Detecting(HandshakeDetector::default())
    }
}

pub struct TransferProgress {
    pub file_name: Option<String>,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
}

pub struct DriveResult {
    pub status: Option<&'static str>,
    pub progress: TransferProgress,
    pub terminal_bytes: Vec<u8>,
}

pub enum ActiveTransfer {
    Download(DownloadTransfer),
    Upload(UploadTransfer),
}

pub struct DownloadTransfer {
    pub id: String,
    receiver: Receiver,
    destination: PathBuf,
    incoming: Vec<u8>,
    file: Option<File>,
    temporary_path: Option<PathBuf>,
    final_path: Option<PathBuf>,
    file_name: Option<String>,
    total_bytes: Option<u64>,
    transferred_bytes: u64,
}

pub struct UploadTransfer {
    pub id: String,
    sender: Sender,
    files: Vec<UploadFile>,
    current: usize,
    file_name: String,
    total_bytes: u64,
    transferred_bytes: u64,
    incoming: Vec<u8>,
}

struct UploadFile {
    source: File,
    file_name: String,
    total_bytes: u64,
}

impl ActiveTransfer {
    pub fn download(destination: PathBuf, incoming: Vec<u8>, id: String) -> AppResult<Self> {
        if !destination.is_dir() {
            return Err(AppError::Validation("请选择有效的 Zmodem 下载目录".into()));
        }
        Ok(Self::Download(DownloadTransfer {
            id,
            receiver: Receiver::new().map_err(protocol_error)?,
            destination,
            incoming,
            file: None,
            temporary_path: None,
            final_path: None,
            file_name: None,
            total_bytes: None,
            transferred_bytes: 0,
        }))
    }

    pub fn upload(sources: Vec<PathBuf>, incoming: Vec<u8>, id: String) -> AppResult<Self> {
        if sources.is_empty() || sources.len() > 100 {
            return Err(AppError::Validation(
                "Zmodem 上传一次需要选择 1 到 100 个文件".into(),
            ));
        }
        let mut files = Vec::with_capacity(sources.len());
        for source in sources {
            let metadata = source
                .metadata()
                .map_err(|error| AppError::Storage(format!("读取上传文件失败：{error}")))?;
            if !metadata.is_file() {
                return Err(AppError::Validation("Zmodem 上传只能选择普通文件".into()));
            }
            if metadata.len() > MAX_FILE_SIZE {
                return Err(AppError::Validation("单个 Zmodem 文件不能超过 4 GB".into()));
            }
            let file_name = source
                .file_name()
                .and_then(|name| name.to_str())
                .filter(|name| !name.is_empty())
                .ok_or_else(|| AppError::Validation("上传文件名无效".into()))?
                .to_owned();
            let source = File::open(&source)
                .map_err(|error| AppError::Storage(format!("打开上传文件失败：{error}")))?;
            files.push(UploadFile {
                source,
                file_name,
                total_bytes: metadata.len(),
            });
        }
        let first = &files[0];
        let mut sender = Sender::new().map_err(protocol_error)?;
        sender
            .start_file(FileInfo::new(
                first.file_name.as_bytes(),
                Some(Position::new(first.total_bytes as u32)),
            ))
            .map_err(protocol_error)?;
        Ok(Self::Upload(UploadTransfer {
            id,
            sender,
            file_name: first.file_name.clone(),
            total_bytes: first.total_bytes,
            files,
            current: 0,
            transferred_bytes: 0,
            incoming,
        }))
    }

    pub fn direction(&self) -> Direction {
        match self {
            Self::Download(_) => Direction::Download,
            Self::Upload(_) => Direction::Upload,
        }
    }

    pub fn progress(&self) -> TransferProgress {
        match self {
            Self::Download(transfer) => TransferProgress {
                file_name: transfer.file_name.clone(),
                total_bytes: transfer.total_bytes,
                transferred_bytes: transfer.transferred_bytes,
            },
            Self::Upload(transfer) => TransferProgress {
                file_name: Some(transfer.file_name.clone()),
                total_bytes: Some(transfer.total_bytes),
                transferred_bytes: transfer.transferred_bytes,
            },
        }
    }

    pub fn append_wire(&mut self, input: &[u8]) -> AppResult<()> {
        let incoming = match self {
            Self::Download(transfer) => &mut transfer.incoming,
            Self::Upload(transfer) => &mut transfer.incoming,
        };
        if incoming.len().saturating_add(input.len()) > MAX_PENDING_WIRE_BYTES {
            return Err(AppError::Unavailable(
                "Zmodem 协议缓冲区已满，传输已中止".into(),
            ));
        }
        incoming.extend_from_slice(input);
        Ok(())
    }

    pub fn drive(&mut self, channel: &mut Channel) -> AppResult<DriveResult> {
        match self {
            Self::Download(transfer) => transfer.drive(channel),
            Self::Upload(transfer) => transfer.drive(channel),
        }
    }
}

impl DownloadTransfer {
    fn drive(&mut self, channel: &mut Channel) -> AppResult<DriveResult> {
        let mut status = None;
        for _ in 0..4096 {
            match self.receiver.poll() {
                Action::WriteWire(bytes) => {
                    let bytes = bytes.to_vec();
                    write_wire(channel, &bytes)?;
                    self.receiver.wire_written(bytes.len());
                }
                Action::WriteFile(bytes) => {
                    let bytes = bytes.to_vec();
                    let file = self.file.as_mut().ok_or_else(|| {
                        AppError::Remote("Zmodem 在文件元数据之前发送了内容".into())
                    })?;
                    file.write_all(&bytes)
                        .map_err(|error| AppError::Storage(format!("写入下载文件失败：{error}")))?;
                    self.receiver
                        .file_written(bytes.len())
                        .map_err(protocol_error)?;
                    self.transferred_bytes =
                        self.transferred_bytes.saturating_add(bytes.len() as u64);
                }
                Action::Event(Event::FileStarted(info)) => {
                    let remote_name = String::from_utf8_lossy(info.name).into_owned();
                    let file_name = safe_file_name(&remote_name)?;
                    let path = available_path(&self.destination, &file_name);
                    let temporary_path = temporary_download_path(&self.destination);
                    self.file = Some(
                        OpenOptions::new()
                            .write(true)
                            .create_new(true)
                            .open(&temporary_path)
                            .map_err(|error| {
                                AppError::Storage(format!("创建下载文件失败：{error}"))
                            })?,
                    );
                    self.file_name = path
                        .file_name()
                        .map(|name| name.to_string_lossy().into_owned());
                    self.temporary_path = Some(temporary_path);
                    self.final_path = Some(path);
                    self.total_bytes = info.size.map(|size| u64::from(size.get()));
                    self.transferred_bytes = 0;
                    status = Some("running");
                }
                Action::Event(Event::FileCompleted) => {
                    if let Some(file) = self.file.as_mut() {
                        file.flush().map_err(|error| {
                            AppError::Storage(format!("保存下载文件失败：{error}"))
                        })?;
                    }
                    self.file = None;
                    let temporary_path = self
                        .temporary_path
                        .take()
                        .ok_or_else(|| AppError::Internal("Zmodem 下载临时路径丢失".into()))?;
                    let final_path = self
                        .final_path
                        .take()
                        .ok_or_else(|| AppError::Internal("Zmodem 下载目标路径丢失".into()))?;
                    std::fs::rename(&temporary_path, &final_path)
                        .map_err(|error| AppError::Storage(format!("提交下载文件失败：{error}")))?;
                }
                Action::Event(Event::SessionCompleted) => {
                    status = Some("completed");
                }
                Action::Event(Event::Aborted) => {
                    status = Some("cancelled");
                    break;
                }
                Action::Idle => {
                    if status.is_some() {
                        break;
                    }
                    if self.incoming.is_empty() {
                        break;
                    }
                    let consumed = self
                        .receiver
                        .submit_wire(&self.incoming)
                        .map_err(protocol_error)?;
                    if consumed == 0 {
                        break;
                    }
                    self.incoming.drain(..consumed);
                }
                _ => return Err(AppError::Internal("Zmodem 接收器返回了无效操作".into())),
            }
        }
        let terminal_bytes = if matches!(status, Some("completed" | "cancelled")) {
            std::mem::take(&mut self.incoming)
        } else {
            Vec::new()
        };
        Ok(DriveResult {
            status,
            progress: TransferProgress {
                file_name: self.file_name.clone(),
                total_bytes: self.total_bytes,
                transferred_bytes: self.transferred_bytes,
            },
            terminal_bytes,
        })
    }
}

impl UploadTransfer {
    fn drive(&mut self, channel: &mut Channel) -> AppResult<DriveResult> {
        let mut status = None;
        for _ in 0..4096 {
            match self.sender.poll() {
                Action::WriteWire(bytes) => {
                    let bytes = bytes.to_vec();
                    write_wire(channel, &bytes)?;
                    self.sender.wire_written(bytes.len());
                }
                Action::ReadFile { offset, max_len } => {
                    let current = &mut self.files[self.current];
                    current
                        .source
                        .seek(SeekFrom::Start(u64::from(offset.get())))
                        .map_err(|error| AppError::Storage(format!("定位上传文件失败：{error}")))?;
                    let remaining =
                        current.total_bytes.saturating_sub(u64::from(offset.get())) as usize;
                    let mut bytes = vec![0_u8; max_len.min(remaining)];
                    current
                        .source
                        .read_exact(&mut bytes)
                        .map_err(|error| AppError::Storage(format!("读取上传文件失败：{error}")))?;
                    self.sender.submit_file(&bytes).map_err(protocol_error)?;
                    self.transferred_bytes =
                        u64::from(offset.get()).saturating_add(bytes.len() as u64);
                }
                Action::Event(Event::FileCompleted) => {
                    self.current += 1;
                    if let Some(next) = self.files.get(self.current) {
                        self.file_name = next.file_name.clone();
                        self.total_bytes = next.total_bytes;
                        self.transferred_bytes = 0;
                        self.sender
                            .start_file(FileInfo::new(
                                next.file_name.as_bytes(),
                                Some(Position::new(next.total_bytes as u32)),
                            ))
                            .map_err(protocol_error)?;
                    } else {
                        self.sender.finish().map_err(protocol_error)?;
                    }
                }
                Action::Event(Event::SessionCompleted) => {
                    status = Some("completed");
                }
                Action::Event(Event::Aborted) => {
                    status = Some("cancelled");
                    break;
                }
                Action::Idle => {
                    if status.is_some() {
                        break;
                    }
                    if self.incoming.is_empty() {
                        break;
                    }
                    let consumed = self
                        .sender
                        .submit_wire(&self.incoming)
                        .map_err(protocol_error)?;
                    if consumed == 0 {
                        break;
                    }
                    self.incoming.drain(..consumed);
                }
                _ => return Err(AppError::Internal("Zmodem 发送器返回了无效操作".into())),
            }
        }
        let terminal_bytes = if matches!(status, Some("completed" | "cancelled")) {
            std::mem::take(&mut self.incoming)
        } else {
            Vec::new()
        };
        Ok(DriveResult {
            status,
            progress: TransferProgress {
                file_name: Some(self.file_name.clone()),
                total_bytes: Some(self.total_bytes),
                transferred_bytes: self.transferred_bytes,
            },
            terminal_bytes,
        })
    }
}

impl Drop for DownloadTransfer {
    fn drop(&mut self) {
        if let Some(path) = self.temporary_path.take() {
            let _ = std::fs::remove_file(path);
        }
    }
}

fn protocol_error(error: zmodem2::Error) -> AppError {
    AppError::Remote(format!("Zmodem 协议错误：{error:?}"))
}

fn write_wire(channel: &mut Channel, bytes: &[u8]) -> AppResult<()> {
    let deadline = Instant::now() + Duration::from_secs(20);
    let mut offset = 0;
    while offset < bytes.len() {
        match channel.write(&bytes[offset..]) {
            Ok(0) => return Err(AppError::Remote("Zmodem 写入 SSH 通道返回 0 字节".into())),
            Ok(written) => offset += written,
            Err(error)
                if error.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline =>
            {
                std::thread::sleep(Duration::from_millis(2))
            }
            Err(error) => return Err(AppError::from(error)),
        }
    }
    Ok(())
}

fn safe_file_name(remote_name: &str) -> AppResult<String> {
    let normalized = remote_name.replace('\\', "/");
    let name = normalized.rsplit('/').next().unwrap_or_default();
    if name.is_empty() || name == "." || name == ".." || name.contains('\0') {
        return Err(AppError::Validation("远端提供的 Zmodem 文件名无效".into()));
    }
    Ok(name.chars().take(255).collect())
}

fn available_path(directory: &Path, file_name: &str) -> PathBuf {
    let initial = directory.join(file_name);
    if !initial.exists() {
        return initial;
    }
    let path = Path::new(file_name);
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("download");
    let extension = path.extension().and_then(|value| value.to_str());
    for index in 1..=9999 {
        let candidate = match extension {
            Some(extension) => directory.join(format!("{stem} ({index}).{extension}")),
            None => directory.join(format!("{stem} ({index})")),
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(format!("{stem}-{}", chrono::Utc::now().timestamp_millis()))
}

fn temporary_download_path(directory: &Path) -> PathBuf {
    loop {
        let path = directory.join(format!(".cnshell-zmodem-{}.part", uuid::Uuid::new_v4()));
        if !path.exists() {
            return path;
        }
    }
}

impl HandshakeDetector {
    pub fn feed(&mut self, input: &[u8]) -> DetectionOutput {
        self.pending.extend_from_slice(input);
        let mut terminal_bytes = Vec::new();

        loop {
            let Some(prefix_index) = find_subslice(&self.pending, ZHEX_PREFIX) else {
                let retained = longest_prefix_suffix(&self.pending, ZHEX_PREFIX);
                let release_len = self.pending.len().saturating_sub(retained);
                terminal_bytes.extend(self.pending.drain(..release_len));
                return DetectionOutput {
                    terminal_bytes,
                    detection: None,
                };
            };

            terminal_bytes.extend(self.pending.drain(..prefix_index));
            if self.pending.len() < ZHEX_HEADER_LEN {
                return DetectionOutput {
                    terminal_bytes,
                    detection: None,
                };
            }

            if let Some(direction) = parse_header(&self.pending[..ZHEX_HEADER_LEN]) {
                return DetectionOutput {
                    terminal_bytes,
                    detection: Some(Detection {
                        direction,
                        protocol_bytes: std::mem::take(&mut self.pending),
                    }),
                };
            }

            terminal_bytes.push(self.pending.remove(0));
        }
    }

    #[cfg(test)]
    pub fn flush(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.pending)
    }
}

fn parse_header(header: &[u8]) -> Option<Direction> {
    if header.len() != ZHEX_HEADER_LEN || &header[..ZHEX_PREFIX.len()] != ZHEX_PREFIX {
        return None;
    }
    let mut decoded = [0_u8; 7];
    for (index, pair) in header[4..].chunks_exact(2).enumerate() {
        decoded[index] = (hex_digit(pair[0])? << 4) | hex_digit(pair[1])?;
    }
    if crc16_xmodem(&decoded[..5]) != u16::from_be_bytes([decoded[5], decoded[6]]) {
        return None;
    }
    match decoded[0] {
        0 => Some(Direction::Download),
        1 => Some(Direction::Upload),
        _ => None,
    }
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn crc16_xmodem(data: &[u8]) -> u16 {
    let mut crc = 0_u16;
    for byte in data {
        crc ^= u16::from(*byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn longest_prefix_suffix(data: &[u8], prefix: &[u8]) -> usize {
    (1..prefix.len())
        .rev()
        .find(|length| data.ends_with(&prefix[..*length]))
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ssh2::Session;
    use std::net::TcpStream;

    const ZRQINIT: &[u8] = b"**\x18B00000000000000";
    const ZRINIT: &[u8] = b"**\x18B0100000000aa51";

    #[test]
    fn detects_complete_download_header() {
        let mut detector = HandshakeDetector::default();
        let output = detector.feed(ZRQINIT);
        assert!(output.terminal_bytes.is_empty());
        assert_eq!(output.detection.unwrap().direction, Direction::Download);
    }

    #[test]
    fn detects_header_across_read_boundaries() {
        let mut detector = HandshakeDetector::default();
        assert_eq!(detector.feed(&ZRINIT[..3]).terminal_bytes, b"");
        assert_eq!(detector.feed(&ZRINIT[3..11]).terminal_bytes, b"");
        let output = detector.feed(&ZRINIT[11..]);
        assert_eq!(output.detection.unwrap().direction, Direction::Upload);
    }

    #[test]
    fn releases_text_before_header_and_preserves_protocol_tail() {
        let mut detector = HandshakeDetector::default();
        let mut input = b"ready\r\n".to_vec();
        input.extend_from_slice(ZRQINIT);
        input.extend_from_slice(b"\r\n\x11tail");
        let output = detector.feed(&input);
        assert_eq!(output.terminal_bytes, b"ready\r\n");
        let detection = output.detection.unwrap();
        assert_eq!(detection.direction, Direction::Download);
        assert_eq!(detection.protocol_bytes, &input[b"ready\r\n".len()..]);
    }

    #[test]
    fn rejects_bad_crc_without_swallowing_output() {
        let mut detector = HandshakeDetector::default();
        let mut bad = ZRQINIT.to_vec();
        *bad.last_mut().unwrap() = b'1';
        let output = detector.feed(&bad);
        assert_eq!(output.terminal_bytes, bad);
        assert!(output.detection.is_none());
    }

    #[test]
    fn rejects_similar_text_and_releases_partial_candidate_on_flush() {
        let mut detector = HandshakeDetector::default();
        assert_eq!(
            detector.feed(b"echo **\\x18B0000").terminal_bytes,
            b"echo **\\x18B0000"
        );
        assert_eq!(detector.feed(b"**\x18").terminal_bytes, b"");
        assert_eq!(detector.flush(), b"**\x18");
    }

    #[test]
    fn malformed_long_candidate_stays_bounded() {
        let mut detector = HandshakeDetector::default();
        let mut input = ZHEX_PREFIX.to_vec();
        input.extend(std::iter::repeat_n(b'x', 32 * 1024));
        let output = detector.feed(&input);
        assert_eq!(output.terminal_bytes, input);
        assert!(output.detection.is_none());
        assert!(detector.pending.len() < ZHEX_PREFIX.len());
    }

    #[test]
    fn download_name_cannot_escape_authorized_directory() {
        assert_eq!(safe_file_name("../../etc/passwd").unwrap(), "passwd");
        assert_eq!(
            safe_file_name("C:\\temp\\report.txt").unwrap(),
            "report.txt"
        );
        assert!(safe_file_name("..").is_err());
        assert!(safe_file_name("").is_err());
    }

    #[test]
    fn existing_download_gets_a_non_destructive_name() {
        let directory = tempfile::tempdir().unwrap();
        std::fs::write(directory.path().join("report.txt"), b"existing").unwrap();
        assert_eq!(
            available_path(directory.path(), "report.txt"),
            directory.path().join("report (1).txt")
        );
    }

    #[test]
    fn finishing_transfer_swallows_oo_and_releases_prompt() {
        let mut split = FinishingTransfer::new(vec![b'O']);
        assert_eq!(split.feed(&[]), None);
        assert_eq!(split.feed(b"Oprompt"), Some(b"prompt".to_vec()));

        let mut combined = FinishingTransfer::new(b"OO\r\n$ ".to_vec());
        assert_eq!(combined.feed(&[]), Some(b"\r\n$ ".to_vec()));
    }

    fn live_channel(command: &str) -> Option<Channel> {
        let host = std::env::var("CNSHELL_ZMODEM_TEST_HOST").ok()?;
        let port = std::env::var("CNSHELL_ZMODEM_TEST_PORT")
            .ok()
            .and_then(|value| value.parse::<u16>().ok())
            .unwrap_or(22);
        let username = std::env::var("CNSHELL_ZMODEM_TEST_USER").ok()?;
        let password = std::env::var("CNSHELL_ZMODEM_TEST_PASSWORD").ok()?;
        let tcp = TcpStream::connect_timeout(
            &format!("{host}:{port}").parse().ok()?,
            Duration::from_secs(10),
        )
        .ok()?;
        let mut session = Session::new().ok()?;
        session.set_tcp_stream(tcp);
        session.handshake().ok()?;
        session.userauth_password(&username, &password).ok()?;
        let mut channel = session.channel_session().ok()?;
        channel
            .request_pty("xterm-256color", None, Some((80, 24, 0, 0)))
            .ok()?;
        channel.exec(command).ok()?;
        session.set_blocking(false);
        Some(channel)
    }

    fn run_live_transfer(
        command: &str,
        expected_direction: Direction,
        selected_paths: &[PathBuf],
    ) -> Option<TransferProgress> {
        let mut channel = live_channel(command)?;
        let mut detector = HandshakeDetector::default();
        let mut active: Option<ActiveTransfer> = None;
        let deadline = Instant::now() + Duration::from_secs(20);
        let mut buffer = [0_u8; 32 * 1024];
        loop {
            let read = match channel.read(&mut buffer) {
                Ok(size) => size,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
                Err(error) => panic!("live Zmodem SSH read failed: {error}"),
            };
            if let Some(transfer) = active.as_mut() {
                if read > 0 {
                    transfer.append_wire(&buffer[..read]).unwrap();
                }
                let result = transfer.drive(&mut channel).unwrap();
                if result.status == Some("completed") {
                    while !channel.eof() {
                        match channel.read(&mut buffer) {
                            Ok(_) => {}
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {}
                            Err(error) => panic!("live Zmodem completion read failed: {error}"),
                        }
                        assert!(
                            Instant::now() < deadline,
                            "live Zmodem remote validation timed out"
                        );
                        std::thread::sleep(Duration::from_millis(2));
                    }
                    assert_eq!(channel.exit_status().unwrap(), 0);
                    return Some(result.progress);
                }
                assert_ne!(result.status, Some("cancelled"));
            } else if read > 0 {
                let output = detector.feed(&buffer[..read]);
                if let Some(detection) = output.detection {
                    assert_eq!(detection.direction, expected_direction);
                    active = Some(match expected_direction {
                        Direction::Download => ActiveTransfer::download(
                            selected_paths[0].clone(),
                            detection.protocol_bytes,
                            "live-download".into(),
                        )
                        .unwrap(),
                        Direction::Upload => ActiveTransfer::upload(
                            selected_paths.to_vec(),
                            detection.protocol_bytes,
                            "live-upload".into(),
                        )
                        .unwrap(),
                    });
                }
            }
            assert!(Instant::now() < deadline, "live Zmodem transfer timed out");
            std::thread::sleep(Duration::from_millis(2));
        }
    }

    #[test]
    fn live_zmodem_lrzsz_download_and_upload() {
        if std::env::var("CNSHELL_ZMODEM_TEST_HOST").is_err() {
            return;
        }
        let download_directory = tempfile::tempdir().unwrap();
        let download = run_live_transfer(
            "tmp=$(mktemp -d); printf cnshell-zmodem-download > \"$tmp/cnshell-zmodem-download.txt\"; sz \"$tmp/cnshell-zmodem-download.txt\"; status=$?; rm -rf \"$tmp\"; exit $status",
            Direction::Download,
            &[download_directory.path().to_path_buf()],
        )
        .unwrap();
        assert_eq!(download.transferred_bytes, 23);
        assert_eq!(
            std::fs::read(
                download_directory
                    .path()
                    .join("cnshell-zmodem-download.txt")
            )
            .unwrap(),
            b"cnshell-zmodem-download"
        );

        let upload_directory = tempfile::tempdir().unwrap();
        let upload_path = upload_directory.path().join("cnshell-zmodem-upload.txt");
        let empty_path = upload_directory.path().join("CNshell 空 文件.txt");
        std::fs::write(&upload_path, b"cnshell-zmodem-upload").unwrap();
        std::fs::write(&empty_path, b"").unwrap();
        let upload = run_live_transfer(
            "tmp=$(mktemp -d); cd \"$tmp\"; rz -y; status=$?; test \"$(cat cnshell-zmodem-upload.txt 2>/dev/null)\" = cnshell-zmodem-upload || status=1; test -f 'CNshell 空 文件.txt' && test ! -s 'CNshell 空 文件.txt' || status=1; cd /; rm -rf \"$tmp\"; exit $status",
            Direction::Upload,
            &[upload_path, empty_path],
        )
        .unwrap();
        assert_eq!(upload.file_name.as_deref(), Some("CNshell 空 文件.txt"));
        assert_eq!(upload.transferred_bytes, 0);
    }
}
