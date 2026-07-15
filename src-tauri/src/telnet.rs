use crate::{
    error::{AppError, AppResult},
    models::{ConnectionProfile, TerminalOutput, TerminalSession, TerminalStatus},
    session_log::SessionLogManager,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    sync::Arc,
    time::Duration,
};
use tauri::{AppHandle, Emitter};
use uuid::Uuid;

struct ManagedTelnet {
    stream: TcpStream,
}

#[derive(Clone, Default)]
pub struct TelnetManager {
    sessions: Arc<Mutex<HashMap<String, Arc<Mutex<ManagedTelnet>>>>>,
    closing: Arc<Mutex<std::collections::HashSet<String>>>,
}

impl TelnetManager {
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
        let stream = connect_stream(&profile.host, profile.port)?;
        let reader = stream.try_clone().map_err(AppError::from)?;
        let id = Uuid::new_v4().to_string();
        let managed = Arc::new(Mutex::new(ManagedTelnet { stream }));
        self.sessions.lock().insert(id.clone(), managed);
        spawn_reader(app, self.clone(), logs, id.clone(), reader);
        Ok(TerminalSession {
            id,
            connection_id: profile.id,
            session_type: "telnet".into(),
            title: format!("{} · Telnet（未加密）", profile.name),
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
            .ok_or_else(|| AppError::NotFound(format!("Telnet 会话 {id}")))?;
        session
            .lock()
            .stream
            .write_all(data.as_bytes())
            .map_err(AppError::from)
    }

    pub fn resize(&self, id: &str, cols: u32, rows: u32) -> AppResult<()> {
        validate_size(cols, rows)?;
        if !self.contains(id) {
            return Err(AppError::NotFound(format!("Telnet 会话 {id}")));
        }
        Ok(())
    }

    pub fn close(&self, id: &str) -> AppResult<()> {
        let session = self
            .sessions
            .lock()
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("Telnet 会话 {id}")))?;
        self.closing.lock().insert(id.into());
        session
            .lock()
            .stream
            .shutdown(Shutdown::Both)
            .map_err(AppError::from)
    }

    pub fn close_all(&self) {
        for id in self.sessions.lock().keys().cloned().collect::<Vec<_>>() {
            let _ = self.close(&id);
        }
    }
}

