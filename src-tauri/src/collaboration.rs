use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{
        TeamControlLease, TeamDevice, TeamTerminalClientRoom, TeamTerminalEncryptedFrame,
        TeamTerminalFrame, TeamTerminalInvitation, TeamTerminalParticipant, TeamTerminalRoom,
    },
    team, team_share,
};
use aes_gcm::{
    Aes256Gcm, KeyInit, Nonce,
    aead::{Aead, Payload},
};
use base64::{
    Engine as _,
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use hkdf::Hkdf;
use parking_lot::Mutex;
use rand::{RngCore, rngs::OsRng};
use sha2::Sha256;
use sqlx::Row;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::{Duration, Instant},
};
use x25519_dalek::{PublicKey as X25519PublicKey, StaticSecret};
use zeroize::{Zeroize, Zeroizing};

const MAX_ROOMS: usize = 16;
const MAX_PARTICIPANTS: usize = 64;
const MAX_FRAME_BYTES: usize = 64 * 1024;
const MIN_LEASE_SECONDS: u64 = 10;
const MAX_LEASE_SECONDS: u64 = 5 * 60;
const INVITATION_TTL: Duration = Duration::from_secs(5 * 60);
const REPLAY_TTL: Duration = Duration::from_secs(5 * 60);
const MAX_REPLAY_FRAMES: usize = 512;
const MAX_REPLAY_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENVELOPE_BYTES: usize = 128 * 1024;

#[derive(Clone)]
struct ParticipantState {
    participant: TeamTerminalParticipant,
    next_input_sequence: u64,
}

#[derive(Clone)]
struct LeaseState {
    lease: TeamControlLease,
    expires_at: Instant,
}

struct StoredFrame {
    frame: TeamTerminalEncryptedFrame,
    bytes: usize,
    created_at: Instant,
}

struct RoomState {
    id: String,
    workspace_id: String,
    terminal_session_id: String,
    host_member_id: String,
    host_device_id: String,
    key_epoch: i64,
    status: String,
    participants: HashMap<String, ParticipantState>,
    control_lease: Option<LeaseState>,
    next_output_sequence: u64,
    lease_generation: u64,
    room_key: Zeroizing<[u8; 32]>,
    replay: VecDeque<StoredFrame>,
    replay_bytes: usize,
    encrypted_transport_enabled: bool,
    created_at: String,
}

impl RoomState {
    fn snapshot(&mut self) -> TeamTerminalRoom {
        self.expire_lease();
        let mut participants = self
            .participants
            .values()
            .map(|value| value.participant.clone())
            .collect::<Vec<_>>();
        participants.sort_by(|left, right| {
            left.joined_at
                .cmp(&right.joined_at)
                .then_with(|| left.device_id.cmp(&right.device_id))
        });
        TeamTerminalRoom {
            id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            terminal_session_id: self.terminal_session_id.clone(),
            host_member_id: self.host_member_id.clone(),
            host_device_id: self.host_device_id.clone(),
            key_epoch: self.key_epoch,
            status: self.status.clone(),
            participants,
            control_lease: self.control_lease.as_ref().map(|value| value.lease.clone()),
            next_output_sequence: self.next_output_sequence,
            created_at: self.created_at.clone(),
        }
    }

    fn expire_lease(&mut self) {
        if self
            .control_lease
            .as_ref()
            .is_some_and(|value| value.expires_at <= Instant::now())
        {
            self.control_lease = None;
        }
    }

    fn prune_replay(&mut self) {
        while self.replay.front().is_some_and(|value| {
            value.created_at.elapsed() > REPLAY_TTL
                || self.replay.len() > MAX_REPLAY_FRAMES
                || self.replay_bytes > MAX_REPLAY_BYTES
        }) {
            if let Some(frame) = self.replay.pop_front() {
                self.replay_bytes = self.replay_bytes.saturating_sub(frame.bytes);
            }
        }
    }

    fn replay_from_sequence(&mut self) -> u64 {
        self.prune_replay();
        self.replay
            .front()
            .map(|value| value.frame.sequence.saturating_sub(1))
            .unwrap_or_else(|| self.next_output_sequence.saturating_sub(1))
    }
}

struct ClientRoomState {
    workspace_id: String,
    host_member_id: String,
    host_device_id: String,
    local_member_id: String,
    local_device_id: String,
    key_epoch: i64,
    room_key: Zeroizing<[u8; 32]>,
    next_output_sequence: u64,
    next_input_sequence: u64,
    status: String,
}

impl ClientRoomState {
    fn snapshot(&self, room_id: &str) -> TeamTerminalClientRoom {
        TeamTerminalClientRoom {
            room_id: room_id.into(),
            workspace_id: self.workspace_id.clone(),
            key_epoch: self.key_epoch,
            host_member_id: self.host_member_id.clone(),
            host_device_id: self.host_device_id.clone(),
            local_member_id: self.local_member_id.clone(),
            local_device_id: self.local_device_id.clone(),
            next_output_sequence: self.next_output_sequence,
            next_input_sequence: self.next_input_sequence,
            status: self.status.clone(),
        }
    }
}

#[derive(Clone, Default)]
pub struct CollaborationManager {
    rooms: Arc<Mutex<HashMap<String, RoomState>>>,
    client_rooms: Arc<Mutex<HashMap<String, ClientRoomState>>>,
}

async fn device_member_role(
    db: &Database,
    workspace_id: &str,
    device_id: &str,
) -> AppResult<(String, String)> {
    if uuid::Uuid::parse_str(device_id).is_err() {
        return Err(AppError::Validation("协作设备 ID 无效".into()));
    }
    let row = sqlx::query("SELECT d.member_id,m.role FROM team_devices d JOIN team_members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.status='active' AND m.status='active'")
        .bind(device_id)
        .bind(workspace_id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| AppError::PermissionDenied("协作设备或成员不存在、已移除或已撤销".into()))?;
    Ok((row.get(0), row.get(1)))
}

fn validate_host(room: &RoomState, authorization: &team::TeamAuthorization) -> AppResult<()> {
    if room.host_member_id != authorization.member_id
        || authorization.local_device_id.as_deref() != Some(room.host_device_id.as_str())
        || room.key_epoch != authorization.key_epoch
    {
        return Err(AppError::PermissionDenied(
            "团队终端房间的主持成员、设备或密钥 epoch 已失效".into(),
        ));
    }
    Ok(())
}

async fn active_device(
    db: &Database,
    workspace_id: &str,
    device_id: &str,
    member_id: &str,
) -> AppResult<TeamDevice> {
    let device = sqlx::query_as::<_, TeamDevice>("SELECT d.id,d.workspace_id,d.member_id,d.name,d.encryption_public_key,d.signing_public_key,d.fingerprint,d.is_local,d.status,d.created_at,d.updated_at,d.revoked_at FROM team_devices d JOIN team_members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.member_id=? AND d.status='active' AND m.status='active'")
        .bind(device_id)
        .bind(workspace_id)
        .bind(member_id)
        .fetch_optional(&db.pool)
        .await?
        .ok_or_else(|| AppError::PermissionDenied("团队终端发送设备或成员已移除或撤销".into()))?;
    team_share::validated_device_keys(&device)?;
    Ok(device)
}

fn room_wrap_aad(workspace_id: &str, room_id: &str, key_epoch: i64, device_id: &str) -> Vec<u8> {
    format!("cnshell-team-room-wrap-v1\0{workspace_id}\0{room_id}\0{key_epoch}\0{device_id}")
        .into_bytes()
}

