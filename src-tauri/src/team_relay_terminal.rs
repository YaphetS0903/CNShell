use crate::{
    collaboration::CollaborationManager,
    db::Database,
    error::{AppError, AppResult},
    models::{
        TeamControlLease, TeamRelayTerminalEvent, TeamRelayTerminalSession,
        TeamTerminalEncryptedFrame, TeamTerminalParticipant, TeamTerminalRoom,
    },
    ssh::SessionManager,
    team_relay,
};
use futures_util::{SinkExt, StreamExt, stream::SplitSink};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, VecDeque},
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tauri::Emitter;
use tokio::{
    net::TcpStream,
    sync::{Mutex as AsyncMutex, mpsc, watch},
    time::{Instant as TokioInstant, interval_at, sleep_until},
};
use tokio_tungstenite::{
    MaybeTlsStream, WebSocketStream, connect_async,
    tungstenite::{Message, client::IntoClientRequest},
};

const MAX_PENDING_FRAMES: usize = 512;
const MAX_PENDING_BYTES: usize = 4 * 1024 * 1024;
const MAX_SOCKET_MESSAGE_BYTES: usize = 128 * 1024;

type RelaySocket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Serialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ClientSocketMessage<'a> {
    Frame {
        frame: &'a TeamTerminalEncryptedFrame,
    },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RelayControlLease {
    lease_id: String,
    member_id: String,
    device_id: String,
    generation: u64,
    expires_at: String,
}