fn spawn_reader(
    app: AppHandle,
    manager: TelnetManager,
    logs: SessionLogManager,
    id: String,
    mut reader: TcpStream,
) {
    std::thread::Builder::new()
        .name(format!("cnshell-telnet-{}", &id[..id.len().min(8)]))
        .spawn(move || {
            let mut buffer = [0_u8; 32 * 1024];
            let mut telnet_state = TelnetParser::default();
            let mut failure = None;
            loop {
                if manager.closing.lock().contains(&id) {
                    break;
                }
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(size) => {
                        let clean = telnet_state.filter(&buffer[..size]);
                        if !telnet_state.responses.is_empty() {
                            let responses = std::mem::take(&mut telnet_state.responses);
                            if let Err(error) = reader.write_all(&responses) {
                                failure = Some(error.to_string());
                                break;
                            }
                        }
                        if !clean.is_empty() {
                            logs.write_output(&id, &clean);
                            let _ = app.emit(
                                "terminal-output",
                                TerminalOutput {
                                    session_id: id.clone(),
                                    data_base64: STANDARD.encode(clean),
                                },
                            );
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::TimedOut => continue,
                    Err(error) => {
                        failure = Some(error.to_string());
                        break;
                    }
                }
            }
            let requested = manager.closing.lock().remove(&id);
            manager.sessions.lock().remove(&id);
            let _ = logs.stop(&id);
            let _ = app.emit(
                "terminal-status",
                TerminalStatus {
                    session_id: id,
                    status: if requested { "closed" } else { "failed" }.into(),
                    last_error: if requested {
                        None
                    } else {
                        failure.or_else(|| Some("Telnet 服务器已关闭连接".into()))
                    },
                    attempt: None,
                },
            );
        })
        .ok();
}

fn format_host_port(host: &str, port: i64) -> AppResult<String> {
    if !(1..=65535).contains(&port) {
        return Err(AppError::Validation(
            "Telnet 端口必须在 1 到 65535 之间".into(),
        ));
    }
    if host.contains(':') && !host.starts_with('[') {
        Ok(format!("[{host}]:{port}"))
    } else {
        Ok(format!("{host}:{port}"))
    }
}

fn connect_stream(host: &str, port: i64) -> AppResult<TcpStream> {
    let address = format_host_port(host, port)?;
    let socket = address
        .to_socket_addrs()
        .map_err(|error| AppError::Unavailable(format!("解析 Telnet 主机失败：{error}")))?
        .next()
        .ok_or_else(|| AppError::Unavailable("Telnet 主机没有可用地址".into()))?;
    let stream = TcpStream::connect_timeout(&socket, Duration::from_secs(10))
        .map_err(|error| AppError::Unavailable(format!("连接 Telnet 服务器失败：{error}")))?;
    stream
        .set_read_timeout(Some(Duration::from_millis(200)))
        .map_err(AppError::from)?;
    stream
        .set_write_timeout(Some(Duration::from_secs(10)))
        .map_err(AppError::from)?;
    Ok(stream)
}

fn validate_size(cols: u32, rows: u32) -> AppResult<()> {
    if !(1..=1000).contains(&cols) || !(1..=500).contains(&rows) {
        return Err(AppError::Validation("终端尺寸超出允许范围".into()));
    }
    Ok(())
}

#[derive(Default)]
struct TelnetParser {
    state: ParserState,
    responses: Vec<u8>,
}

#[derive(Default, Clone, Copy)]
enum ParserState {
    #[default]
    Data,
    Iac,
    Negotiation(u8),
    Subnegotiation,
    SubnegotiationIac,
}

impl TelnetParser {
    fn filter(&mut self, input: &[u8]) -> Vec<u8> {
        let mut output = Vec::with_capacity(input.len());
        for &byte in input {
            match self.state {
                ParserState::Data if byte == 255 => self.state = ParserState::Iac,
                ParserState::Data => output.push(byte),
                ParserState::Iac => match byte {
                    255 => {
                        output.push(255);
                        self.state = ParserState::Data;
                    }
                    250 => self.state = ParserState::Subnegotiation,
                    251..=254 => self.state = ParserState::Negotiation(byte),
                    _ => self.state = ParserState::Data,
                },
                ParserState::Negotiation(command) => {
                    if command == 251 {
                        self.responses.extend_from_slice(&[255, 252, byte]);
                    } else if command == 253 {
                        self.responses.extend_from_slice(&[255, 254, byte]);
                    }
                    self.state = ParserState::Data;
                }
                ParserState::Subnegotiation if byte == 255 => {
                    self.state = ParserState::SubnegotiationIac
                }
                ParserState::Subnegotiation => {}
                ParserState::SubnegotiationIac => {
                    self.state = if byte == 240 {
                        ParserState::Data
                    } else {
                        ParserState::Subnegotiation
                    }
                }
            }
        }
        output
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    #[test]
    fn telnet_parser_removes_negotiation_and_preserves_escaped_iac() {
        let mut parser = TelnetParser::default();
        assert_eq!(parser.filter(&[255, 251, 1, b'o', b'k']), b"ok");
        assert_eq!(parser.responses, vec![255, 252, 1]);
        assert_eq!(parser.filter(&[255, 255]), vec![255]);
        assert_eq!(parser.filter(&[255, 250, 24, 1, 255]), Vec::<u8>::new());
        assert_eq!(parser.filter(&[240, b'x']), b"x");
    }

    #[test]
    fn ipv6_host_is_bracketed_for_socket_resolution() {
        assert_eq!(format_host_port("::1", 23).unwrap(), "[::1]:23");
        assert_eq!(
            format_host_port("example.test", 23).unwrap(),
            "example.test:23"
        );
    }

    #[test]
    fn loopback_telnet_transport_connects_and_exchanges_bytes() {
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut socket, _) = listener.accept().unwrap();
            socket.write_all(b"ready\r\n").unwrap();
            let mut input = [0_u8; 4];
            socket.read_exact(&mut input).unwrap();
            input
        });
        let mut client = connect_stream("127.0.0.1", i64::from(port)).unwrap();
        let mut greeting = [0_u8; 7];
        client.read_exact(&mut greeting).unwrap();
        assert_eq!(&greeting, b"ready\r\n");
        client.write_all(b"help").unwrap();
        assert_eq!(&server.join().unwrap(), b"help");
    }
}