fn derive_room_wrapping_key(
    shared_secret: &[u8; 32],
    workspace_id: &str,
    room_id: &str,
    key_epoch: i64,
    device_id: &str,
) -> AppResult<Zeroizing<[u8; 32]>> {
    let hkdf = Hkdf::<Sha256>::new(Some(room_id.as_bytes()), shared_secret);
    let mut wrapping_key = Zeroizing::new([0_u8; 32]);
    hkdf.expand(
        format!("cnshell-team-room-key-v1\0{workspace_id}\0{key_epoch}\0{device_id}").as_bytes(),
        &mut *wrapping_key,
    )
    .map_err(|_| AppError::Internal("派生团队终端房间封装密钥失败".into()))?;
    Ok(wrapping_key)
}

fn wrap_room_key(
    shared_secret: &[u8; 32],
    workspace_id: &str,
    room_id: &str,
    key_epoch: i64,
    device_id: &str,
    room_key: &[u8; 32],
    nonce: &[u8; 12],
) -> AppResult<Vec<u8>> {
    let wrapping_key =
        derive_room_wrapping_key(shared_secret, workspace_id, room_id, key_epoch, device_id)?;
    Aes256Gcm::new_from_slice(&wrapping_key[..])
        .map_err(|_| AppError::Internal("初始化团队终端房间封装密钥失败".into()))?
        .encrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: room_key,
                aad: &room_wrap_aad(workspace_id, room_id, key_epoch, device_id),
            },
        )
        .map_err(|_| AppError::Internal("封装团队终端房间密钥失败".into()))
}

fn unwrap_room_key(
    shared_secret: &[u8; 32],
    invitation: &TeamTerminalInvitation,
    wrapped: &[u8],
    nonce: &[u8; 12],
) -> AppResult<[u8; 32]> {
    let wrapping_key = derive_room_wrapping_key(
        shared_secret,
        &invitation.workspace_id,
        &invitation.room_id,
        invitation.key_epoch,
        &invitation.recipient_device_id,
    )?;
    let plaintext = Aes256Gcm::new_from_slice(&wrapping_key[..])
        .map_err(|_| AppError::Internal("初始化团队终端房间封装密钥失败".into()))?
        .decrypt(
            Nonce::from_slice(nonce),
            Payload {
                msg: wrapped,
                aad: &room_wrap_aad(
                    &invitation.workspace_id,
                    &invitation.room_id,
                    invitation.key_epoch,
                    &invitation.recipient_device_id,
                ),
            },
        )
        .map_err(|_| AppError::Authentication("当前设备无法解封团队终端房间密钥".into()))?;
    plaintext
        .try_into()
        .map_err(|_| AppError::Validation("团队终端房间密钥长度无效".into()))
}

fn invitation_signing_payload(invitation: &TeamTerminalInvitation) -> AppResult<Vec<u8>> {
    let mut unsigned = invitation.clone();
    unsigned.signature = None;
    serde_jcs::to_vec(&unsigned)
        .map_err(|error| AppError::Internal(format!("规范化团队终端邀请失败：{error}")))
}

fn frame_aad(frame: &TeamTerminalEncryptedFrame) -> AppResult<Vec<u8>> {
    let mut metadata = frame.clone();
    metadata.ciphertext.clear();
    metadata.signature = None;
    serde_jcs::to_vec(&metadata)
        .map_err(|error| AppError::Internal(format!("规范化团队终端帧 AAD 失败：{error}")))
}

fn frame_signing_payload(frame: &TeamTerminalEncryptedFrame) -> AppResult<Vec<u8>> {
    let mut unsigned = frame.clone();
    unsigned.signature = None;
    serde_jcs::to_vec(&unsigned)
        .map_err(|error| AppError::Internal(format!("规范化团队终端帧签名失败：{error}")))
}

fn decode_nonce(value: &str, label: &str) -> AppResult<[u8; 12]> {
    URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| AppError::Validation(format!("{label} nonce Base64URL 无效")))?
        .try_into()
        .map_err(|_| AppError::Validation(format!("{label} nonce 必须为 12 字节")))
}

fn decode_signature(value: Option<&str>, label: &str) -> AppResult<Signature> {
    let encoded = value
        .and_then(|value| value.strip_prefix("ed25519:"))
        .ok_or_else(|| AppError::Validation(format!("{label}缺少 Ed25519 签名")))?;
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| AppError::Validation(format!("{label}签名 Base64URL 无效")))?;
    Signature::from_slice(&bytes).map_err(|_| AppError::Validation(format!("{label}签名长度无效")))
}

fn verify_invitation_signature(
    invitation: &TeamTerminalInvitation,
    host: &TeamDevice,
) -> AppResult<()> {
    let (_, signing_public) = team_share::validated_device_keys(host)?;
    let verifying_key = VerifyingKey::from_bytes(&signing_public)
        .map_err(|_| AppError::Validation("团队终端主持设备签名公钥无效".into()))?;
    verifying_key
        .verify(
            &invitation_signing_payload(invitation)?,
            &decode_signature(invitation.signature.as_deref(), "团队终端邀请")?,
        )
        .map_err(|_| AppError::Authentication("团队终端邀请签名验证失败".into()))
}

fn verify_frame_signature(
    frame: &TeamTerminalEncryptedFrame,
    sender: &TeamDevice,
) -> AppResult<()> {
    let (_, signing_public) = team_share::validated_device_keys(sender)?;
    let verifying_key = VerifyingKey::from_bytes(&signing_public)
        .map_err(|_| AppError::Validation("团队终端发送设备签名公钥无效".into()))?;
    verifying_key
        .verify(
            &frame_signing_payload(frame)?,
            &decode_signature(frame.signature.as_deref(), "团队终端帧")?,
        )
        .map_err(|_| AppError::Authentication("团队终端帧签名验证失败".into()))
}

#[allow(clippy::too_many_arguments)]
fn encrypt_frame(
    room_key: &[u8; 32],
    workspace_id: &str,
    room_id: &str,
    key_epoch: i64,
    sender_member_id: &str,
    sender_device_id: &str,
    direction: &str,
    kind: &str,
    sequence: u64,
    lease_id: Option<String>,
    lease_generation: u64,
    plaintext: &[u8],
) -> AppResult<TeamTerminalEncryptedFrame> {
    if plaintext.is_empty() || plaintext.len() > MAX_FRAME_BYTES {
        return Err(AppError::Validation(
            "团队终端明文帧必须为 1 字节至 64 KB".into(),
        ));
    }
    let mut nonce = [0_u8; 12];
    OsRng.fill_bytes(&mut nonce);
    let mut frame = TeamTerminalEncryptedFrame {
        schema_version: 1,
        workspace_id: workspace_id.into(),
        room_id: room_id.into(),
        key_epoch,
        sender_member_id: sender_member_id.into(),
        sender_device_id: sender_device_id.into(),
        direction: direction.into(),
        kind: kind.into(),
        sequence,
        lease_id,
        lease_generation,
        nonce: URL_SAFE_NO_PAD.encode(nonce),
        ciphertext: String::new(),
        signature: None,
    };
    let ciphertext = Aes256Gcm::new_from_slice(room_key)
        .map_err(|_| AppError::Internal("初始化团队终端内容密钥失败".into()))?
        .encrypt(
            Nonce::from_slice(&nonce),
            Payload {
                msg: plaintext,
                aad: &frame_aad(&frame)?,
            },
        )
        .map_err(|_| AppError::Internal("加密团队终端帧失败".into()))?;
    frame.ciphertext = URL_SAFE_NO_PAD.encode(ciphertext);
    let signing_secret = Zeroizing::new(team_share::load_private_key(sender_device_id, "ed25519")?);
    let signing_key = SigningKey::from_bytes(&signing_secret);
    frame.signature = Some(format!(
        "ed25519:{}",
        URL_SAFE_NO_PAD.encode(signing_key.sign(&frame_signing_payload(&frame)?).to_bytes())
    ));
    Ok(frame)
}