impl From<RelayControlLease> for TeamControlLease {
    fn from(value: RelayControlLease) -> Self {
        Self {
            id: value.lease_id,
            member_id: value.member_id,
            device_id: value.device_id,
            generation: value.generation,
            expires_at: value.expires_at,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(
    tag = "type",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
enum ServerSocketMessage {
    Ready {
        latest_output_sequence: u64,
        next_input_sequence: u64,
    },
    Accepted {
        direction: String,
        sequence: u64,
    },
    Frame {
        frame: Box<TeamTerminalEncryptedFrame>,
    },
    Control {
        lease: Option<RelayControlLease>,
    },
    Participants {
        participants: Vec<TeamTerminalParticipant>,
    },
    Closed,
    Error {
        code: String,
        message: String,
    },
}

struct QueuedFrame {
    frame: TeamTerminalEncryptedFrame,
    encoded_bytes: usize,
}

enum RoomCommand {
    Frame(QueuedFrame),
}

struct ManagedRoom {
    state: Arc<Mutex<TeamRelayTerminalSession>>,
    sender: mpsc::Sender<RoomCommand>,
    close_sender: watch::Sender<bool>,
    pending_bytes: Arc<AtomicUsize>,
    crypto_lock: Arc<AsyncMutex<()>>,
}

struct RoomSendContext {
    state: Arc<Mutex<TeamRelayTerminalSession>>,
    sender: mpsc::Sender<RoomCommand>,
    pending_bytes: Arc<AtomicUsize>,
    crypto_lock: Arc<AsyncMutex<()>>,
}

#[derive(Clone, Default)]
pub struct TeamRelayTerminalManager {
    rooms: Arc<Mutex<HashMap<String, ManagedRoom>>>,
}

pub struct ConnectRoomInput {
    pub room_id: String,
    pub workspace_id: String,
    pub mode: String,
    pub terminal_session_id: Option<String>,
    pub local_member_id: String,
    pub local_device_id: String,
    pub after_sequence: u64,
    pub participants: Vec<TeamTerminalParticipant>,
    pub control_lease: Option<TeamControlLease>,
}

struct RoomRuntime {
    app: tauri::AppHandle,
    db: Database,
    collaboration: CollaborationManager,
    sessions: SessionManager,
    state: Arc<Mutex<TeamRelayTerminalSession>>,
    pending_bytes: Arc<AtomicUsize>,
}

impl TeamRelayTerminalManager {
    #[allow(clippy::too_many_arguments)]
    pub fn connect(
        &self,
        app: tauri::AppHandle,
        db: Database,
        collaboration: CollaborationManager,
        sessions: SessionManager,
        input: ConnectRoomInput,
    ) -> AppResult<TeamRelayTerminalSession> {
        if !matches!(input.mode.as_str(), "host" | "participant")
            || uuid::Uuid::parse_str(&input.room_id).is_err()
            || uuid::Uuid::parse_str(&input.workspace_id).is_err()
        {
            return Err(AppError::Validation(
                "在线团队终端房间模式或 ID 无效".into(),
            ));
        }
        let mut rooms = self.rooms.lock();
        rooms.retain(|_, room| !matches!(room.state.lock().status.as_str(), "closed" | "failed"));
        if rooms.contains_key(&input.room_id) {
            return Err(AppError::Validation(
                "在线团队终端房间已经在当前进程中连接".into(),
            ));
        }
        if rooms.len() >= 16 {
            return Err(AppError::Validation(
                "最多同时连接 16 个在线团队终端房间".into(),
            ));
        }
        let state = TeamRelayTerminalSession {
            room_id: input.room_id.clone(),
            workspace_id: input.workspace_id,
            mode: input.mode,
            terminal_session_id: input.terminal_session_id,
            local_member_id: input.local_member_id,
            local_device_id: input.local_device_id,
            status: "connecting".into(),
            last_error: None,
            last_output_sequence: input.after_sequence,
            participants: input.participants,
            control_lease: input.control_lease,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        let state = Arc::new(Mutex::new(state));
        let pending_bytes = Arc::new(AtomicUsize::new(0));
        let crypto_lock = Arc::new(AsyncMutex::new(()));
        let (sender, receiver) = mpsc::channel(MAX_PENDING_FRAMES);
        let (close_sender, close_receiver) = watch::channel(false);
        rooms.insert(
            input.room_id.clone(),
            ManagedRoom {
                state: state.clone(),
                sender,
                close_sender,
                pending_bytes: pending_bytes.clone(),
                crypto_lock,
            },
        );
        drop(rooms);
        let snapshot = state.lock().clone();
        emit_event(&app, &snapshot, "status", None, None);
        tauri::async_runtime::spawn(run_room(
            RoomRuntime {
                app,
                db,
                collaboration,
                sessions,
                state,
                pending_bytes,
            },
            receiver,
            close_receiver,
        ));
        Ok(snapshot)
    }

    pub fn list(&self) -> Vec<TeamRelayTerminalSession> {
        let mut sessions = self
            .rooms
            .lock()
            .values()
            .map(|room| room.state.lock().clone())
            .collect::<Vec<_>>();
        sessions.sort_by(|left, right| left.created_at.cmp(&right.created_at));
        sessions
    }

    pub fn status(&self, room_id: &str) -> AppResult<TeamRelayTerminalSession> {
        self.rooms
            .lock()
            .get(room_id)
            .map(|room| room.state.lock().clone())
            .ok_or_else(|| AppError::NotFound(format!("在线团队终端房间 {room_id}")))
    }

    pub async fn encrypt_and_enqueue_output(
        &self,
        db: &Database,
        collaboration: &CollaborationManager,
        room_id: &str,
        bytes: &[u8],
    ) -> AppResult<()> {
        let context = self.room_sender(room_id, "host")?;
        let _serial = context.crypto_lock.lock().await;
        validate_send_state(&context.state, "host")?;
        let permit = context
            .sender
            .reserve_owned()
            .await
            .map_err(|_| AppError::Unavailable("在线团队终端连接已经停止".into()))?;
        reserve_pending_bytes(&context.pending_bytes, MAX_SOCKET_MESSAGE_BYTES)?;
        let frame = match collaboration
            .publish_encrypted_output(db, room_id, bytes)
            .await
        {
            Ok(frame) => frame,
            Err(error) => {
                release_pending_bytes(&context.pending_bytes, MAX_SOCKET_MESSAGE_BYTES);
                return Err(error);
            }
        };
        self.finish_reserved_frame(permit, context.pending_bytes, frame)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn encrypt_and_enqueue_input(
        &self,
        db: &Database,
        collaboration: &CollaborationManager,
        room_id: &str,
        lease_id: &str,
        lease_generation: u64,
        bytes: &[u8],
    ) -> AppResult<()> {
        let context = self.room_sender(room_id, "participant")?;
        let _serial = context.crypto_lock.lock().await;
        validate_send_state(&context.state, "participant")?;
        let permit = context
            .sender
            .reserve_owned()
            .await
            .map_err(|_| AppError::Unavailable("在线团队终端连接已经停止".into()))?;
        reserve_pending_bytes(&context.pending_bytes, MAX_SOCKET_MESSAGE_BYTES)?;
        let frame = match collaboration
            .encrypt_input(db, room_id, lease_id, lease_generation, bytes)
            .await
        {
            Ok(frame) => frame,
            Err(error) => {
                release_pending_bytes(&context.pending_bytes, MAX_SOCKET_MESSAGE_BYTES);
                return Err(error);
            }
        };
        self.finish_reserved_frame(permit, context.pending_bytes, frame)
    }

    fn room_sender(&self, room_id: &str, expected_mode: &str) -> AppResult<RoomSendContext> {
        let rooms = self.rooms.lock();
        let room = rooms
            .get(room_id)
            .ok_or_else(|| AppError::NotFound(format!("在线团队终端房间 {room_id}")))?;
        validate_send_state(&room.state, expected_mode)?;
        Ok(RoomSendContext {
            state: room.state.clone(),
            sender: room.sender.clone(),
            pending_bytes: room.pending_bytes.clone(),
            crypto_lock: room.crypto_lock.clone(),
        })
    }

    fn finish_reserved_frame(
        &self,
        permit: mpsc::OwnedPermit<RoomCommand>,
        pending_bytes: Arc<AtomicUsize>,
        frame: TeamTerminalEncryptedFrame,
    ) -> AppResult<()> {
        let encoded_bytes = match serde_jcs::to_vec(&frame) {
            Ok(encoded) => encoded.len(),
            Err(error) => {
                release_pending_bytes(&pending_bytes, MAX_SOCKET_MESSAGE_BYTES);
                return Err(AppError::Validation(format!("在线团队终端帧无效：{error}")));
            }
        };
        if encoded_bytes > MAX_SOCKET_MESSAGE_BYTES {
            release_pending_bytes(&pending_bytes, MAX_SOCKET_MESSAGE_BYTES);
            return Err(AppError::Validation(
                "在线团队终端帧超过 128 KB 上限".into(),
            ));
        }
        release_pending_bytes(&pending_bytes, MAX_SOCKET_MESSAGE_BYTES - encoded_bytes);
        permit.send(RoomCommand::Frame(QueuedFrame {
            frame,
            encoded_bytes,
        }));
        Ok(())
    }

    pub fn update_host_control(
        &self,
        app: &tauri::AppHandle,
        room: &TeamTerminalRoom,
    ) -> AppResult<TeamRelayTerminalSession> {
        let rooms = self.rooms.lock();
        let managed = rooms
            .get(&room.id)
            .ok_or_else(|| AppError::NotFound(format!("在线团队终端房间 {}", room.id)))?;
        let mut state = managed.state.lock();
        if state.mode != "host" || state.workspace_id != room.workspace_id {
            return Err(AppError::PermissionDenied(
                "在线团队终端主持房间身份不匹配".into(),
            ));
        }
        state.control_lease = room.control_lease.clone();
        let snapshot = state.clone();
        drop(state);
        emit_event(app, &snapshot, "control", None, None);
        Ok(snapshot)
    }

    pub fn close(&self, room_id: &str) -> AppResult<TeamRelayTerminalSession> {
        let rooms = self.rooms.lock();
        let room = rooms
            .get(room_id)
            .ok_or_else(|| AppError::NotFound(format!("在线团队终端房间 {room_id}")))?;
        let _ = room.close_sender.send(true);
        let mut state = room.state.lock();
        state.status = "closed".into();
        state.control_lease = None;
        Ok(state.clone())
    }

    pub fn close_all(&self) {
        let rooms = self.rooms.lock();
        for room in rooms.values() {
            let _ = room.close_sender.send(true);
            let mut state = room.state.lock();
            state.status = "closed".into();
            state.control_lease = None;
        }
    }
}

fn validate_send_state(
    state: &Arc<Mutex<TeamRelayTerminalSession>>,
    expected_mode: &str,
) -> AppResult<()> {
    let state = state.lock();
    if state.mode != expected_mode {
        return Err(AppError::PermissionDenied(
            "当前在线团队终端角色不能发送该方向的帧".into(),
        ));
    }
    if matches!(state.status.as_str(), "failed" | "closed") {
        return Err(AppError::Unavailable(
            state
                .last_error
                .clone()
                .unwrap_or_else(|| "在线团队终端房间未连接".into()),
        ));
    }
    Ok(())
}

fn reserve_pending_bytes(pending: &AtomicUsize, bytes: usize) -> AppResult<()> {
    let mut current = pending.load(Ordering::Acquire);
    loop {
        let Some(next) = current.checked_add(bytes) else {
            return Err(AppError::Unavailable("在线团队终端待发队列大小溢出".into()));
        };
        if next > MAX_PENDING_BYTES {
            return Err(AppError::Unavailable(
                "在线团队终端待发队列达到 4 MiB 上限".into(),
            ));
        }
        match pending.compare_exchange_weak(current, next, Ordering::AcqRel, Ordering::Acquire) {
            Ok(_) => return Ok(()),
            Err(actual) => current = actual,
        }
    }
}

fn release_pending_bytes(pending: &AtomicUsize, bytes: usize) {
    let _ = pending.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_sub(bytes))
    });
}

async fn run_room(
    runtime: RoomRuntime,
    mut receiver: mpsc::Receiver<RoomCommand>,
    mut close_receiver: watch::Receiver<bool>,
) {
    let RoomRuntime {
        app,
        db,
        collaboration,
        sessions,
        state,
        pending_bytes,
    } = runtime;
    let mut pending = VecDeque::<QueuedFrame>::new();
    let mut reconnect_attempt = 0_u32;
    let mut next_connect_at = TokioInstant::now();
    'connection: loop {
        if *close_receiver.borrow() {
            finish_room(&app, &state, &pending_bytes, "closed", None);
            return;
        }
        while TokioInstant::now() < next_connect_at {
            tokio::select! {
                _ = close_receiver.changed() => {
                    finish_room(&app, &state, &pending_bytes, "closed", None);
                    return;
                }
                _ = sleep_until(next_connect_at) => break,
                command = receiver.recv() => {
                    if !queue_command(command, &mut pending, &pending_bytes) {
                        finish_room(&app, &state, &pending_bytes, "closed", None);
                        return;
                    }
                }
            }
        }
        let snapshot = state.lock().clone();
        let context = match team_relay::device_context(&db, &snapshot.workspace_id).await {
            Ok(context) => context,
            Err(error) => {
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                set_reconnecting(&app, &state, &error.to_string());
                next_connect_at = TokioInstant::now() + reconnect_delay(reconnect_attempt);
                continue;
            }
        };
        if context.workspace_id != snapshot.workspace_id
            || context.member_id != snapshot.local_member_id
            || context.device_id != snapshot.local_device_id
        {
            finish_room(
                &app,
                &state,
                &pending_bytes,
                "failed",
                Some("在线团队设备会话在重连期间切换了本机身份".into()),
            );
            return;
        }
        let request = match socket_request(
            &context.base_url,
            &snapshot.room_id,
            snapshot.last_output_sequence,
            &context.token,
        ) {
            Ok(request) => request,
            Err(error) => {
                finish_room(
                    &app,
                    &state,
                    &pending_bytes,
                    "failed",
                    Some(error.to_string()),
                );
                return;
            }
        };
        let socket = match connect_async(request).await {
            Ok((socket, _)) => socket,
            Err(error) => {
                reconnect_attempt = reconnect_attempt.saturating_add(1);
                set_reconnecting(&app, &state, &format!("WebSocket 连接失败：{error}"));
                next_connect_at = TokioInstant::now() + reconnect_delay(reconnect_attempt);
                continue;
            }
        };
        reconnect_attempt = 0;
        let (mut sink, mut stream) = socket.split();
        let mut ready = false;
        let token_deadline = token_refresh_deadline(&context.expires_at);
        let mut heartbeat = interval_at(
            TokioInstant::now() + Duration::from_secs(15),
            Duration::from_secs(15),
        );
        heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                _ = close_receiver.changed() => {
                    let _ = sink.send(Message::Close(None)).await;
                    finish_room(&app, &state, &pending_bytes, "closed", None);
                    return;
                }
                _ = sleep_until(token_deadline) => {
                    set_reconnecting(&app, &state, "设备会话即将过期，正在刷新");
                    next_connect_at = TokioInstant::now();
                    continue 'connection;
                }
                _ = heartbeat.tick() => {
                    if sink.send(Message::Ping(Vec::new().into())).await.is_err() {
                        set_reconnecting(&app, &state, "WebSocket 心跳失败，正在重连");
                        next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                        continue 'connection;
                    }
                }
                command = receiver.recv() => {
                    match command {
                        Some(RoomCommand::Frame(frame)) => {
                            pending.push_back(frame);
                            if ready && pending.len() == 1
                                && send_front(&mut sink, &pending).await.is_err()
                            {
                                set_reconnecting(&app, &state, "WebSocket 发送失败，正在重连");
                                next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                                continue 'connection;
                            }
                        }
                        None => {
                            let _ = sink.send(Message::Close(None)).await;
                            finish_room(&app, &state, &pending_bytes, "closed", None);
                            return;
                        }
                    }
                }
                incoming = stream.next() => {
                    let Some(Ok(message)) = incoming else {
                        set_reconnecting(&app, &state, "WebSocket 已断开，正在重连");
                        next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                        continue 'connection;
                    };
                    match message {
                        Message::Text(text) if text.len() <= MAX_SOCKET_MESSAGE_BYTES => {
                            let parsed = match serde_json::from_str::<ServerSocketMessage>(&text) {
                                Ok(parsed) => parsed,
                                Err(_) => {
                                    finish_room(&app, &state, &pending_bytes, "failed", Some("团队服务返回了无效 WebSocket 消息".into()));
                                    return;
                                }
                            };
                            match handle_server_message(
                                &app,
                                &db,
                                &collaboration,
                                &sessions,
                                &state,
                                &pending_bytes,
                                &mut pending,
                                parsed,
                            ).await {
                                Ok(ServerAction::None) => {}
                                Ok(ServerAction::Ready) => {
                                    ready = true;
                                    set_online(&app, &state);
                                    if !pending.is_empty() && send_front(&mut sink, &pending).await.is_err() {
                                        set_reconnecting(&app, &state, "WebSocket 发送失败，正在重连");
                                        next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                                        continue 'connection;
                                    }
                                }
                                Ok(ServerAction::SendNext) => {
                                    if ready && !pending.is_empty() && send_front(&mut sink, &pending).await.is_err() {
                                        set_reconnecting(&app, &state, "WebSocket 发送失败，正在重连");
                                        next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                                        continue 'connection;
                                    }
                                }
                                Ok(ServerAction::Reconnect(message)) => {
                                    set_reconnecting(&app, &state, &message);
                                    next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                                    continue 'connection;
                                }
                                Ok(ServerAction::Closed) => {
                                    finish_room(&app, &state, &pending_bytes, "closed", None);
                                    return;
                                }
                                Err(error) => {
                                    finish_room(&app, &state, &pending_bytes, "failed", Some(error.to_string()));
                                    return;
                                }
                            }
                        }
                        Message::Ping(value) => {
                            if sink.send(Message::Pong(value)).await.is_err() {
                                next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                                continue 'connection;
                            }
                        }
                        Message::Close(_) => {
                            set_reconnecting(&app, &state, "在线团队终端房间连接已关闭");
                            next_connect_at = TokioInstant::now() + Duration::from_secs(1);
                            continue 'connection;
                        }
                        Message::Text(_) | Message::Binary(_) => {
                            finish_room(&app, &state, &pending_bytes, "failed", Some("团队服务 WebSocket 消息超过限制或类型无效".into()));
                            return;
                        }
                        Message::Pong(_) | Message::Frame(_) => {}
                    }
                }
            }
        }
    }
}

