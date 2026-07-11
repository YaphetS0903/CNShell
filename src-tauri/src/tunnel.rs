use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::PortForward,
    ssh::SessionManager,
};
use parking_lot::Mutex;
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpListener, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Clone, Default)]
pub struct TunnelManager {
    running: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
    errors: Arc<Mutex<HashMap<String, String>>>,
}
impl TunnelManager {
    pub fn start_token(&self, id: &str) -> AppResult<Arc<AtomicBool>> {
        let mut running = self.running.lock();
        if running.contains_key(id) {
            return Err(AppError::Validation("隧道已在运行".into()));
        }
        let token = Arc::new(AtomicBool::new(false));
        running.insert(id.into(), token.clone());
        self.errors.lock().remove(id);
        Ok(token)
    }
    pub fn stop(&self, id: &str) -> AppResult<()> {
        let token = self
            .running
            .lock()
            .remove(id)
            .ok_or_else(|| AppError::NotFound(format!("运行中的隧道 {id}")))?;
        token.store(true, Ordering::Relaxed);
        Ok(())
    }
    pub fn finish(&self, id: &str, error: Option<String>) {
        self.running.lock().remove(id);
        if let Some(error) = error {
            self.errors.lock().insert(id.into(), error);
        }
    }
    pub fn status(&self, id: &str) -> (String, Option<String>) {
        if self.running.lock().contains_key(id) {
            ("running".into(), None)
        } else if let Some(error) = self.errors.lock().get(id) {
            ("failed".into(), Some(error.clone()))
        } else {
            ("stopped".into(), None)
        }
    }
}

fn transient(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
            | std::io::ErrorKind::Interrupted
    )
}

pub(crate) fn bridge(mut socket: TcpStream, mut channel: ssh2::Channel) {
    std::thread::spawn(move || {
        let _ = socket.set_nonblocking(true);
        let (mut to_remote, mut to_local) = (Vec::new(), Vec::new());
        let (mut client_eof, mut remote_eof, mut eof_sent) = (false, false, false);
        let (mut socket_buffer, mut channel_buffer) = ([0_u8; 32 * 1024], [0_u8; 32 * 1024]);
        loop {
            let mut progressed = false;
            if !client_eof && to_remote.is_empty() {
                match socket.read(&mut socket_buffer) {
                    Ok(0) => client_eof = true,
                    Ok(read) => {
                        to_remote.extend_from_slice(&socket_buffer[..read]);
                        progressed = true;
                    }
                    Err(error) if transient(&error) => {}
                    Err(_) => client_eof = true,
                }
            }
            if !to_remote.is_empty() {
                match channel.write(&to_remote) {
                    Ok(0) => {}
                    Ok(written) => {
                        to_remote.drain(..written);
                        progressed = true;
                    }
                    Err(error) if transient(&error) => {}
                    Err(_) => break,
                }
            }
            if client_eof && to_remote.is_empty() && !eof_sent {
                match channel.send_eof() {
                    Ok(()) => {
                        eof_sent = true;
                        progressed = true;
                    }
                    Err(error) if matches!(error.code(), ssh2::ErrorCode::Session(-37 | -9)) => {}
                    Err(_) => break,
                }
            }
            if !remote_eof && to_local.is_empty() {
                match channel.read(&mut channel_buffer) {
                    Ok(0) => remote_eof = true,
                    Ok(read) => {
                        to_local.extend_from_slice(&channel_buffer[..read]);
                        progressed = true;
                    }
                    Err(error) if transient(&error) => {}
                    Err(_) => break,
                }
            }
            if !to_local.is_empty() {
                match socket.write(&to_local) {
                    Ok(0) => break,
                    Ok(written) => {
                        to_local.drain(..written);
                        progressed = true;
                    }
                    Err(error) if transient(&error) => {}
                    Err(_) => break,
                }
            }
            if remote_eof && to_local.is_empty() {
                break;
            }
            if !progressed {
                std::thread::sleep(Duration::from_millis(2));
            }
        }
        let _ = socket.shutdown(Shutdown::Both);
        let _ = channel.close();
    });
}

fn read_socks_target(stream: &mut TcpStream) -> AppResult<(String, u16)> {
    let mut greeting = [0_u8; 2];
    stream.read_exact(&mut greeting)?;
    if greeting[0] != 5 {
        return Err(AppError::Remote("动态转发仅支持 SOCKS5".into()));
    }
    let mut methods = vec![0_u8; greeting[1] as usize];
    stream.read_exact(&mut methods)?;
    stream.write_all(&[5, 0])?;
    let mut head = [0_u8; 4];
    stream.read_exact(&mut head)?;
    if head[0] != 5 || head[1] != 1 {
        return Err(AppError::Remote("SOCKS5 仅支持 CONNECT".into()));
    }
    let host = match head[3] {
        1 => {
            let mut bytes = [0_u8; 4];
            stream.read_exact(&mut bytes)?;
            std::net::Ipv4Addr::from(bytes).to_string()
        }
        4 => {
            let mut bytes = [0_u8; 16];
            stream.read_exact(&mut bytes)?;
            std::net::Ipv6Addr::from(bytes).to_string()
        }
        3 => {
            let mut length = [0_u8; 1];
            stream.read_exact(&mut length)?;
            let mut bytes = vec![0_u8; length[0] as usize];
            stream.read_exact(&mut bytes)?;
            String::from_utf8(bytes).map_err(|_| AppError::Remote("SOCKS5 域名编码无效".into()))?
        }
        _ => return Err(AppError::Remote("SOCKS5 地址类型无效".into())),
    };
    let mut port = [0_u8; 2];
    stream.read_exact(&mut port)?;
    Ok((host, u16::from_be_bytes(port)))
}

