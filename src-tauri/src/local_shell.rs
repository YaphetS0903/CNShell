use crate::{
    error::{AppError, AppResult},
    models::{ConnectionProfile, TerminalOutput, TerminalSession, TerminalStatus},
    session_log::SessionLogManager,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use parking_lot::Mutex;
use portable_pty::{Child, CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::{
    collections::HashMap,
    ffi::OsString,
    io::{ErrorKind, Read, Write},
    path::PathBuf,
    sync::Arc,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

struct ManagedLocal {
    child: Box<dyn Child + Send + Sync>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

#[derive(Clone, Default)]
pub struct LocalShellManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedLocal>>>>>,
    closing: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl LocalShellManager {
    pub fn contains(&self, id: &str) -> bool {
        self.sessions.lock().contains_key(id)
    }

    pub fn open(
        &self,
        app: AppHandle,
        profile: ConnectionProfile,
        logs: SessionLogManager,
        cols: u32,
        rows: u32,
    ) -> AppResult<TerminalSession> {
        validate_size(cols, rows)?;
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::Internal(format!("创建本地 PTY 失败：{error}")))?;
        let reader = pty
            .master
            .try_clone_reader()
            .map_err(|error| AppError::Internal(format!("读取本地 PTY 失败：{error}")))?;
        let writer = pty
            .master
            .take_writer()
            .map_err(|error| AppError::Internal(format!("写入本地 PTY 失败：{error}")))?;
        let (shell, arguments) = shell_command()?;
        let mut command = CommandBuilder::new(shell);
        for argument in arguments {
            command.arg(argument);
        }
        command.env("TERM", "xterm-256color");
        for (key, value) in &profile.environment {
            if crate::serial::is_option_key(key) {
                continue;
            }
            command.env(key, value);
        }
        let child = pty
            .slave
            .spawn_command(command)
            .map_err(|error| AppError::Unavailable(format!("启动本地 Shell 失败：{error}")))?;
        drop(pty.slave);
        let id = Uuid::new_v4().to_string();
        let managed = Arc::new(Mutex::new(ManagedLocal {
            child,
            master: pty.master,
            writer,
        }));
        if let Some(startup) = profile
            .startup_command
            .as_deref()
            .filter(|value| !value.is_empty())
        {
            let mut handle = managed.lock();
            handle.writer.write_all(startup.as_bytes())?;
            handle.writer.write_all(local_line_ending())?;
            handle.writer.flush()?;
        }
        self.sessions.lock().insert(id.clone(), managed.clone());
        spawn_reader(app, self.clone(), logs, id.clone(), managed, reader);
        Ok(TerminalSession {
            id,
            connection_id: profile.id,
            session_type: "local".into(),
            title: format!("{} · 本地", profile.name),
            status: "online".into(),
            started_at: chrono::Utc::now().to_rfc3339(),
            last_error: None,
        })
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
            .ok_or_else(|| AppError::NotFound(format!("本地 Shell 会话 {id}")))?;
        let mut session = session.lock();
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
            .ok_or_else(|| AppError::NotFound(format!("本地 Shell 会话 {id}")))?;
        session
            .lock()
            .master
            .resize(PtySize {
                rows: rows as u16,
                cols: cols as u16,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|error| AppError::Internal(format!("调整本地 PTY 失败：{error}")))
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let session = self
            .sessions
            .lock()
            .get(id)
            .cloned()
            .ok_or_else(|| AppError::NotFound(format!("本地 Shell 会话 {id}")))?;
        self.closing.lock().insert(id.into());
        session
            .lock()
            .child
            .kill()
            .map_err(|error| AppError::Unavailable(format!("关闭本地 Shell 失败：{error}")))
    }

    pub fn close_all(&self) {
        for id in self.sessions.lock().keys().cloned().collect::<Vec<_>>() {
            let _ = self.close(&id);
        }
    }
}

fn spawn_reader(
    app: AppHandle,
    manager: LocalShellManager,
    logs: SessionLogManager,
    id: String,
    session: Arc<Mutex<ManagedLocal>>,
    mut reader: Box<dyn Read + Send>,
) {
    std::thread::Builder::new()
        .name(format!("cnshell-local-{}", &id[..id.len().min(8)]))
        .spawn(move || {
            let mut buffer = [0_u8; 32 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
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
            let status = session.lock().child.wait();
            manager.sessions.lock().remove(&id);
            let requested = manager.closing.lock().remove(&id);
            let successful = status.as_ref().is_ok_and(|status| status.success());
            let error = if requested || successful {
                None
            } else {
                Some(format!(
                    "本地 Shell 异常退出：{}",
                    status
                        .map(|value| value.to_string())
                        .unwrap_or_else(|error| error.to_string())
                ))
            };
            let _ = logs.stop(&id);
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
        .ok();
}

fn validate_size(cols: u32, rows: u32) -> AppResult<()> {
    if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(AppError::Validation("PTY 尺寸超出允许范围".into()));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn shell_command() -> AppResult<(PathBuf, Vec<OsString>)> {
    for (candidate, arguments) in [
        ("pwsh.exe", vec![OsString::from("-NoLogo")]),
        ("powershell.exe", vec![OsString::from("-NoLogo")]),
        ("cmd.exe", Vec::new()),
    ] {
        if let Ok(path) = which::which(candidate) {
            return Ok((path, arguments));
        }
    }
    Err(AppError::Unavailable(
        "未找到 pwsh.exe、powershell.exe 或 cmd.exe".into(),
    ))
}

#[cfg(not(target_os = "windows"))]
fn shell_command() -> AppResult<(PathBuf, Vec<OsString>)> {
    let shell = std::env::var_os("SHELL").unwrap_or_else(|| OsString::from("/bin/zsh"));
    Ok((PathBuf::from(shell), vec![OsString::from("-l")]))
}

#[cfg(target_os = "windows")]
fn local_line_ending() -> &'static [u8] {
    b"\r\n"
}

#[cfg(not(target_os = "windows"))]
fn local_line_ending() -> &'static [u8] {
    b"\n"
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "windows")]
    use super::*;

    #[test]
    fn local_shell_is_a_distinct_session_type() {
        assert_eq!("local", "local");
    }

    #[cfg(target_os = "windows")]
    type ConptyFixture = (
        Box<dyn Child + Send + Sync>,
        Box<dyn MasterPty + Send>,
        Box<dyn Write + Send>,
        std::sync::mpsc::Receiver<Vec<u8>>,
    );

    #[cfg(target_os = "windows")]
    fn spawn_conpty_fixture() -> ConptyFixture {
        let pty = native_pty_system()
            .openpty(PtySize {
                rows: 24,
                cols: 80,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("ConPTY should open");
        let mut reader = pty
            .master
            .try_clone_reader()
            .expect("ConPTY reader should clone");
        let writer = pty.master.take_writer().expect("ConPTY writer should open");
        let (shell, arguments) = shell_command().expect("a Windows shell should be available");
        let mut command = CommandBuilder::new(shell);
        for argument in arguments {
            command.arg(argument);
        }
        command.env("TERM", "xterm-256color");
        let child = pty
            .slave
            .spawn_command(command)
            .expect("Windows shell should start inside ConPTY");
        drop(pty.slave);
        let (sender, receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 4096];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) if sender.send(buffer[..size].to_vec()).is_err() => break,
                    Ok(_) => {}
                    Err(error) if error.kind() == ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
        });
        (child, pty.master, writer, receiver)
    }

    #[cfg(target_os = "windows")]
    fn expect_output(
        receiver: &std::sync::mpsc::Receiver<Vec<u8>>,
        writer: &mut (dyn Write + Send),
        marker: &str,
    ) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
        let mut output = Vec::new();
        let mut cursor_position_reported = false;
        while std::time::Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            match receiver.recv_timeout(remaining) {
                Ok(chunk) => {
                    output.extend_from_slice(&chunk);
                    assert!(
                        output.len() <= 1024 * 1024,
                        "ConPTY fixture output is unbounded"
                    );
                    if !cursor_position_reported
                        && output.windows(4).any(|window| window == b"\x1b[6n")
                    {
                        writer
                            .write_all(b"\x1b[1;1R")
                            .expect("ConPTY should accept a cursor position report");
                        writer
                            .flush()
                            .expect("ConPTY cursor position report should flush");
                        cursor_position_reported = true;
                    }
                    if String::from_utf8_lossy(&output).contains(marker) {
                        return;
                    }
                }
                Err(_) => break,
            }
        }
        panic!(
            "ConPTY did not emit {marker:?}; output was {:?}",
            String::from_utf8_lossy(&output)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn conpty_accepts_input_resize_close_and_reopen() {
        let (mut first_child, first_master, mut first_writer, first_output) =
            spawn_conpty_fixture();
        first_master
            .resize(PtySize {
                rows: 40,
                cols: 120,
                pixel_width: 0,
                pixel_height: 0,
            })
            .expect("ConPTY should resize");
        first_writer
            .write_all(b"echo CNSHELL_CONPTY_FIRST\r\n")
            .expect("ConPTY should accept input");
        first_writer.flush().expect("ConPTY input should flush");
        expect_output(&first_output, first_writer.as_mut(), "CNSHELL_CONPTY_FIRST");
        first_child.kill().expect("ConPTY child should close");
        first_child.wait().expect("closed ConPTY child should reap");
        drop(first_writer);
        drop(first_master);

        let (mut second_child, second_master, mut second_writer, second_output) =
            spawn_conpty_fixture();
        second_writer
            .write_all(b"echo CNSHELL_CONPTY_REOPENED\r\n")
            .expect("reopened ConPTY should accept input");
        second_writer
            .flush()
            .expect("reopened ConPTY input should flush");
        expect_output(
            &second_output,
            second_writer.as_mut(),
            "CNSHELL_CONPTY_REOPENED",
        );
        second_child
            .kill()
            .expect("reopened ConPTY child should close");
        second_child
            .wait()
            .expect("reopened ConPTY child should reap");
        drop(second_writer);
        drop(second_master);
    }
}