enum ServerAction {
    None,
    Ready,
    SendNext,
    Reconnect(String),
    Closed,
}

#[allow(clippy::too_many_arguments)]
async fn handle_server_message(
    app: &tauri::AppHandle,
    db: &Database,
    collaboration: &CollaborationManager,
    sessions: &SessionManager,
    state: &Arc<Mutex<TeamRelayTerminalSession>>,
    pending_bytes: &AtomicUsize,
    pending: &mut VecDeque<QueuedFrame>,
    message: ServerSocketMessage,
) -> AppResult<ServerAction> {
    match message {
        ServerSocketMessage::Ready {
            latest_output_sequence,
            next_input_sequence,
        } => {
            let snapshot = state.lock().clone();
            let expected = if snapshot.mode == "host" {
                latest_output_sequence.saturating_add(1)
            } else {
                next_input_sequence
            };
            reconcile_pending(pending, pending_bytes, expected)?;
            if snapshot.mode == "host" {
                let mut current = state.lock();
                current.last_output_sequence =
                    current.last_output_sequence.max(latest_output_sequence);
            }
            Ok(ServerAction::Ready)
        }
        ServerSocketMessage::Accepted {
            direction,
            sequence,
        } => {
            if acknowledge_front(pending, pending_bytes, &direction, sequence) {
                if direction == "output" {
                    let mut current = state.lock();
                    current.last_output_sequence = current.last_output_sequence.max(sequence);
                }
                return Ok(ServerAction::SendNext);
            }
            Ok(ServerAction::None)
        }
        ServerSocketMessage::Frame { frame } => {
            let snapshot = state.lock().clone();
            match (snapshot.mode.as_str(), frame.direction.as_str()) {
                ("host", "output") => {
                    let mut current = state.lock();
                    current.last_output_sequence = current.last_output_sequence.max(frame.sequence);
                    Ok(ServerAction::None)
                }
                ("participant", "output") => {
                    let output = collaboration.decrypt_output(db, *frame).await?;
                    let mut current = state.lock();
                    current.last_output_sequence = output.sequence;
                    let snapshot = current.clone();
                    drop(current);
                    emit_event(
                        app,
                        &snapshot,
                        "output",
                        Some(output.sequence),
                        Some(output.data_base64),
                    );
                    Ok(ServerAction::None)
                }
                ("host", "input") => {
                    let (session_id, data) =
                        collaboration.receive_encrypted_input(db, *frame).await?;
                    crate::ssh::terminal_input(sessions.clone(), session_id, data).await?;
                    Ok(ServerAction::None)
                }
                _ => Err(AppError::PermissionDenied(
                    "团队服务把终端帧路由到了错误的客户端角色".into(),
                )),
            }
        }
        ServerSocketMessage::Control { lease } => {
            let next_lease = lease.map(Into::into).filter(|lease: &TeamControlLease| {
                chrono::DateTime::parse_from_rfc3339(&lease.expires_at)
                    .map(|expires| expires.with_timezone(&chrono::Utc) > chrono::Utc::now())
                    .unwrap_or(false)
            });
            let current_snapshot = state.lock().clone();
            if current_snapshot.mode == "host" {
                let room = match next_lease.clone() {
                    Some(lease) => {
                        collaboration
                            .apply_control_lease(db, &current_snapshot.room_id, lease)
                            .await?
                    }
                    None => {
                        collaboration
                            .revoke_control(db, &current_snapshot.room_id)
                            .await?
                    }
                };
                let mut current = state.lock();
                current.control_lease = room.control_lease;
                let snapshot = current.clone();
                drop(current);
                emit_event(app, &snapshot, "control", None, None);
                return Ok(ServerAction::None);
            }
            let mut current = state.lock();
            current.control_lease = next_lease;
            let snapshot = current.clone();
            drop(current);
            emit_event(app, &snapshot, "control", None, None);
            Ok(ServerAction::None)
        }
        ServerSocketMessage::Participants { participants } => {
            let mut current = state.lock();
            current.participants = participants;
            let snapshot = current.clone();
            drop(current);
            emit_event(app, &snapshot, "participants", None, None);
            Ok(ServerAction::None)
        }
        ServerSocketMessage::Closed => Ok(ServerAction::Closed),
        ServerSocketMessage::Error { code, message } => {
            if code == "authentication" {
                Ok(ServerAction::Reconnect(message))
            } else {
                Err(AppError::Remote(format!(
                    "团队服务拒绝 WebSocket 操作：{message}"
                )))
            }
        }
    }
}