fn decrypt_frame(room_key: &[u8; 32], frame: &TeamTerminalEncryptedFrame) -> AppResult<Vec<u8>> {
    if serde_jcs::to_vec(frame)
        .map_err(|error| AppError::Validation(format!("团队终端帧无效：{error}")))?
        .len()
        > MAX_ENVELOPE_BYTES
    {
        return Err(AppError::Validation("团队终端密文帧超过 128 KB".into()));
    }
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&frame.ciphertext)
        .map_err(|_| AppError::Validation("团队终端密文 Base64URL 无效".into()))?;
    if ciphertext.len() <= 16 || ciphertext.len() > MAX_FRAME_BYTES + 16 {
        return Err(AppError::Validation("团队终端密文长度无效".into()));
    }
    Aes256Gcm::new_from_slice(room_key)
        .map_err(|_| AppError::Internal("初始化团队终端内容密钥失败".into()))?
        .decrypt(
            Nonce::from_slice(&decode_nonce(&frame.nonce, "团队终端帧")?),
            Payload {
                msg: &ciphertext,
                aad: &frame_aad(frame)?,
            },
        )
        .map_err(|_| AppError::Authentication("团队终端密文或 AAD 验证失败".into()))
}

impl CollaborationManager {
    pub async fn start(
        &self,
        db: &Database,
        workspace_id: &str,
        terminal_session_id: &str,
    ) -> AppResult<TeamTerminalRoom> {
        if terminal_session_id.is_empty() || terminal_session_id.len() > 128 {
            return Err(AppError::Validation("终端会话 ID 无效".into()));
        }
        let authorization = team::authorize(db, workspace_id, "shareManage").await?;
        let device_id = authorization
            .local_device_id
            .ok_or_else(|| AppError::Unavailable("开始协作前必须创建本机团队设备身份".into()))?;
        let (member_id, role) = device_member_role(db, workspace_id, &device_id).await?;
        if member_id != authorization.member_id {
            return Err(AppError::PermissionDenied("本机设备不属于当前成员".into()));
        }
        let mut rooms = self.rooms.lock();
        rooms.retain(|_, room| room.status == "active");
        if rooms.len() >= MAX_ROOMS {
            return Err(AppError::Validation(
                "最多同时打开 16 个团队终端房间".into(),
            ));
        }
        if rooms
            .values()
            .any(|room| room.terminal_session_id == terminal_session_id && room.status == "active")
        {
            return Err(AppError::Validation("该终端已经在团队房间中".into()));
        }
        let room_id = uuid::Uuid::new_v4().to_string();
        let joined_at = Utc::now().to_rfc3339();
        let participant = TeamTerminalParticipant {
            member_id: member_id.clone(),
            device_id: device_id.clone(),
            role,
            joined_at: joined_at.clone(),
        };
        let mut participants = HashMap::new();
        participants.insert(
            device_id.clone(),
            ParticipantState {
                participant,
                next_input_sequence: 1,
            },
        );
        let mut room_key = Zeroizing::new([0_u8; 32]);
        OsRng.fill_bytes(&mut *room_key);
        let mut room = RoomState {
            id: room_id.clone(),
            workspace_id: workspace_id.into(),
            terminal_session_id: terminal_session_id.into(),
            host_member_id: member_id,
            host_device_id: device_id,
            key_epoch: authorization.key_epoch,
            status: "active".into(),
            participants,
            control_lease: None,
            next_output_sequence: 1,
            lease_generation: 0,
            room_key,
            replay: VecDeque::new(),
            replay_bytes: 0,
            encrypted_transport_enabled: false,
            created_at: joined_at,
        };
        let snapshot = room.snapshot();
        rooms.insert(room_id, room);
        Ok(snapshot)
    }

