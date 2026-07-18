use crate::{
    error::{AppError, AppResult},
    models::SessionLogStatus,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use parking_lot::Mutex;
use serde_json::json;
use std::{
    collections::HashMap,
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        mpsc::{self, SyncSender, TrySendError},
    },
    time::{Duration, SystemTime},
};

const MAX_FILE_BYTES: u64 = 100 * 1024 * 1024;

struct LogWriter {
    writer: BufWriter<File>,
    format: String,
    line_timestamps: bool,
    pending_line: Vec<u8>,
    bytes_written: u64,
}

enum LogMessage {
    Output(Vec<u8>),
    Flush(mpsc::Sender<AppResult<()>>),
    Stop(mpsc::Sender<AppResult<()>>),
}

struct LogState {
    status: Arc<Mutex<SessionLogStatus>>,
    sender: Option<SyncSender<LogMessage>>,
}

#[derive(Clone)]
pub struct SessionLogManager {
    directory: PathBuf,
    states: Arc<Mutex<HashMap<String, LogState>>>,
}

impl SessionLogManager {
    pub fn new(directory: PathBuf) -> AppResult<Self> {
        fs::create_dir_all(&directory)?;
        Ok(Self {
            directory,
            states: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    pub fn start(
        &self,
        session_id: &str,
        connection_name: &str,
        format: &str,
        line_timestamps: bool,
        retention_days: u64,
        max_total_bytes: u64,
    ) -> AppResult<SessionLogStatus> {
        if self
            .states
            .lock()
            .get(session_id)
            .is_some_and(|state| state.status.lock().active)
        {
            return Err(AppError::Validation("该会话已经在记录日志".into()));
        }
        if !matches!(format, "text" | "jsonl") {
            return Err(AppError::Validation("日志格式必须是 text 或 jsonl".into()));
        }
        if !(1..=3650).contains(&retention_days) {
            return Err(AppError::Validation(
                "日志保留天数必须在 1 到 3650 之间".into(),
            ));
        }
        if !(10 * 1024 * 1024..=20 * 1024 * 1024 * 1024).contains(&max_total_bytes) {
            return Err(AppError::Validation(
                "日志总容量上限必须在 10 MB 到 20 GB 之间".into(),
            ));
        }
        self.cleanup(retention_days, max_total_bytes)?;
        let started_at = chrono::Utc::now();
        let safe_name = safe_file_name(connection_name);
        let extension = if format == "jsonl" { "jsonl" } else { "log" };
        let path = self.directory.join(format!(
            "{}-{}-{}.{}",
            started_at.format("%Y%m%d-%H%M%S-%9f"),
            safe_name,
            &session_id[..session_id.len().min(8)],
            extension
        ));
        let file = File::options().create_new(true).write(true).open(&path)?;
        let mut writer = LogWriter {
            writer: BufWriter::new(file),
            format: format.into(),
            line_timestamps,
            pending_line: Vec::new(),
            bytes_written: 0,
        };
        writer.write_event("start", Some(connection_name.as_bytes()))?;
        let status = SessionLogStatus {
            session_id: session_id.into(),
            active: true,
            path: Some(path.to_string_lossy().into_owned()),
            format: Some(format.into()),
            line_timestamps,
            started_at: Some(started_at.to_rfc3339()),
            bytes_written: writer.bytes_written,
            error: None,
        };
        let status = Arc::new(Mutex::new(status));
        let (sender, receiver) = mpsc::sync_channel(64);
        let worker_status = status.clone();
        std::thread::Builder::new()
            .name(format!(
                "cnshell-log-{}",
                &session_id[..session_id.len().min(8)]
            ))
            .spawn(move || log_worker(writer, receiver, worker_status))
            .map_err(AppError::from)?;
        let returned = status.lock().clone();
        self.states.lock().insert(
            session_id.into(),
            LogState {
                status,
                sender: Some(sender),
            },
        );
        Ok(returned)
    }

    pub fn write_output(&self, session_id: &str, data: &[u8]) {
        let target = self.states.lock().get(session_id).and_then(|state| {
            state
                .sender
                .as_ref()
                .map(|sender| (sender.clone(), state.status.clone()))
        });
        let Some((sender, status)) = target else {
            return;
        };
        match sender.try_send(LogMessage::Output(data.to_vec())) {
            Ok(()) => {}
            Err(TrySendError::Full(_)) => {
                mark_failed(
                    &status,
                    "日志写入速度跟不上终端输出，已自动停止以保护 SSH 会话",
                );
                if let Some(state) = self.states.lock().get_mut(session_id) {
                    state.sender = None;
                }
            }
            Err(TrySendError::Disconnected(_)) => mark_failed(&status, "日志写入线程已停止"),
        }
    }

    pub fn stop(&self, session_id: &str) -> AppResult<SessionLogStatus> {
        let (status, sender) = {
            let mut states = self.states.lock();
            let state = states
                .get_mut(session_id)
                .ok_or_else(|| AppError::NotFound("该会话没有日志记录".into()))?;
            (state.status.clone(), state.sender.take())
        };
        let Some(sender) = sender else {
            status.lock().active = false;
            return Ok(status.lock().clone());
        };
        let (reply, result) = mpsc::channel();
        match sender.try_send(LogMessage::Stop(reply)) {
            Ok(()) => result
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| AppError::Storage("停止日志超时".into()))??,
            Err(TrySendError::Full(_)) => mark_failed(&status, "日志队列繁忙，已请求停止"),
            Err(TrySendError::Disconnected(_)) => mark_failed(&status, "日志写入线程已停止"),
        }
        status.lock().active = false;
        Ok(status.lock().clone())
    }

    pub fn status(&self, session_id: &str) -> SessionLogStatus {
        self.states
            .lock()
            .get(session_id)
            .map(|state| state.status.lock().clone())
            .unwrap_or_else(|| SessionLogStatus {
                session_id: session_id.into(),
                active: false,
                path: None,
                format: None,
                line_timestamps: false,
                started_at: None,
                bytes_written: 0,
                error: None,
            })
    }

    pub fn export(&self, session_id: &str, destination: &Path) -> AppResult<()> {
        let (status, sender) = {
            let states = self.states.lock();
            let state = states
                .get(session_id)
                .ok_or_else(|| AppError::NotFound("该会话没有可导出的日志".into()))?;
            (state.status.clone(), state.sender.clone())
        };
        if let Some(sender) = sender {
            let (reply, result) = mpsc::channel();
            sender
                .try_send(LogMessage::Flush(reply))
                .map_err(|_| AppError::Storage("日志队列繁忙，请稍后重试导出".into()))?;
            result
                .recv_timeout(Duration::from_secs(5))
                .map_err(|_| AppError::Storage("刷新日志超时".into()))??;
        }
        let source = status
            .lock()
            .path
            .clone()
            .ok_or_else(|| AppError::NotFound("日志文件不存在".into()))?;
        if Path::new(&source) == destination {
            return Ok(());
        }
        fs::copy(&source, destination)?;
        Ok(())
    }

    fn cleanup(&self, retention_days: u64, max_total_bytes: u64) -> AppResult<()> {
        let now = SystemTime::now();
        let retention = Duration::from_secs(retention_days.saturating_mul(86_400));
        let active_paths: Vec<PathBuf> = self
            .states
            .lock()
            .values()
            .filter_map(|state| {
                let status = state.status.lock();
                if status.active {
                    status.path.as_ref().map(PathBuf::from)
                } else {
                    None
                }
            })
            .collect();
        let mut files = Vec::new();
        for entry in fs::read_dir(&self.directory)? {
            let entry = entry?;
            let path = entry.path();
            if !path.is_file() || active_paths.contains(&path) {
                continue;
            }
            let metadata = entry.metadata()?;
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            if now.duration_since(modified).unwrap_or_default() > retention {
                let _ = fs::remove_file(path);
                continue;
            }
            files.push((path, modified, metadata.len()));
        }
        files.sort_by_key(|(_, modified, _)| *modified);
        let mut total = files.iter().map(|(_, _, size)| *size).sum::<u64>();
        for (path, _, size) in files {
            if total <= max_total_bytes {
                break;
            }
            if fs::remove_file(path).is_ok() {
                total = total.saturating_sub(size);
            }
        }
        Ok(())
    }
}

impl LogWriter {
    fn write_event(&mut self, event: &str, data: Option<&[u8]>) -> AppResult<()> {
        if self.format == "jsonl" {
            let value = json!({"timestamp":chrono::Utc::now().to_rfc3339(),"event":event,"dataBase64":data.map(|value|STANDARD.encode(value))});
            let serialized = serde_json::to_string(&value)
                .map_err(|error| AppError::Internal(error.to_string()))?;
            self.write_bytes(format!("{serialized}\n").as_bytes())?;
        } else if event == "start" {
            let name = String::from_utf8_lossy(data.unwrap_or_default());
            self.write_bytes(
                format!(
                    "# CNshell 会话日志\n# 连接: {name}\n# 开始: {}\n\n",
                    chrono::Utc::now().to_rfc3339()
                )
                .as_bytes(),
            )?;
        } else if event == "end" {
            self.flush_pending_line()?;
            self.write_bytes(
                format!("\n# 结束: {}\n", chrono::Utc::now().to_rfc3339()).as_bytes(),
            )?;
        } else if let Some(data) = data {
            if self.line_timestamps {
                self.pending_line.extend_from_slice(data);
                while let Some(end) = self.pending_line.iter().position(|byte| *byte == b'\n') {
                    let line = self.pending_line.drain(..=end).collect::<Vec<_>>();
                    self.write_timestamped_line(&line)?;
                }
            } else {
                self.write_bytes(data)?;
            }
        }
        self.writer.flush()?;
        Ok(())
    }

    fn write_bytes(&mut self, data: &[u8]) -> AppResult<()> {
        self.writer.write_all(data)?;
        self.bytes_written = self.bytes_written.saturating_add(data.len() as u64);
        Ok(())
    }

    fn write_timestamped_line(&mut self, data: &[u8]) -> AppResult<()> {
        self.write_bytes(
            format!(
                "[{}] ",
                chrono::Local::now().format("%Y-%m-%d %H:%M:%S%.3f")
            )
            .as_bytes(),
        )?;
        self.write_bytes(String::from_utf8_lossy(data).as_bytes())
    }

    fn flush_pending_line(&mut self) -> AppResult<()> {
        if self.pending_line.is_empty() {
            return Ok(());
        }
        let pending = std::mem::take(&mut self.pending_line);
        self.write_timestamped_line(&pending)
    }
}

fn log_worker(
    mut writer: LogWriter,
    receiver: mpsc::Receiver<LogMessage>,
    status: Arc<Mutex<SessionLogStatus>>,
) {
    while let Ok(message) = receiver.recv() {
        let result = match message {
            LogMessage::Output(data) => {
                if writer.bytes_written.saturating_add(data.len() as u64) > MAX_FILE_BYTES {
                    Err(AppError::Storage(
                        "会话日志达到 100 MB 单文件上限，已自动停止".into(),
                    ))
                } else {
                    writer.write_event("output", Some(&data))
                }
            }
            LogMessage::Flush(reply) => {
                let result = writer.writer.flush().map_err(AppError::from);
                let failed = result.as_ref().err().map(ToString::to_string);
                let _ = reply.send(result);
                if let Some(error) = failed {
                    mark_failed(&status, &error);
                    return;
                }
                continue;
            }
            LogMessage::Stop(reply) => {
                let result = writer
                    .write_event("end", None)
                    .and_then(|_| writer.writer.flush().map_err(AppError::from));
                let failed = result.as_ref().err().map(ToString::to_string);
                let _ = reply.send(result);
                if let Some(error) = failed {
                    mark_failed(&status, &error);
                } else {
                    let mut current = status.lock();
                    current.active = false;
                    current.bytes_written = writer.bytes_written;
                }
                return;
            }
        };
        match result {
            Ok(()) => status.lock().bytes_written = writer.bytes_written,
            Err(error) => {
                mark_failed(&status, &error.to_string());
                return;
            }
        }
    }
    let _ = writer.flush_pending_line();
    let _ = writer.writer.flush();
    status.lock().active = false;
}

fn mark_failed(status: &Arc<Mutex<SessionLogStatus>>, message: &str) {
    let mut current = status.lock();
    current.active = false;
    current.error = Some(message.into());
}

fn safe_file_name(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .map(|character| {
            if character.is_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(48)
        .collect();
    if cleaned.is_empty() {
        "session".into()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn text_log_survives_chunked_lines_and_has_timestamps() {
        let temp = tempdir().unwrap();
        let manager = SessionLogManager::new(temp.path().into()).unwrap();
        let status = manager
            .start(
                "session-123",
                "测试 / 主机",
                "text",
                true,
                30,
                10 * 1024 * 1024,
            )
            .unwrap();
        manager.write_output("session-123", b"hello ");
        manager.write_output("session-123", b"world\nnext\n");
        let chinese = "中文跨块\n".as_bytes();
        manager.write_output("session-123", &chinese[..2]);
        manager.write_output("session-123", &chinese[2..]);
        manager.stop("session-123").unwrap();
        let text = fs::read_to_string(status.path.unwrap()).unwrap();
        assert!(text.contains("hello world\n"));
        assert_eq!(text.matches("] hello").count(), 1);
        assert!(text.contains("] next\n"));
        assert!(text.contains("] 中文跨块\n"));
        assert!(text.contains("# 结束:"));
    }

    #[test]
    fn jsonl_log_is_structured_and_exportable() {
        let temp = tempdir().unwrap();
        let manager = SessionLogManager::new(temp.path().join("logs")).unwrap();
        let status = manager
            .start("session-456", "host", "jsonl", false, 30, 10 * 1024 * 1024)
            .unwrap();
        manager.write_output("session-456", b"\0binary");
        let exported = temp.path().join("export.jsonl");
        manager.export("session-456", &exported).unwrap();
        let lines = fs::read_to_string(exported).unwrap();
        assert!(
            lines
                .lines()
                .all(|line| serde_json::from_str::<serde_json::Value>(line).is_ok())
        );
        assert!(lines.contains(&STANDARD.encode(b"\0binary")));
        assert!(status.active);
    }

    #[test]
    fn one_megabyte_continuous_output_is_not_lost() {
        let temp = tempdir().unwrap();
        let manager = SessionLogManager::new(temp.path().into()).unwrap();
        let status = manager
            .start("session-large", "host", "text", false, 30, 10 * 1024 * 1024)
            .unwrap();
        let chunk = vec![b'x'; 32 * 1024];
        for _ in 0..32 {
            manager.write_output("session-large", &chunk);
        }
        manager.stop("session-large").unwrap();
        let bytes = fs::read(status.path.unwrap()).unwrap();
        assert_eq!(
            bytes.iter().filter(|byte| **byte == b'x').count(),
            1024 * 1024
        );
    }
}