fn queue_command(
    command: Option<RoomCommand>,
    pending: &mut VecDeque<QueuedFrame>,
    pending_bytes: &AtomicUsize,
) -> bool {
    match command {
        Some(RoomCommand::Frame(frame)) => {
            pending.push_back(frame);
            true
        }
        None => {
            pending.clear();
            pending_bytes.store(0, Ordering::Release);
            false
        }
    }
}

fn confirm_front(pending: &mut VecDeque<QueuedFrame>, pending_bytes: &AtomicUsize) {
    if let Some(frame) = pending.pop_front() {
        release_pending_bytes(pending_bytes, frame.encoded_bytes);
    }
}

fn reconcile_pending(
    pending: &mut VecDeque<QueuedFrame>,
    pending_bytes: &AtomicUsize,
    expected_sequence: u64,
) -> AppResult<()> {
    while pending
        .front()
        .is_some_and(|queued| queued.frame.sequence < expected_sequence)
    {
        confirm_front(pending, pending_bytes);
    }
    if pending
        .front()
        .is_some_and(|queued| queued.frame.sequence > expected_sequence)
    {
        return Err(AppError::Unavailable(
            "在线团队终端待发序号与服务端游标不连续，请重新加入房间".into(),
        ));
    }
    Ok(())
}

fn acknowledge_front(
    pending: &mut VecDeque<QueuedFrame>,
    pending_bytes: &AtomicUsize,
    direction: &str,
    sequence: u64,
) -> bool {
    if !pending.front().is_some_and(|queued| {
        queued.frame.direction == direction && queued.frame.sequence == sequence
    }) {
        return false;
    }
    confirm_front(pending, pending_bytes);
    true
}