    pub async fn join(
        &self,
        db: &Database,
        room_id: &str,
        device_id: &str,
    ) -> AppResult<TeamTerminalRoom> {
        let (workspace_id, status) = {
            let rooms = self.rooms.lock();
            let room = rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
            (room.workspace_id.clone(), room.status.clone())
        };
        if status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        let host_authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        {
            let rooms = self.rooms.lock();
            let room = rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
            validate_host(room, &host_authorization)?;
        }
        let (member_id, role) = device_member_role(db, &workspace_id, device_id).await?;
        team::require_permission(&role, "terminalView")?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &host_authorization)?;
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        if !room.participants.contains_key(device_id) && room.participants.len() >= MAX_PARTICIPANTS
        {
            return Err(AppError::Validation("团队终端最多 64 名参与设备".into()));
        }
        if let Some(participant) = room.participants.get_mut(device_id) {
            if participant.participant.member_id != member_id {
                return Err(AppError::PermissionDenied(
                    "团队终端设备成员归属在重连期间发生变化".into(),
                ));
            }
            participant.participant.role = role;
        } else {
            room.participants.insert(
                device_id.into(),
                ParticipantState {
                    participant: TeamTerminalParticipant {
                        member_id,
                        device_id: device_id.into(),
                        role,
                        joined_at: Utc::now().to_rfc3339(),
                    },
                    next_input_sequence: 1,
                },
            );
        }
        Ok(room.snapshot())
    }

    pub async fn create_invitation(
        &self,
        db: &Database,
        room_id: &str,
        recipient_device_id: &str,
    ) -> AppResult<TeamTerminalInvitation> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let (recipient_member_id, recipient_role) =
            device_member_role(db, &workspace_id, recipient_device_id).await?;
        team::require_permission(&recipient_role, "terminalView")?;
        let recipient =
            active_device(db, &workspace_id, recipient_device_id, &recipient_member_id).await?;
        let host_device_id = authorization
            .local_device_id
            .as_deref()
            .ok_or_else(|| AppError::Unavailable("主持设备身份不可用".into()))?;
        let host =
            active_device(db, &workspace_id, host_device_id, &authorization.member_id).await?;
        let (recipient_public, _) = team_share::validated_device_keys(&recipient)?;

        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        if !room.participants.contains_key(recipient_device_id)
            && room.participants.len() >= MAX_PARTICIPANTS
        {
            return Err(AppError::Validation("团队终端最多 64 名参与设备".into()));
        }
        if let Some(participant) = room.participants.get_mut(recipient_device_id) {
            if participant.participant.member_id != recipient_member_id {
                return Err(AppError::PermissionDenied(
                    "团队终端设备成员归属在邀请期间发生变化".into(),
                ));
            }
            participant.participant.role = recipient_role;
        } else {
            room.participants.insert(
                recipient_device_id.into(),
                ParticipantState {
                    participant: TeamTerminalParticipant {
                        member_id: recipient_member_id.clone(),
                        device_id: recipient_device_id.into(),
                        role: recipient_role,
                        joined_at: Utc::now().to_rfc3339(),
                    },
                    next_input_sequence: 1,
                },
            );
        }

        let mut ephemeral_secret_bytes = Zeroizing::new([0_u8; 32]);
        let mut key_nonce = [0_u8; 12];
        OsRng.fill_bytes(&mut *ephemeral_secret_bytes);
        OsRng.fill_bytes(&mut key_nonce);
        let ephemeral_secret = StaticSecret::from(*ephemeral_secret_bytes);
        let ephemeral_public = X25519PublicKey::from(&ephemeral_secret).to_bytes();
        let shared = ephemeral_secret.diffie_hellman(&X25519PublicKey::from(recipient_public));
        if !shared.was_contributory() {
            return Err(AppError::Validation(
                "团队终端接收设备 X25519 公钥不可用".into(),
            ));
        }
        let wrapped_room_key = wrap_room_key(
            shared.as_bytes(),
            &room.workspace_id,
            &room.id,
            room.key_epoch,
            recipient_device_id,
            &room.room_key,
            &key_nonce,
        )?;
        let created_at = Utc::now();
        let mut invitation = TeamTerminalInvitation {
            schema_version: 1,
            room_id: room.id.clone(),
            workspace_id: room.workspace_id.clone(),
            key_epoch: room.key_epoch,
            host_member_id: room.host_member_id.clone(),
            host_device_id: room.host_device_id.clone(),
            recipient_member_id,
            recipient_device_id: recipient_device_id.into(),
            ephemeral_public_key: team_share::encode_key("x25519", &ephemeral_public),
            key_nonce: URL_SAFE_NO_PAD.encode(key_nonce),
            wrapped_room_key: URL_SAFE_NO_PAD.encode(wrapped_room_key),
            replay_from_sequence: room.replay_from_sequence(),
            next_input_sequence: room
                .participants
                .get(recipient_device_id)
                .map(|participant| participant.next_input_sequence)
                .unwrap_or(1),
            created_at: created_at.to_rfc3339(),
            expires_at: (created_at + ChronoDuration::from_std(INVITATION_TTL).unwrap())
                .to_rfc3339(),
            signature: None,
        };
        let signing_secret = Zeroizing::new(team_share::load_private_key(&host.id, "ed25519")?);
        let signing_key = SigningKey::from_bytes(&signing_secret);
        invitation.signature = Some(format!(
            "ed25519:{}",
            URL_SAFE_NO_PAD.encode(
                signing_key
                    .sign(&invitation_signing_payload(&invitation)?)
                    .to_bytes()
            )
        ));
        room.encrypted_transport_enabled = true;
        Ok(invitation)
    }

    pub async fn accept_invitation(
        &self,
        db: &Database,
        invitation: TeamTerminalInvitation,
    ) -> AppResult<TeamTerminalClientRoom> {
        if serde_jcs::to_vec(&invitation)
            .map_err(|error| AppError::Validation(format!("团队终端邀请无效：{error}")))?
            .len()
            > MAX_ENVELOPE_BYTES
            || invitation.schema_version != 1
            || uuid::Uuid::parse_str(&invitation.room_id).is_err()
            || uuid::Uuid::parse_str(&invitation.workspace_id).is_err()
            || invitation.key_epoch < 1
            || invitation.next_input_sequence == 0
        {
            return Err(AppError::Validation(
                "团队终端邀请版本、ID、epoch 或大小无效".into(),
            ));
        }
        let created_at = DateTime::parse_from_rfc3339(&invitation.created_at)
            .map_err(|_| AppError::Validation("团队终端邀请创建时间无效".into()))?
            .with_timezone(&Utc);
        let expires_at = DateTime::parse_from_rfc3339(&invitation.expires_at)
            .map_err(|_| AppError::Validation("团队终端邀请过期时间无效".into()))?
            .with_timezone(&Utc);
        let now = Utc::now();
        if expires_at <= now
            || created_at > now + ChronoDuration::minutes(1)
            || expires_at <= created_at
            || expires_at - created_at > ChronoDuration::minutes(5)
        {
            return Err(AppError::Unavailable(
                "团队终端邀请已过期或时间范围无效".into(),
            ));
        }
        let authorization = team::authorize(db, &invitation.workspace_id, "terminalView").await?;
        let local_device_id = authorization
            .local_device_id
            .as_deref()
            .ok_or_else(|| AppError::Unavailable("接受邀请前必须创建本机团队设备身份".into()))?;
        if authorization.member_id != invitation.recipient_member_id
            || local_device_id != invitation.recipient_device_id
            || authorization.key_epoch != invitation.key_epoch
        {
            return Err(AppError::PermissionDenied(
                "团队终端邀请不属于当前成员、设备或密钥 epoch".into(),
            ));
        }
        let local_device = active_device(
            db,
            &invitation.workspace_id,
            local_device_id,
            &authorization.member_id,
        )
        .await?;
        let host = active_device(
            db,
            &invitation.workspace_id,
            &invitation.host_device_id,
            &invitation.host_member_id,
        )
        .await?;
        verify_invitation_signature(&invitation, &host)?;
        let ephemeral_public = X25519PublicKey::from(team_share::decode_key(
            &invitation.ephemeral_public_key,
            "x25519",
        )?);
        let local_secret_bytes =
            Zeroizing::new(team_share::load_private_key(&local_device.id, "x25519")?);
        let local_secret = StaticSecret::from(*local_secret_bytes);
        let shared = local_secret.diffie_hellman(&ephemeral_public);
        if !shared.was_contributory() {
            return Err(AppError::Authentication(
                "团队终端邀请 X25519 共享密钥无效".into(),
            ));
        }
        let wrapped = URL_SAFE_NO_PAD
            .decode(&invitation.wrapped_room_key)
            .map_err(|_| AppError::Validation("团队终端房间密钥 Base64URL 无效".into()))?;
        if wrapped.len() != 48 {
            return Err(AppError::Validation("团队终端房间密钥密文长度无效".into()));
        }
        let room_key = Zeroizing::new(unwrap_room_key(
            shared.as_bytes(),
            &invitation,
            &wrapped,
            &decode_nonce(&invitation.key_nonce, "团队终端邀请")?,
        )?);
        let next_output_sequence = invitation
            .replay_from_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Validation("团队终端邀请重放序号无效".into()))?;
        let mut client_rooms = self.client_rooms.lock();
        if client_rooms.contains_key(&invitation.room_id) {
            return Err(AppError::Validation(
                "该团队终端邀请已接受，不能回滚客户端序号".into(),
            ));
        }
        if client_rooms.len() >= MAX_ROOMS {
            return Err(AppError::Validation(
                "最多同时加入 16 个团队终端房间".into(),
            ));
        }
        let state = ClientRoomState {
            workspace_id: invitation.workspace_id.clone(),
            host_member_id: invitation.host_member_id,
            host_device_id: invitation.host_device_id,
            local_member_id: invitation.recipient_member_id,
            local_device_id: invitation.recipient_device_id,
            key_epoch: invitation.key_epoch,
            room_key,
            next_output_sequence,
            next_input_sequence: invitation.next_input_sequence,
            status: "active".into(),
        };
        let snapshot = state.snapshot(&invitation.room_id);
        client_rooms.insert(invitation.room_id, state);
        Ok(snapshot)
    }

    pub async fn publish_encrypted_output(
        &self,
        db: &Database,
        room_id: &str,
        bytes: &[u8],
    ) -> AppResult<TeamTerminalEncryptedFrame> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        let sequence = room.next_output_sequence;
        let frame = encrypt_frame(
            &room.room_key,
            &room.workspace_id,
            &room.id,
            room.key_epoch,
            &room.host_member_id,
            &room.host_device_id,
            "output",
            "terminal",
            sequence,
            None,
            0,
            bytes,
        )?;
        room.next_output_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("团队终端输出序号已耗尽".into()))?;
        let encoded_bytes = serde_jcs::to_vec(&frame)
            .map_err(|error| AppError::Internal(format!("计算团队终端帧大小失败：{error}")))?
            .len();
        room.replay_bytes = room.replay_bytes.saturating_add(encoded_bytes);
        room.replay.push_back(StoredFrame {
            frame: frame.clone(),
            bytes: encoded_bytes,
            created_at: Instant::now(),
        });
        room.encrypted_transport_enabled = true;
        room.prune_replay();
        Ok(frame)
    }

    pub async fn replay_output(
        &self,
        db: &Database,
        room_id: &str,
        recipient_device_id: &str,
        after_sequence: u64,
    ) -> AppResult<Vec<TeamTerminalEncryptedFrame>> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let (member_id, role) = device_member_role(db, &workspace_id, recipient_device_id).await?;
        team::require_permission(&role, "terminalView")?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        if room.status != "active"
            || room
                .participants
                .get(recipient_device_id)
                .is_none_or(|participant| participant.participant.member_id != member_id)
        {
            return Err(AppError::PermissionDenied(
                "设备未加入活动团队终端房间".into(),
            ));
        }
        room.prune_replay();
        let latest = room.next_output_sequence.saturating_sub(1);
        if after_sequence > latest {
            return Err(AppError::Validation(
                "团队终端客户端确认序号超过主持端最新输出".into(),
            ));
        }
        if after_sequence == latest {
            return Ok(Vec::new());
        }
        let earliest = room
            .replay
            .front()
            .map(|value| value.frame.sequence)
            .ok_or_else(|| AppError::Unavailable("团队终端断线重放窗口已过期".into()))?;
        if after_sequence.saturating_add(1) < earliest {
            return Err(AppError::Unavailable(
                "团队终端断线超过 5 分钟或 512 帧重放窗口，请重新加入".into(),
            ));
        }
        Ok(room
            .replay
            .iter()
            .filter(|value| value.frame.sequence > after_sequence)
            .map(|value| value.frame.clone())
            .collect())
    }

    pub async fn publish_output(
        &self,
        db: &Database,
        room_id: &str,
        bytes: &[u8],
    ) -> AppResult<TeamTerminalFrame> {
        if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
            return Err(AppError::Validation(
                "团队终端输出帧必须为 1 字节至 64 KB".into(),
            ));
        }
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        if room.encrypted_transport_enabled {
            return Err(AppError::PermissionDenied(
                "团队终端房间启用端到端加密后不能继续发布明文帧".into(),
            ));
        }
        let sequence = room.next_output_sequence;
        room.next_output_sequence = room
            .next_output_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("团队终端输出序号已耗尽".into()))?;
        Ok(TeamTerminalFrame {
            room_id: room_id.into(),
            sequence,
            kind: "output".into(),
            data_base64: STANDARD.encode(bytes),
        })
    }

    pub async fn decrypt_output(
        &self,
        db: &Database,
        frame: TeamTerminalEncryptedFrame,
    ) -> AppResult<TeamTerminalFrame> {
        let (
            workspace_id,
            key_epoch,
            host_member_id,
            host_device_id,
            local_member_id,
            local_device_id,
        ) = {
            let client_rooms = self.client_rooms.lock();
            let room = client_rooms.get(&frame.room_id).ok_or_else(|| {
                AppError::NotFound(format!("团队终端客户端房间 {}", frame.room_id))
            })?;
            (
                room.workspace_id.clone(),
                room.key_epoch,
                room.host_member_id.clone(),
                room.host_device_id.clone(),
                room.local_member_id.clone(),
                room.local_device_id.clone(),
            )
        };
        if frame.schema_version != 1
            || frame.workspace_id != workspace_id
            || frame.key_epoch != key_epoch
            || frame.sender_member_id != host_member_id
            || frame.sender_device_id != host_device_id
            || frame.direction != "output"
            || frame.kind != "terminal"
            || frame.lease_id.is_some()
            || frame.lease_generation != 0
            || frame.sequence == 0
        {
            return Err(AppError::Validation(
                "团队终端输出帧路由、方向、发送者、epoch 或租约元数据无效".into(),
            ));
        }
        let authorization = team::authorize(db, &workspace_id, "terminalView").await?;
        if authorization.member_id != local_member_id
            || authorization.local_device_id.as_deref() != Some(local_device_id.as_str())
            || authorization.key_epoch != key_epoch
        {
            return Err(AppError::PermissionDenied(
                "当前团队终端客户端成员、设备或密钥 epoch 已失效".into(),
            ));
        }
        let host = active_device(db, &workspace_id, &host_device_id, &host_member_id).await?;
        verify_frame_signature(&frame, &host)?;
        let mut client_rooms = self.client_rooms.lock();
        let room = client_rooms
            .get_mut(&frame.room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端客户端房间 {}", frame.room_id)))?;
        if room.status != "active" || frame.sequence != room.next_output_sequence {
            return Err(AppError::Validation(format!(
                "团队终端输出序号无效：期望 {}，收到 {}",
                room.next_output_sequence, frame.sequence
            )));
        }
        let plaintext = decrypt_frame(&room.room_key, &frame)?;
        room.next_output_sequence = room
            .next_output_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("团队终端输出序号已耗尽".into()))?;
        Ok(TeamTerminalFrame {
            room_id: frame.room_id,
            sequence: frame.sequence,
            kind: "output".into(),
            data_base64: STANDARD.encode(plaintext),
        })
    }

    pub async fn encrypt_input(
        &self,
        db: &Database,
        room_id: &str,
        lease_id: &str,
        lease_generation: u64,
        bytes: &[u8],
    ) -> AppResult<TeamTerminalEncryptedFrame> {
        if uuid::Uuid::parse_str(lease_id).is_err() || lease_generation == 0 {
            return Err(AppError::Validation(
                "团队终端控制租约 ID 或 generation 无效".into(),
            ));
        }
        std::str::from_utf8(bytes)
            .map_err(|_| AppError::Validation("团队终端输入必须是 UTF-8".into()))?;
        let (workspace_id, key_epoch, local_member_id, local_device_id) = {
            let client_rooms = self.client_rooms.lock();
            let room = client_rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端客户端房间 {room_id}")))?;
            (
                room.workspace_id.clone(),
                room.key_epoch,
                room.local_member_id.clone(),
                room.local_device_id.clone(),
            )
        };
        let authorization = team::authorize(db, &workspace_id, "terminalControl").await?;
        if authorization.member_id != local_member_id
            || authorization.local_device_id.as_deref() != Some(local_device_id.as_str())
            || authorization.key_epoch != key_epoch
        {
            return Err(AppError::PermissionDenied(
                "当前团队终端控制成员、设备或密钥 epoch 已失效".into(),
            ));
        }
        let mut client_rooms = self.client_rooms.lock();
        let room = client_rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端客户端房间 {room_id}")))?;
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端客户端房间已关闭".into()));
        }
        let sequence = room.next_input_sequence;
        let frame = encrypt_frame(
            &room.room_key,
            &room.workspace_id,
            room_id,
            room.key_epoch,
            &room.local_member_id,
            &room.local_device_id,
            "input",
            "terminal",
            sequence,
            Some(lease_id.into()),
            lease_generation,
            bytes,
        )?;
        room.next_input_sequence = sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("团队终端输入序号已耗尽".into()))?;
        Ok(frame)
    }

    pub async fn receive_encrypted_input(
        &self,
        db: &Database,
        frame: TeamTerminalEncryptedFrame,
    ) -> AppResult<(String, String)> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(&frame.room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {}", frame.room_id)))?
                .workspace_id
                .clone()
        };
        if frame.schema_version != 1
            || frame.workspace_id != workspace_id
            || frame.direction != "input"
            || frame.kind != "terminal"
            || frame.sequence == 0
            || frame.lease_generation == 0
            || frame
                .lease_id
                .as_deref()
                .is_none_or(|value| uuid::Uuid::parse_str(value).is_err())
        {
            return Err(AppError::Validation(
                "团队终端输入帧路由、方向、序号或租约元数据无效".into(),
            ));
        }
        let (member_id, role) =
            device_member_role(db, &workspace_id, &frame.sender_device_id).await?;
        if member_id != frame.sender_member_id {
            return Err(AppError::PermissionDenied(
                "团队终端输入发送设备不属于声明成员".into(),
            ));
        }
        team::require_permission(&role, "terminalControl")?;
        let sender = active_device(
            db,
            &workspace_id,
            &frame.sender_device_id,
            &frame.sender_member_id,
        )
        .await?;
        verify_frame_signature(&frame, &sender)?;

        let plaintext = {
            let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
            let mut rooms = self.rooms.lock();
            let room = rooms
                .get_mut(&frame.room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {}", frame.room_id)))?;
            validate_host(room, &authorization)?;
            room.expire_lease();
            let lease = room
                .control_lease
                .as_ref()
                .ok_or_else(|| AppError::PermissionDenied("当前没有有效控制租约".into()))?;
            if room.status != "active"
                || room.key_epoch != frame.key_epoch
                || room
                    .participants
                    .get(&frame.sender_device_id)
                    .is_none_or(|participant| {
                        participant.participant.member_id != frame.sender_member_id
                            || participant.next_input_sequence != frame.sequence
                    })
                || lease.lease.id != frame.lease_id.as_deref().unwrap_or_default()
                || lease.lease.device_id != frame.sender_device_id
                || lease.lease.member_id != frame.sender_member_id
                || lease.lease.generation != frame.lease_generation
            {
                return Err(AppError::PermissionDenied(
                    "团队终端输入参与者、序号、epoch 或控制租约已失效".into(),
                ));
            }
            decrypt_frame(&room.room_key, &frame)?
        };
        let data = String::from_utf8(plaintext)
            .map_err(|_| AppError::Validation("团队终端输入必须是 UTF-8".into()))?;
        let encoded = STANDARD.encode(data.as_bytes());
        self.receive_input(
            db,
            &frame.room_id,
            &frame.sender_device_id,
            frame.lease_id.as_deref().unwrap_or_default(),
            frame.sequence,
            &encoded,
        )
        .await
    }

    pub fn client_status(&self, room_id: &str) -> AppResult<TeamTerminalClientRoom> {
        self.client_rooms
            .lock()
            .get(room_id)
            .map(|room| room.snapshot(room_id))
            .ok_or_else(|| AppError::NotFound(format!("团队终端客户端房间 {room_id}")))
    }

    pub fn close_client(&self, room_id: &str) -> AppResult<TeamTerminalClientRoom> {
        let mut room = self
            .client_rooms
            .lock()
            .remove(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端客户端房间 {room_id}")))?;
        room.status = "closed".into();
        Ok(room.snapshot(room_id))
    }

    pub async fn grant_control(
        &self,
        db: &Database,
        room_id: &str,
        device_id: &str,
        duration_seconds: u64,
    ) -> AppResult<TeamTerminalRoom> {
        if !(MIN_LEASE_SECONDS..=MAX_LEASE_SECONDS).contains(&duration_seconds) {
            return Err(AppError::Validation("控制租约必须为 10 至 300 秒".into()));
        }
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let host_authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let (member_id, role) = device_member_role(db, &workspace_id, device_id).await?;
        team::require_permission(&role, "terminalControl")?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &host_authorization)?;
        if room.status != "active" || !room.participants.contains_key(device_id) {
            return Err(AppError::PermissionDenied(
                "设备尚未加入活动团队终端房间".into(),
            ));
        }
        room.lease_generation = room
            .lease_generation
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("控制租约代次已耗尽".into()))?;
        let lease = TeamControlLease {
            id: uuid::Uuid::new_v4().to_string(),
            member_id,
            device_id: device_id.into(),
            expires_at: (Utc::now() + ChronoDuration::seconds(duration_seconds as i64))
                .to_rfc3339(),
            generation: room.lease_generation,
        };
        room.control_lease = Some(LeaseState {
            lease,
            expires_at: Instant::now() + Duration::from_secs(duration_seconds),
        });
        Ok(room.snapshot())
    }

    pub async fn apply_control_lease(
        &self,
        db: &Database,
        room_id: &str,
        lease: TeamControlLease,
    ) -> AppResult<TeamTerminalRoom> {
        if uuid::Uuid::parse_str(&lease.id).is_err() || lease.generation == 0 {
            return Err(AppError::Validation(
                "在线团队终端控制租约 ID 或 generation 无效".into(),
            ));
        }
        let expires_at = DateTime::parse_from_rfc3339(&lease.expires_at)
            .map_err(|_| AppError::Validation("在线团队终端控制租约到期时间无效".into()))?
            .with_timezone(&Utc);
        let remaining = expires_at - Utc::now();
        if remaining <= ChronoDuration::zero() || remaining > ChronoDuration::minutes(5) {
            return Err(AppError::Validation(
                "在线团队终端控制租约已过期或超过 5 分钟".into(),
            ));
        }
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let (member_id, role) = device_member_role(db, &workspace_id, &lease.device_id).await?;
        if member_id != lease.member_id {
            return Err(AppError::PermissionDenied(
                "在线控制租约设备不属于声明成员".into(),
            ));
        }
        team::require_permission(&role, "terminalControl")?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        let participant = room
            .participants
            .get(&lease.device_id)
            .ok_or_else(|| AppError::PermissionDenied("控制设备尚未加入房间".into()))?;
        if participant.participant.member_id != lease.member_id {
            return Err(AppError::PermissionDenied(
                "在线控制租约参与设备归属不匹配".into(),
            ));
        }
        if lease.generation < room.lease_generation {
            return Err(AppError::Validation(
                "在线控制租约 generation 已回滚".into(),
            ));
        }
        if lease.generation == room.lease_generation {
            if room
                .control_lease
                .as_ref()
                .is_some_and(|current| current.lease == lease)
            {
                return Ok(room.snapshot());
            }
            return Err(AppError::Validation(
                "在线控制租约 generation 重复但内容不一致".into(),
            ));
        }
        room.lease_generation = lease.generation;
        room.control_lease = Some(LeaseState {
            lease,
            expires_at: Instant::now()
                + remaining
                    .to_std()
                    .map_err(|_| AppError::Validation("在线控制租约时长无效".into()))?,
        });
        Ok(room.snapshot())
    }

    pub async fn remove_participant(
        &self,
        db: &Database,
        room_id: &str,
        device_id: &str,
    ) -> AppResult<TeamTerminalRoom> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        if device_id == room.host_device_id {
            return Err(AppError::Validation("主持设备不能从活动房间移除".into()));
        }
        room.participants.remove(device_id);
        if room
            .control_lease
            .as_ref()
            .is_some_and(|lease| lease.lease.device_id == device_id)
        {
            room.control_lease = None;
        }
        Ok(room.snapshot())
    }

    pub async fn receive_input(
        &self,
        db: &Database,
        room_id: &str,
        device_id: &str,
        lease_id: &str,
        sequence: u64,
        data_base64: &str,
    ) -> AppResult<(String, String)> {
        if data_base64.len() > 128 * 1024 {
            return Err(AppError::Validation("团队终端输入帧编码超过限制".into()));
        }
        let bytes = STANDARD
            .decode(data_base64)
            .map_err(|_| AppError::Validation("团队终端输入帧 Base64 无效".into()))?;
        if bytes.is_empty() || bytes.len() > MAX_FRAME_BYTES {
            return Err(AppError::Validation(
                "团队终端输入帧必须为 1 字节至 64 KB".into(),
            ));
        }
        let data = String::from_utf8(bytes)
            .map_err(|_| AppError::Validation("团队终端输入必须是 UTF-8".into()))?;
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let (member_id, role) = device_member_role(db, &workspace_id, device_id).await?;
        team::require_permission(&role, "terminalControl")?;
        let host_authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &host_authorization)?;
        room.expire_lease();
        if room.status != "active" {
            return Err(AppError::Unavailable("团队终端房间已关闭".into()));
        }
        let lease = room
            .control_lease
            .as_ref()
            .ok_or_else(|| AppError::PermissionDenied("当前没有有效控制租约".into()))?;
        if lease.lease.id != lease_id
            || lease.lease.device_id != device_id
            || lease.lease.member_id != member_id
        {
            return Err(AppError::PermissionDenied(
                "控制租约不属于当前成员设备".into(),
            ));
        }
        let participant = room
            .participants
            .get_mut(device_id)
            .ok_or_else(|| AppError::PermissionDenied("设备未加入团队终端".into()))?;
        if participant.next_input_sequence != sequence {
            return Err(AppError::Validation(format!(
                "团队终端输入序号无效：期望 {}，收到 {sequence}",
                participant.next_input_sequence
            )));
        }
        participant.next_input_sequence = participant
            .next_input_sequence
            .checked_add(1)
            .ok_or_else(|| AppError::Unavailable("团队终端输入序号已耗尽".into()))?;
        Ok((room.terminal_session_id.clone(), data))
    }

    pub async fn revoke_control(
        &self,
        db: &Database,
        room_id: &str,
    ) -> AppResult<TeamTerminalRoom> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        room.control_lease = None;
        Ok(room.snapshot())
    }

    pub fn status(&self, room_id: &str) -> AppResult<TeamTerminalRoom> {
        let mut rooms = self.rooms.lock();
        rooms
            .get_mut(room_id)
            .map(RoomState::snapshot)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))
    }

    pub async fn close(&self, db: &Database, room_id: &str) -> AppResult<TeamTerminalRoom> {
        let workspace_id = {
            let rooms = self.rooms.lock();
            rooms
                .get(room_id)
                .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?
                .workspace_id
                .clone()
        };
        let authorization = team::authorize(db, &workspace_id, "shareManage").await?;
        let mut rooms = self.rooms.lock();
        let room = rooms
            .get_mut(room_id)
            .ok_or_else(|| AppError::NotFound(format!("团队终端房间 {room_id}")))?;
        validate_host(room, &authorization)?;
        room.status = "closed".into();
        room.control_lease = None;
        room.room_key.zeroize();
        room.replay.clear();
        room.replay_bytes = 0;
        Ok(room.snapshot())
    }

    pub fn close_all(&self) {
        let mut rooms = self.rooms.lock();
        for room in rooms.values_mut() {
            room.status = "closed".into();
            room.control_lease = None;
            room.room_key.zeroize();
            room.replay.clear();
            room.replay_bytes = 0;
        }
        self.client_rooms.lock().clear();
    }

    pub fn close_terminal(&self, terminal_session_id: &str) -> Vec<(String, String)> {
        let mut rooms = self.rooms.lock();
        let mut closed = Vec::new();
        for room in rooms.values_mut().filter(|room| {
            room.terminal_session_id == terminal_session_id && room.status == "active"
        }) {
            room.status = "closed".into();
            room.control_lease = None;
            room.room_key.zeroize();
            room.replay.clear();
            room.replay_bytes = 0;
            closed.push((room.workspace_id.clone(), room.id.clone()));
        }
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::team_share;
    use crate::{
        models::{CreateTeamWorkspaceInput, SaveTeamMemberInput},
        team,
    };
    use tempfile::tempdir;

    #[cfg(target_os = "macos")]
    struct KeyCleanup(Vec<String>);

    #[cfg(target_os = "macos")]
    impl Drop for KeyCleanup {
        fn drop(&mut self) {
            for device_id in &self.0 {
                team_share::delete_private_keys(device_id);
            }
        }
    }

    async fn insert_device(
        db: &Database,
        workspace_id: &str,
        member_id: &str,
        is_local: bool,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO team_devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,?,'active',?,?,NULL)")
            .bind(&id).bind(workspace_id).bind(member_id).bind("Test Device").bind(format!("x25519:{}", "A".repeat(43))).bind(format!("ed25519:{}", "B".repeat(43))).bind(format!("sha256:{}", "c".repeat(64))).bind(is_local).bind(&now).bind(&now).execute(&db.pool).await.unwrap();
        if is_local {
            sqlx::query("UPDATE team_workspaces SET local_device_id=? WHERE id=?")
                .bind(&id)
                .bind(workspace_id)
                .execute(&db.pool)
                .await
                .unwrap();
        }
        id
    }

    #[cfg(target_os = "macos")]
    async fn insert_crypto_device(
        db: &Database,
        workspace_id: &str,
        member_id: &str,
        name: &str,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let mut encryption_secret = Zeroizing::new([0_u8; 32]);
        let mut signing_secret = Zeroizing::new([0_u8; 32]);
        OsRng.fill_bytes(&mut *encryption_secret);
        OsRng.fill_bytes(&mut *signing_secret);
        let encryption_public =
            X25519PublicKey::from(&StaticSecret::from(*encryption_secret)).to_bytes();
        let signing_public = SigningKey::from_bytes(&signing_secret)
            .verifying_key()
            .to_bytes();
        team_share::save_private_key(&id, "x25519", &encryption_secret).unwrap();
        team_share::save_private_key(&id, "ed25519", &signing_secret).unwrap();
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO team_devices(id,workspace_id,member_id,name,encryption_public_key,signing_public_key,fingerprint,is_local,status,created_at,updated_at,revoked_at) VALUES(?,?,?,?,?,?,?,0,'active',?,?,NULL)")
            .bind(&id)
            .bind(workspace_id)
            .bind(member_id)
            .bind(name)
            .bind(team_share::encode_key("x25519", &encryption_public))
            .bind(team_share::encode_key("ed25519", &signing_public))
            .bind(team_share::device_fingerprint(
                &encryption_public,
                &signing_public,
            ))
            .bind(&now)
            .bind(&now)
            .execute(&db.pool)
            .await
            .unwrap();
        id
    }

    #[tokio::test]
    async fn frames_leases_replay_and_role_downgrades_are_enforced() {
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let workspace = team::create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Collaboration".into(),
                owner_name: "Alice".into(),
            },
        )
        .await
        .unwrap();
        let host_device = insert_device(&db, &workspace.id, &workspace.local_member_id, true).await;
        let operator = team::save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: None,
                display_name: "Bob".into(),
                role: "operator".into(),
            },
        )
        .await
        .unwrap();
        let operator_device = insert_device(&db, &workspace.id, &operator.id, false).await;
        let manager = CollaborationManager::default();
        let room = manager
            .start(&db, &workspace.id, "ssh-session")
            .await
            .unwrap();
        assert_eq!(room.host_device_id, host_device);
        let joined = manager.join(&db, &room.id, &operator_device).await.unwrap();
        assert_eq!(joined.participants.len(), 2);
        assert_eq!(
            manager
                .publish_output(&db, &room.id, b"one")
                .await
                .unwrap()
                .sequence,
            1
        );
        assert_eq!(
            manager
                .publish_output(&db, &room.id, b"two")
                .await
                .unwrap()
                .sequence,
            2
        );

        let controlled = manager
            .grant_control(&db, &room.id, &operator_device, 30)
            .await
            .unwrap();
        let lease = controlled.control_lease.unwrap();
        let input = STANDARD.encode("ls\n");
        assert_eq!(
            manager
                .receive_input(&db, &room.id, &operator_device, &lease.id, 1, &input)
                .await
                .unwrap(),
            ("ssh-session".into(), "ls\n".into())
        );
        assert!(
            manager
                .receive_input(&db, &room.id, &operator_device, &lease.id, 1, &input)
                .await
                .is_err()
        );

        team::save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: Some(operator.id),
                display_name: "Bob".into(),
                role: "viewer".into(),
            },
        )
        .await
        .unwrap();
        assert!(
            manager
                .receive_input(&db, &room.id, &operator_device, &lease.id, 2, &input)
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn only_one_bounded_control_lease_exists_at_a_time() {
        let directory = tempdir().unwrap();
        let db = Database::open(&directory.path().join("cnshell.sqlite"))
            .await
            .unwrap();
        let workspace = team::create_workspace(
            &db,
            CreateTeamWorkspaceInput {
                name: "Lease".into(),
                owner_name: "Alice".into(),
            },
        )
        .await
        .unwrap();
        insert_device(&db, &workspace.id, &workspace.local_member_id, true).await;
        let operator = team::save_member(
            &db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: None,
                display_name: "Operator".into(),
                role: "operator".into(),
            },
        )
        .await
        .unwrap();
        let device = insert_device(&db, &workspace.id, &operator.id, false).await;
        let manager = CollaborationManager::default();
        let room = manager.start(&db, &workspace.id, "session").await.unwrap();
        manager.join(&db, &room.id, &device).await.unwrap();
        assert!(
            manager
                .grant_control(&db, &room.id, &device, 9)
                .await
                .is_err()
        );
        let first = manager
            .grant_control(&db, &room.id, &device, 10)
            .await
            .unwrap()
            .control_lease
            .unwrap();
        let second = manager
            .grant_control(&db, &room.id, &device, 20)
            .await
            .unwrap()
            .control_lease
            .unwrap();
        assert_ne!(first.id, second.id);
        assert!(second.generation > first.generation);
        manager.revoke_control(&db, &room.id).await.unwrap();
        assert!(manager.status(&room.id).unwrap().control_lease.is_none());
        assert_eq!(manager.close_terminal("session").len(), 1);
        assert_eq!(manager.status(&room.id).unwrap().status, "closed");
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn encrypted_room_two_client_loopback_replays_and_rejects_tampering() {
        let directory = tempdir().unwrap();
        let host_db_path = directory.path().join("host.sqlite");
        let client_db_path = directory.path().join("client.sqlite");
        let host_db = Database::open(&host_db_path).await.unwrap();
        let workspace = team::create_workspace(
            &host_db,
            CreateTeamWorkspaceInput {
                name: "Encrypted room".into(),
                owner_name: "Alice".into(),
            },
        )
        .await
        .unwrap();
        let host_device = team_share::ensure_local_device(&host_db, &workspace.id, "Host Mac")
            .await
            .unwrap();
        let operator = team::save_member(
            &host_db,
            SaveTeamMemberInput {
                workspace_id: workspace.id.clone(),
                member_id: None,
                display_name: "Bob".into(),
                role: "operator".into(),
            },
        )
        .await
        .unwrap();
        let recipient_device =
            insert_crypto_device(&host_db, &workspace.id, &operator.id, "Recipient Mac").await;
        let _key_cleanup = KeyCleanup(vec![host_device.id.clone(), recipient_device.clone()]);

        sqlx::query("VACUUM INTO ?")
            .bind(client_db_path.to_string_lossy().as_ref())
            .execute(&host_db.pool)
            .await
            .unwrap();
        let client_db = Database::open(&client_db_path).await.unwrap();
        sqlx::query("UPDATE team_devices SET is_local=0 WHERE workspace_id=?")
            .bind(&workspace.id)
            .execute(&client_db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE team_devices SET is_local=1 WHERE id=? AND workspace_id=?")
            .bind(&recipient_device)
            .bind(&workspace.id)
            .execute(&client_db.pool)
            .await
            .unwrap();
        sqlx::query("UPDATE team_workspaces SET local_member_id=?,local_device_id=? WHERE id=?")
            .bind(&operator.id)
            .bind(&recipient_device)
            .bind(&workspace.id)
            .execute(&client_db.pool)
            .await
            .unwrap();

        let host = CollaborationManager::default();
        let client = CollaborationManager::default();
        let room = host
            .start(&host_db, &workspace.id, "ssh-session")
            .await
            .unwrap();
        let invitation = host
            .create_invitation(&host_db, &room.id, &recipient_device)
            .await
            .unwrap();
        let mut tampered_invitation = invitation.clone();
        tampered_invitation.replay_from_sequence = 9;
        assert!(
            client
                .accept_invitation(&client_db, tampered_invitation)
                .await
                .is_err()
        );
        let joined = client
            .accept_invitation(&client_db, invitation.clone())
            .await
            .unwrap();
        assert_eq!(joined.next_output_sequence, 1);
        assert!(
            client
                .accept_invitation(&client_db, invitation)
                .await
                .is_err()
        );

        let first = host
            .publish_encrypted_output(&host_db, &room.id, b"one")
            .await
            .unwrap();
        let second = host
            .publish_encrypted_output(&host_db, &room.id, b"two")
            .await
            .unwrap();
        let third = host
            .publish_encrypted_output(&host_db, &room.id, b"three")
            .await
            .unwrap();
        let decoded = client.decrypt_output(&client_db, first).await.unwrap();
        assert_eq!(STANDARD.decode(decoded.data_base64).unwrap(), b"one");
        assert!(client.decrypt_output(&client_db, third).await.is_err());
        let replay = host
            .replay_output(&host_db, &room.id, &recipient_device, 1)
            .await
            .unwrap();
        assert_eq!(replay.len(), 2);
        assert_eq!(replay[0].sequence, second.sequence);
        assert_eq!(replay[0].ciphertext, second.ciphertext);
        for frame in replay {
            client.decrypt_output(&client_db, frame).await.unwrap();
        }
        assert_eq!(
            client.client_status(&room.id).unwrap().next_output_sequence,
            4
        );

        let fourth = host
            .publish_encrypted_output(&host_db, &room.id, b"four")
            .await
            .unwrap();
        let mut tampered = fourth.clone();
        let replacement = if tampered.ciphertext.starts_with('A') {
            "B"
        } else {
            "A"
        };
        tampered.ciphertext.replace_range(..1, replacement);
        assert!(client.decrypt_output(&client_db, tampered).await.is_err());
        let decoded = client.decrypt_output(&client_db, fourth).await.unwrap();
        assert_eq!(STANDARD.decode(decoded.data_base64).unwrap(), b"four");

        let lease = host
            .grant_control(&host_db, &room.id, &recipient_device, 30)
            .await
            .unwrap()
            .control_lease
            .unwrap();
        let input = client
            .encrypt_input(&client_db, &room.id, &lease.id, lease.generation, b"pwd\n")
            .await
            .unwrap();
        assert_eq!(
            host.receive_encrypted_input(&host_db, input.clone())
                .await
                .unwrap(),
            ("ssh-session".into(), "pwd\n".into())
        );
        assert!(host.receive_encrypted_input(&host_db, input).await.is_err());

        assert_eq!(client.close_client(&room.id).unwrap().status, "closed");
        let reconnect_invitation = host
            .create_invitation(&host_db, &room.id, &recipient_device)
            .await
            .unwrap();
        assert_eq!(reconnect_invitation.next_input_sequence, 2);
        let reconnected_client = CollaborationManager::default();
        let reconnected = reconnected_client
            .accept_invitation(&client_db, reconnect_invitation)
            .await
            .unwrap();
        assert_eq!(reconnected.next_input_sequence, 2);
        let second_input = reconnected_client
            .encrypt_input(
                &client_db,
                &room.id,
                &lease.id,
                lease.generation,
                b"whoami\n",
            )
            .await
            .unwrap();
        assert_eq!(second_input.sequence, 2);
        assert_eq!(
            host.receive_encrypted_input(&host_db, second_input)
                .await
                .unwrap(),
            ("ssh-session".into(), "whoami\n".into())
        );

        team_share::revoke_device(&host_db, &workspace.id, &recipient_device)
            .await
            .unwrap();
        assert!(
            host.replay_output(&host_db, &room.id, &recipient_device, 4)
                .await
                .is_err()
        );
        assert_eq!(
            reconnected_client.close_client(&room.id).unwrap().status,
            "closed"
        );
    }
}