pub async fn start(
    db: Database,
    sessions: SessionManager,
    manager: TunnelManager,
    forward: PortForward,
) -> AppResult<()> {
    let token = manager.start_token(&forward.id)?;
    let id = forward.id.clone();
    let profile = match db.get_connection(&forward.connection_id).await {
        Ok(profile) => profile,
        Err(error) => {
            manager.finish(&id, Some(error.to_string()));
            return Err(error);
        }
    };
    let transport = match sessions.acquire_transport(&db, &profile, false).await {
        Ok(transport) => transport,
        Err(error) => {
            manager.finish(&id, Some(error.to_string()));
            return Err(error);
        }
    };
    let manager_clone = manager.clone();
    std::thread::spawn(move || {
        let connected = transport.connected();
        let result = (|| -> AppResult<()> {
            match forward.forward_type.as_str() {
                "local" | "dynamic" => {
                    connected.session.set_timeout(20);
                    let listener =
                        TcpListener::bind(format!("{}:{}", forward.bind_host, forward.bind_port))?;
                    listener.set_nonblocking(true)?;
                    while !token.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok((mut client, address)) => {
                                client.set_nonblocking(false)?;
                                let (target, port) = if forward.forward_type == "dynamic" {
                                    match read_socks_target(&mut client) {
                                        Ok(value) => value,
                                        Err(error) => {
                                            let _ =
                                                client.write_all(&[5, 1, 0, 1, 0, 0, 0, 0, 0, 0]);
                                            return Err(error);
                                        }
                                    }
                                } else {
                                    (
                                        forward.destination_host.clone().unwrap_or_default(),
                                        forward.destination_port.unwrap_or(0) as u16,
                                    )
                                };
                                match connected.session.channel_direct_tcpip(
                                    &target,
                                    port,
                                    Some((&address.ip().to_string(), address.port())),
                                ) {
                                    Ok(channel) => {
                                        if forward.forward_type == "dynamic" {
                                            client.write_all(&[5, 0, 0, 1, 0, 0, 0, 0, 0, 0])?;
                                        }
                                        bridge(client, channel);
                                    }
                                    Err(error) => {
                                        if forward.forward_type == "dynamic" {
                                            let _ =
                                                client.write_all(&[5, 5, 0, 1, 0, 0, 0, 0, 0, 0]);
                                        }
                                        return Err(AppError::from(error));
                                    }
                                }
                            }
                            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                                std::thread::sleep(Duration::from_millis(40))
                            }
                            Err(error) => return Err(AppError::from(error)),
                        }
                    }
                }
                "remote" => {
                    let (mut listener, _) = connected.session.channel_forward_listen(
                        forward.bind_port as u16,
                        Some(&forward.bind_host),
                        Some(16),
                    )?;
                    connected.session.set_blocking(false);
                    while !token.load(Ordering::Relaxed) {
                        match listener.accept() {
                            Ok(channel) => {
                                let target =
                                    forward.destination_host.as_deref().unwrap_or("127.0.0.1");
                                let port = forward.destination_port.unwrap_or(0);
                                let local = TcpStream::connect(format!("{target}:{port}"))?;
                                bridge(local, channel);
                            }
                            Err(error) if error.code() == ssh2::ErrorCode::Session(-37) => {
                                std::thread::sleep(Duration::from_millis(40))
                            }
                            Err(error) => return Err(AppError::from(error)),
                        }
                    }
                }
                other => return Err(AppError::Validation(format!("不支持的隧道类型：{other}"))),
            }
            Ok(())
        })();
        manager_clone.finish(&id, result.err().map(|error| error.to_string()));
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn manager_tracks_lifecycle() {
        let manager = TunnelManager::default();
        let token = manager.start_token("x").unwrap();
        assert_eq!(manager.status("x").0, "running");
        manager.stop("x").unwrap();
        assert!(token.load(Ordering::Relaxed));
        assert_eq!(manager.status("x").0, "stopped");
        manager.start_token("x").unwrap();
        manager.finish("x", Some("connect failed".into()));
        let (status, error) = manager.status("x");
        assert_eq!(status, "failed");
        assert_eq!(error.as_deref(), Some("connect failed"));
        assert!(manager.start_token("x").is_ok());
    }
}