async fn send_front(
    sink: &mut SplitSink<RelaySocket, Message>,
    pending: &VecDeque<QueuedFrame>,
) -> AppResult<()> {
    let Some(front) = pending.front() else {
        return Ok(());
    };
    let payload = serde_json::to_string(&ClientSocketMessage::Frame {
        frame: &front.frame,
    })
    .map_err(|error| AppError::Internal(format!("编码在线团队终端帧失败：{error}")))?;
    sink.send(Message::Text(payload.into()))
        .await
        .map_err(|error| AppError::Remote(format!("发送在线团队终端帧失败：{error}")))
}

fn socket_request(
    base_url: &str,
    room_id: &str,
    after_sequence: u64,
    token: &str,
) -> AppResult<tokio_tungstenite::tungstenite::http::Request<()>> {
    let mut url = reqwest::Url::parse(base_url)
        .map_err(|_| AppError::Validation("团队服务 WebSocket 地址无效".into()))?;
    let scheme = match url.scheme() {
        "https" => "wss",
        "http" => "ws",
        _ => return Err(AppError::Validation("团队服务 WebSocket 协议无效".into())),
    };
    url.set_scheme(scheme)
        .map_err(|_| AppError::Validation("团队服务 WebSocket 协议无效".into()))?;
    url.set_path(&format!("/v1/terminal/ws/{room_id}"));
    url.set_query(Some(&format!("afterSequence={after_sequence}")));
    let mut request = url
        .as_str()
        .into_client_request()
        .map_err(|_| AppError::Validation("团队服务 WebSocket 请求无效".into()))?;
    request.headers_mut().insert(
        "Authorization",
        format!("Bearer {token}")
            .parse()
            .map_err(|_| AppError::Authentication("设备会话令牌无法写入请求".into()))?,
    );
    Ok(request)
}

fn token_refresh_deadline(expires_at: &str) -> TokioInstant {
    let remaining = chrono::DateTime::parse_from_rfc3339(expires_at)
        .map(|expires| {
            (expires.with_timezone(&chrono::Utc) - chrono::Utc::now())
                .to_std()
                .unwrap_or_default()
        })
        .unwrap_or_default();
    TokioInstant::now() + remaining.saturating_sub(Duration::from_secs(30))
}

fn reconnect_delay(attempt: u32) -> Duration {
    Duration::from_secs(match attempt {
        0 | 1 => 1,
        2 => 2,
        3 => 5,
        4 => 10,
        _ => 30,
    })
}

fn set_online(app: &tauri::AppHandle, state: &Arc<Mutex<TeamRelayTerminalSession>>) {
    let mut current = state.lock();
    current.status = "online".into();
    current.last_error = None;
    let snapshot = current.clone();
    drop(current);
    emit_event(app, &snapshot, "status", None, None);
}

fn set_reconnecting(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<TeamRelayTerminalSession>>,
    message: &str,
) {
    let mut current = state.lock();
    current.status = "reconnecting".into();
    current.last_error = Some(message.into());
    let snapshot = current.clone();
    drop(current);
    emit_event(app, &snapshot, "status", None, None);
}

fn finish_room(
    app: &tauri::AppHandle,
    state: &Arc<Mutex<TeamRelayTerminalSession>>,
    pending_bytes: &AtomicUsize,
    status: &str,
    error: Option<String>,
) {
    pending_bytes.store(0, Ordering::Release);
    let mut current = state.lock();
    current.status = status.into();
    current.last_error = error;
    current.control_lease = None;
    let snapshot = current.clone();
    drop(current);
    emit_event(app, &snapshot, "status", None, None);
}

fn emit_event(
    app: &tauri::AppHandle,
    session: &TeamRelayTerminalSession,
    kind: &str,
    sequence: Option<u64>,
    data_base64: Option<String>,
) {
    let _ = app.emit(
        "team-relay-terminal-event",
        TeamRelayTerminalEvent {
            room_id: session.room_id.clone(),
            kind: kind.into(),
            session: session.clone(),
            sequence,
            data_base64,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    fn queued(direction: &str, sequence: u64, encoded_bytes: usize) -> QueuedFrame {
        QueuedFrame {
            frame: TeamTerminalEncryptedFrame {
                schema_version: 1,
                workspace_id: "11111111-1111-4111-8111-111111111111".into(),
                room_id: "22222222-2222-4222-8222-222222222222".into(),
                key_epoch: 1,
                sender_member_id: "33333333-3333-4333-8333-333333333333".into(),
                sender_device_id: "44444444-4444-4444-8444-444444444444".into(),
                direction: direction.into(),
                kind: "terminal".into(),
                sequence,
                lease_id: (direction == "input")
                    .then(|| "55555555-5555-4555-8555-555555555555".into()),
                lease_generation: u64::from(direction == "input"),
                nonce: "xchacha20poly1305:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA".into(),
                ciphertext: "AA".into(),
                signature: Some(format!("ed25519:{}", "a".repeat(86))),
            },
            encoded_bytes,
        }
    }

    #[test]
    fn websocket_url_and_pending_limits_are_bounded() {
        let request = socket_request(
            "https://relay.example.com/",
            "11111111-1111-4111-8111-111111111111",
            42,
            &"a".repeat(43),
        )
        .unwrap();
        assert_eq!(
            request.uri().to_string(),
            "wss://relay.example.com/v1/terminal/ws/11111111-1111-4111-8111-111111111111?afterSequence=42"
        );
        assert!(request.headers().get("Authorization").is_some());
        let pending = AtomicUsize::new(MAX_PENDING_BYTES - 1);
        assert!(reserve_pending_bytes(&pending, 1).is_ok());
        assert!(reserve_pending_bytes(&pending, 1).is_err());
        pending.store(0, Ordering::Release);
        release_pending_bytes(&pending, MAX_SOCKET_MESSAGE_BYTES);
        assert_eq!(pending.load(Ordering::Acquire), 0);
    }

    #[test]
    fn ready_drops_only_frames_already_committed_by_the_relay() {
        let mut frames = VecDeque::from([queued("output", 4, 40), queued("output", 5, 50)]);
        let bytes = AtomicUsize::new(90);

        reconcile_pending(&mut frames, &bytes, 5).unwrap();

        assert_eq!(frames.front().map(|frame| frame.frame.sequence), Some(5));
        assert_eq!(bytes.load(Ordering::Acquire), 50);
    }

    #[test]
    fn ready_rejects_a_missing_outbound_sequence() {
        let mut frames = VecDeque::from([queued("output", 7, 40)]);
        let bytes = AtomicUsize::new(40);

        assert!(reconcile_pending(&mut frames, &bytes, 6).is_err());
        assert_eq!(frames.len(), 1);
        assert_eq!(bytes.load(Ordering::Acquire), 40);
    }

    #[test]
    fn accepted_advances_exactly_once() {
        let mut frames = VecDeque::from([queued("output", 8, 40), queued("output", 9, 50)]);
        let bytes = AtomicUsize::new(90);

        assert!(acknowledge_front(&mut frames, &bytes, "output", 8));
        assert!(!acknowledge_front(&mut frames, &bytes, "output", 8));
        assert_eq!(frames.front().map(|frame| frame.frame.sequence), Some(9));
        assert_eq!(bytes.load(Ordering::Acquire), 50);
    }

    #[test]
    fn reconnect_retains_an_unacknowledged_output_frame() {
        let mut frames = VecDeque::from([queued("output", 12, 64)]);
        let bytes = AtomicUsize::new(64);

        reconcile_pending(&mut frames, &bytes, 12).unwrap();

        assert_eq!(frames.front().map(|frame| frame.frame.sequence), Some(12));
        assert_eq!(bytes.load(Ordering::Acquire), 64);
    }

    #[test]
    fn input_cursor_recovery_uses_the_same_confirmed_prefix_rule() {
        let mut frames = VecDeque::from([queued("input", 2, 30), queued("input", 3, 31)]);
        let bytes = AtomicUsize::new(61);

        reconcile_pending(&mut frames, &bytes, 3).unwrap();

        assert_eq!(frames.front().map(|frame| frame.frame.sequence), Some(3));
        assert_eq!(bytes.load(Ordering::Acquire), 31);
    }
}
