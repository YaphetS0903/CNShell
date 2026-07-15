use crate::{
    error::{RelayError, RelayResult},
    metrics::RelayMetrics,
    models::{
        ClientSocketMessage, ControlLeaseOutput, CreateRoomInput, GrantControlInput, RoomView,
        RouteRoomInvitationInput, RoutedRoomInvitation, ServerSocketMessage,
        TeamTerminalEncryptedFrame, TeamTerminalInvitation, TerminalParticipantOutput,
    },
    store::{DeviceAuth, RelayStore, audit, require_device_permission, validate_uuid},
};
use axum::extract::ws::{Message, WebSocket};
use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use futures_util::{SinkExt, StreamExt};
use parking_lot::Mutex;
use sqlx::Row;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::{broadcast, watch};

const MAX_ENVELOPE_BYTES: usize = 128 * 1024;
const MAX_FRAME_BYTES: usize = 64 * 1024;
const MAX_REPLAY_FRAMES: i64 = 512;
const MAX_REPLAY_BYTES: i64 = 4 * 1024 * 1024;
const REPLAY_MINUTES: i64 = 5;
const MAX_ACTIVE_ROOMS: i64 = 64;
const MAX_ROOM_HISTORY: i64 = 4096;
const MAX_ROOM_PARTICIPANTS: i64 = 64;

#[derive(Clone)]
struct BroadcastFrame {
    json: String,
    direction: String,
    sequence: Option<u64>,
    host_device_id: String,
}

#[derive(Clone)]
pub struct TerminalRelay {
    store: RelayStore,
    hubs: Arc<Mutex<HashMap<String, broadcast::Sender<BroadcastFrame>>>>,
    shutdown: watch::Sender<bool>,
    metrics: RelayMetrics,
}

impl TerminalRelay {
    pub fn new(store: RelayStore, metrics: RelayMetrics) -> Self {
        let (shutdown, _) = watch::channel(false);
        Self {
            store,
            hubs: Arc::new(Mutex::new(HashMap::new())),
            shutdown,
            metrics,
        }
    }

    pub fn shutdown(&self) {
        self.shutdown.send_replace(true);
    }

    pub async fn create_room(
        &self,
        auth: &DeviceAuth,
        input: CreateRoomInput,
    ) -> RelayResult<RoomView> {
        require_device_permission(auth, "shareManage")?;
        validate_uuid(&input.room_id, "房间 ID")?;
        if input.key_epoch != auth.key_epoch {
            return Err(RelayError::PermissionDenied(
                "房间密钥 epoch 不是工作区当前值".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.store.pool.begin().await?;
        sqlx::query("DELETE FROM terminal_rooms WHERE workspace_id=? AND status='closed' AND id NOT IN (SELECT id FROM terminal_rooms WHERE workspace_id=? AND status='closed' ORDER BY closed_at DESC,id DESC LIMIT ?)")
            .bind(&auth.workspace_id)
            .bind(&auth.workspace_id)
            .bind(MAX_ROOM_HISTORY)
            .execute(&mut *transaction)
            .await?;
        let active_rooms: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM terminal_rooms WHERE workspace_id=? AND status='active'",
        )
        .bind(&auth.workspace_id)
        .fetch_one(&mut *transaction)
        .await?;
        if active_rooms >= MAX_ACTIVE_ROOMS {
            return Err(RelayError::Conflict(
                "工作区活动终端房间已经达到 64 个上限".into(),
            ));
        }
        sqlx::query("INSERT INTO terminal_rooms(id,workspace_id,host_member_id,host_device_id,key_epoch,status,next_output_sequence,lease_generation,created_at,closed_at) VALUES(?,?,?,?,?,'active',1,0,?,NULL)")
            .bind(&input.room_id)
            .bind(&auth.workspace_id)
            .bind(&auth.member_id)
            .bind(&auth.device_id)
            .bind(auth.key_epoch)
            .bind(&now)
            .execute(&mut *transaction)
            .await
            .map_err(|error| match error {
                sqlx::Error::Database(database) if database.is_unique_violation() => {
                    RelayError::Conflict("终端房间 ID 已存在".into())
                }
                other => RelayError::Storage(other),
            })?;
        sqlx::query("INSERT INTO room_participants(room_id,member_id,device_id,next_input_sequence,joined_at,removed_at) VALUES(?,?,?,1,?,NULL)")
            .bind(&input.room_id)
            .bind(&auth.member_id)
            .bind(&auth.device_id)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-room-created",
            "terminalRoom",
            &input.room_id,
        )
        .await?;
        transaction.commit().await?;
        Ok(RoomView {
            id: input.room_id,
            workspace_id: auth.workspace_id.clone(),
            host_member_id: auth.member_id.clone(),
            host_device_id: auth.device_id.clone(),
            key_epoch: auth.key_epoch,
            status: "active".into(),
        })
    }

    pub async fn route_invitation(
        &self,
        auth: &DeviceAuth,
        room_id: &str,
        input: RouteRoomInvitationInput,
    ) -> RelayResult<()> {
        let invitation = input.invitation;
        validate_invitation_shape(&invitation)?;
        self.host_room(auth, room_id).await?;
        if invitation.room_id != room_id
            || invitation.workspace_id != auth.workspace_id
            || invitation.key_epoch != auth.key_epoch
            || invitation.host_member_id != auth.member_id
            || invitation.host_device_id != auth.device_id
        {
            return Err(RelayError::Validation(
                "房间邀请路由、主持设备或 epoch 不匹配".into(),
            ));
        }
        let recipient: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM devices d JOIN members m ON m.id=d.member_id AND m.workspace_id=d.workspace_id WHERE d.id=? AND d.workspace_id=? AND d.member_id=? AND d.status='active' AND m.status='active'")
            .bind(&invitation.recipient_device_id)
            .bind(&auth.workspace_id)
            .bind(&invitation.recipient_member_id)
            .fetch_one(&self.store.pool)
            .await?;
        if recipient != 1 {
            return Err(RelayError::PermissionDenied(
                "邀请接收设备或成员已撤销".into(),
            ));
        }
        let signing_key: String = sqlx::query_scalar(
            "SELECT signing_public_key FROM devices WHERE id=? AND status='active'",
        )
        .bind(&auth.device_id)
        .fetch_one(&self.store.pool)
        .await?;
        verify_invitation_signature(&invitation, &signing_key)?;
        let envelope = serde_json::to_string(&invitation).map_err(|_| RelayError::Internal)?;
        let now = Utc::now().to_rfc3339();
        let participant_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM (SELECT device_id FROM room_participants WHERE room_id=? AND removed_at IS NULL UNION SELECT recipient_device_id FROM room_invitations WHERE room_id=? AND accepted_at IS NULL)",
        )
        .bind(room_id)
        .bind(room_id)
        .fetch_one(&self.store.pool)
        .await?;
        let already_invited: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM room_invitations WHERE room_id=? AND recipient_device_id=?",
        )
        .bind(room_id)
        .bind(&invitation.recipient_device_id)
        .fetch_one(&self.store.pool)
        .await?;
        if participant_count >= MAX_ROOM_PARTICIPANTS && already_invited == 0 {
            return Err(RelayError::Conflict(
                "终端房间接收设备已经达到 64 台上限".into(),
            ));
        }
        sqlx::query("INSERT INTO room_invitations(room_id,recipient_device_id,envelope_json,expires_at,accepted_at,created_at) VALUES(?,?,?,?,NULL,?) ON CONFLICT(room_id,recipient_device_id) DO UPDATE SET envelope_json=excluded.envelope_json,expires_at=excluded.expires_at,accepted_at=NULL,created_at=excluded.created_at")
            .bind(room_id)
            .bind(&invitation.recipient_device_id)
            .bind(envelope)
            .bind(&invitation.expires_at)
            .bind(now)
            .execute(&self.store.pool)
            .await?;
        Ok(())
    }

    pub async fn list_invitations(
        &self,
        auth: &DeviceAuth,
    ) -> RelayResult<Vec<RoutedRoomInvitation>> {
        require_device_permission(auth, "terminalView")?;
        let rows = sqlx::query("SELECT i.room_id,i.envelope_json FROM room_invitations i JOIN terminal_rooms r ON r.id=i.room_id WHERE i.recipient_device_id=? AND i.accepted_at IS NULL AND i.expires_at>? AND r.status='active' AND r.workspace_id=? AND r.key_epoch=? ORDER BY i.created_at")
            .bind(&auth.device_id)
            .bind(Utc::now().to_rfc3339())
            .bind(&auth.workspace_id)
            .bind(auth.key_epoch)
            .fetch_all(&self.store.pool)
            .await?;
        rows.into_iter()
            .map(|row| {
                let invitation = serde_json::from_str::<TeamTerminalInvitation>(row.get(1))
                    .map_err(|_| RelayError::Internal)?;
                Ok(RoutedRoomInvitation {
                    room_id: row.get(0),
                    invitation,
                })
            })
            .collect()
    }

    pub async fn join_room(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<RoomView> {
        require_device_permission(auth, "terminalView")?;
        let mut transaction = self.store.pool.begin().await?;
        let row = sqlx::query("SELECT r.workspace_id,r.host_member_id,r.host_device_id,r.key_epoch,r.status,i.envelope_json FROM terminal_rooms r JOIN room_invitations i ON i.room_id=r.id AND i.recipient_device_id=? WHERE r.id=? AND i.accepted_at IS NULL AND i.expires_at>?")
            .bind(&auth.device_id)
            .bind(room_id)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&mut *transaction)
            .await?
            .ok_or_else(|| RelayError::NotFound("活动房间邀请不存在".into()))?;
        let workspace_id: String = row.get(0);
        let host_member_id: String = row.get(1);
        let host_device_id: String = row.get(2);
        let key_epoch: i64 = row.get(3);
        let status: String = row.get(4);
        if workspace_id != auth.workspace_id || key_epoch != auth.key_epoch || status != "active" {
            return Err(RelayError::PermissionDenied(
                "房间工作区或 epoch 已失效".into(),
            ));
        }
        let invitation: TeamTerminalInvitation =
            serde_json::from_str(row.get(5)).map_err(|_| RelayError::Internal)?;
        if invitation.recipient_member_id != auth.member_id
            || invitation.recipient_device_id != auth.device_id
        {
            return Err(RelayError::PermissionDenied(
                "房间邀请不属于当前设备".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        sqlx::query("INSERT INTO room_participants(room_id,member_id,device_id,next_input_sequence,joined_at,removed_at) VALUES(?,?,?,?,?,NULL) ON CONFLICT(room_id,device_id) DO UPDATE SET member_id=excluded.member_id,removed_at=NULL")
            .bind(room_id)
            .bind(&auth.member_id)
            .bind(&auth.device_id)
            .bind(invitation.next_input_sequence as i64)
            .bind(&now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("UPDATE room_invitations SET accepted_at=? WHERE room_id=? AND recipient_device_id=? AND accepted_at IS NULL")
            .bind(&now)
            .bind(room_id)
            .bind(&auth.device_id)
            .execute(&mut *transaction)
            .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-room-joined",
            "terminalRoom",
            room_id,
        )
        .await?;
        transaction.commit().await?;
        let _ = self.broadcast_participants(room_id, &host_device_id).await;
        Ok(RoomView {
            id: room_id.into(),
            workspace_id,
            host_member_id,
            host_device_id,
            key_epoch,
            status,
        })
    }

    pub async fn leave_room(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<()> {
        validate_uuid(room_id, "房间 ID")?;
        let room = self.authorize_room(auth, room_id).await?;
        if room.host_device_id == auth.device_id {
            return Err(RelayError::Validation(
                "主持设备必须关闭房间，不能以参与者身份离开".into(),
            ));
        }
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.store.pool.begin().await?;
        let removed = sqlx::query("UPDATE room_participants SET removed_at=? WHERE room_id=? AND device_id=? AND member_id=? AND removed_at IS NULL")
            .bind(&now)
            .bind(room_id)
            .bind(&auth.device_id)
            .bind(&auth.member_id)
            .execute(&mut *transaction)
            .await?;
        if removed.rows_affected() != 1 {
            return Err(RelayError::NotFound("活动房间参与记录不存在".into()));
        }
        let revoked =
            sqlx::query("DELETE FROM room_control_leases WHERE room_id=? AND device_id=?")
                .bind(room_id)
                .bind(&auth.device_id)
                .execute(&mut *transaction)
                .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-room-left",
            "terminalRoom",
            room_id,
        )
        .await?;
        transaction.commit().await?;
        let _ = self
            .broadcast_participants(room_id, &room.host_device_id)
            .await;
        if revoked.rows_affected() > 0 {
            self.broadcast_control(room_id, &room.host_device_id, None)?;
        }
        Ok(())
    }

    pub async fn grant_control(
        &self,
        auth: &DeviceAuth,
        room_id: &str,
        input: GrantControlInput,
    ) -> RelayResult<ControlLeaseOutput> {
        if !(10..=300).contains(&input.duration_seconds) {
            return Err(RelayError::Validation("控制租约必须为 10 至 300 秒".into()));
        }
        self.host_room(auth, room_id).await?;
        let target = sqlx::query("SELECT p.member_id,m.role FROM room_participants p JOIN devices d ON d.id=p.device_id AND d.member_id=p.member_id JOIN members m ON m.id=p.member_id AND m.workspace_id=d.workspace_id WHERE p.room_id=? AND p.device_id=? AND p.removed_at IS NULL AND d.status='active' AND m.status='active'")
            .bind(room_id)
            .bind(&input.device_id)
            .fetch_optional(&self.store.pool)
            .await?
            .ok_or_else(|| RelayError::NotFound("控制设备未加入房间".into()))?;
        let member_id: String = target.get(0);
        let role: String = target.get(1);
        if !matches!(role.as_str(), "owner" | "admin" | "operator") {
            return Err(RelayError::PermissionDenied(
                "目标成员没有终端控制权限".into(),
            ));
        }
        let mut transaction = self.store.pool.begin().await?;
        let generation: i64 = sqlx::query_scalar("UPDATE terminal_rooms SET lease_generation=lease_generation+1 WHERE id=? AND status='active' RETURNING lease_generation")
            .bind(room_id)
            .fetch_one(&mut *transaction)
            .await?;
        let lease_id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let expires_at = now + ChronoDuration::seconds(input.duration_seconds as i64);
        sqlx::query("INSERT INTO room_control_leases(room_id,lease_id,member_id,device_id,generation,expires_at,created_at) VALUES(?,?,?,?,?,?,?) ON CONFLICT(room_id) DO UPDATE SET lease_id=excluded.lease_id,member_id=excluded.member_id,device_id=excluded.device_id,generation=excluded.generation,expires_at=excluded.expires_at,created_at=excluded.created_at")
            .bind(room_id)
            .bind(&lease_id)
            .bind(&member_id)
            .bind(&input.device_id)
            .bind(generation)
            .bind(expires_at.to_rfc3339())
            .bind(now.to_rfc3339())
            .execute(&mut *transaction)
            .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-control-granted",
            "device",
            &input.device_id,
        )
        .await?;
        transaction.commit().await?;
        let output = ControlLeaseOutput {
            lease_id,
            member_id,
            device_id: input.device_id,
            generation: generation as u64,
            expires_at: expires_at.to_rfc3339(),
        };
        self.broadcast_control(room_id, &auth.device_id, Some(output.clone()))?;
        Ok(output)
    }

    pub async fn revoke_control(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<()> {
        self.host_room(auth, room_id).await?;
        let mut transaction = self.store.pool.begin().await?;
        sqlx::query("DELETE FROM room_control_leases WHERE room_id=?")
            .bind(room_id)
            .execute(&mut *transaction)
            .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-control-revoked",
            "terminalRoom",
            room_id,
        )
        .await?;
        transaction.commit().await?;
        self.broadcast_control(room_id, &auth.device_id, None)?;
        Ok(())
    }

    pub async fn close_room(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<()> {
        self.host_room(auth, room_id).await?;
        let now = Utc::now().to_rfc3339();
        let mut transaction = self.store.pool.begin().await?;
        sqlx::query(
            "UPDATE terminal_rooms SET status='closed',closed_at=? WHERE id=? AND status='active'",
        )
        .bind(&now)
        .bind(room_id)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM room_control_leases WHERE room_id=?")
            .bind(room_id)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM relay_frames WHERE room_id=?")
            .bind(room_id)
            .execute(&mut *transaction)
            .await?;
        audit(
            &mut transaction,
            &auth.workspace_id,
            &auth.member_id,
            "terminal-room-closed",
            "terminalRoom",
            room_id,
        )
        .await?;
        transaction.commit().await?;
        if let Ok(payload) = serde_json::to_string(&ServerSocketMessage::Closed) {
            let _ = self.sender(room_id).send(BroadcastFrame {
                json: payload,
                direction: "control".into(),
                sequence: None,
                host_device_id: auth.device_id.clone(),
            });
        }
        self.hubs.lock().remove(room_id);
        Ok(())
    }

    pub async fn authorize_room(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<RoomView> {
        require_device_permission(auth, "terminalView")?;
        let row = sqlx::query("SELECT r.workspace_id,r.host_member_id,r.host_device_id,r.key_epoch,r.status FROM terminal_rooms r JOIN room_participants p ON p.room_id=r.id AND p.device_id=? AND p.member_id=? AND p.removed_at IS NULL WHERE r.id=?")
            .bind(&auth.device_id)
            .bind(&auth.member_id)
            .bind(room_id)
            .fetch_optional(&self.store.pool)
            .await?
            .ok_or_else(|| RelayError::PermissionDenied("设备未加入终端房间".into()))?;
        let view = RoomView {
            id: room_id.into(),
            workspace_id: row.get(0),
            host_member_id: row.get(1),
            host_device_id: row.get(2),
            key_epoch: row.get(3),
            status: row.get(4),
        };
        if view.workspace_id != auth.workspace_id
            || view.key_epoch != auth.key_epoch
            || view.status != "active"
        {
            return Err(RelayError::PermissionDenied(
                "终端房间、工作区或 epoch 已失效".into(),
            ));
        }
        Ok(view)
    }

    async fn host_room(&self, auth: &DeviceAuth, room_id: &str) -> RelayResult<RoomView> {
        require_device_permission(auth, "shareManage")?;
        let room = self.authorize_room(auth, room_id).await?;
        if room.host_member_id != auth.member_id || room.host_device_id != auth.device_id {
            return Err(RelayError::PermissionDenied(
                "只有房间主持设备可以执行该操作".into(),
            ));
        }
        Ok(room)
    }

    async fn replay(
        &self,
        auth: &DeviceAuth,
        room_id: &str,
        after_sequence: u64,
        up_to_sequence: u64,
    ) -> RelayResult<Vec<TeamTerminalEncryptedFrame>> {
        self.authorize_room(auth, room_id).await?;
        if after_sequence > up_to_sequence {
            return Err(RelayError::Validation(
                "客户端确认序号超过房间最新输出".into(),
            ));
        }
        if after_sequence == up_to_sequence {
            return Ok(Vec::new());
        }
        if up_to_sequence.saturating_sub(after_sequence) > MAX_REPLAY_FRAMES as u64 {
            return Err(RelayError::Unavailable(
                "终端断线重放窗口已过期，请重新加入".into(),
            ));
        }
        let rows = sqlx::query("SELECT sequence,envelope_json FROM relay_frames WHERE room_id=? AND sequence>? AND sequence<=? ORDER BY sequence LIMIT ?")
            .bind(room_id)
            .bind(after_sequence as i64)
            .bind(up_to_sequence as i64)
            .bind(MAX_REPLAY_FRAMES)
            .fetch_all(&self.store.pool)
            .await?;
        let first = rows.first().map(|row| row.get::<i64, _>(0) as u64);
        let last = rows.last().map(|row| row.get::<i64, _>(0) as u64);
        if first != Some(after_sequence.saturating_add(1)) || last != Some(up_to_sequence) {
            return Err(RelayError::Unavailable(
                "终端断线重放窗口已过期，请重新加入".into(),
            ));
        }
        rows.into_iter()
            .map(|row| serde_json::from_str(row.get(1)).map_err(|_| RelayError::Internal))
            .collect()
    }

    pub async fn process_frame(
        &self,
        auth: &DeviceAuth,
        frame: TeamTerminalEncryptedFrame,
    ) -> RelayResult<String> {
        validate_frame_shape(&frame)?;
        if frame.workspace_id != auth.workspace_id
            || frame.sender_member_id != auth.member_id
            || frame.sender_device_id != auth.device_id
            || frame.key_epoch != auth.key_epoch
        {
            return Err(RelayError::PermissionDenied(
                "帧发送者、工作区或 epoch 与设备会话不匹配".into(),
            ));
        }
        let room = self.authorize_room(auth, &frame.room_id).await?;
        let signing_key: String = sqlx::query_scalar(
            "SELECT signing_public_key FROM devices WHERE id=? AND status='active'",
        )
        .bind(&auth.device_id)
        .fetch_one(&self.store.pool)
        .await?;
        verify_frame_signature(&frame, &signing_key)?;
        let envelope = serde_json::to_string(&frame).map_err(|_| RelayError::Internal)?;
        let mut transaction = self.store.pool.begin().await?;
        match frame.direction.as_str() {
            "output" => {
                if room.host_device_id != auth.device_id
                    || frame.kind != "terminal"
                    || frame.lease_id.is_some()
                    || frame.lease_generation != 0
                {
                    return Err(RelayError::PermissionDenied(
                        "只有主持设备可以发送无租约输出帧".into(),
                    ));
                }
                let expected: i64 = sqlx::query_scalar("SELECT next_output_sequence FROM terminal_rooms WHERE id=? AND status='active'")
                    .bind(&frame.room_id)
                    .fetch_one(&mut *transaction)
                    .await?;
                if frame.sequence != expected as u64 {
                    return Err(RelayError::Conflict(format!(
                        "输出序号无效：期望 {expected}，收到 {}",
                        frame.sequence
                    )));
                }
                let now = Utc::now().to_rfc3339();
                sqlx::query("UPDATE terminal_rooms SET next_output_sequence=next_output_sequence+1 WHERE id=? AND next_output_sequence=?")
                    .bind(&frame.room_id)
                    .bind(expected)
                    .execute(&mut *transaction)
                    .await?;
                sqlx::query("INSERT INTO relay_frames(room_id,sequence,envelope_json,encoded_bytes,created_at) VALUES(?,?,?,?,?)")
                    .bind(&frame.room_id)
                    .bind(frame.sequence as i64)
                    .bind(&envelope)
                    .bind(envelope.len() as i64)
                    .bind(&now)
                    .execute(&mut *transaction)
                    .await?;
                prune_replay(&mut transaction, &frame.room_id).await?;
            }
            "input" => {
                require_device_permission(auth, "terminalControl")?;
                if frame.kind != "terminal"
                    || frame.lease_generation == 0
                    || frame
                        .lease_id
                        .as_deref()
                        .is_none_or(|value| uuid::Uuid::parse_str(value).is_err())
                {
                    return Err(RelayError::Validation("输入帧租约元数据无效".into()));
                }
                let participant_sequence: i64 = sqlx::query_scalar("SELECT next_input_sequence FROM room_participants WHERE room_id=? AND device_id=? AND member_id=? AND removed_at IS NULL")
                    .bind(&frame.room_id)
                    .bind(&auth.device_id)
                    .bind(&auth.member_id)
                    .fetch_one(&mut *transaction)
                    .await?;
                if frame.sequence != participant_sequence as u64 {
                    return Err(RelayError::Conflict(format!(
                        "输入序号无效：期望 {participant_sequence}，收到 {}",
                        frame.sequence
                    )));
                }
                let lease: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM room_control_leases WHERE room_id=? AND lease_id=? AND member_id=? AND device_id=? AND generation=? AND expires_at>?")
                    .bind(&frame.room_id)
                    .bind(frame.lease_id.as_deref().unwrap_or_default())
                    .bind(&auth.member_id)
                    .bind(&auth.device_id)
                    .bind(frame.lease_generation as i64)
                    .bind(Utc::now().to_rfc3339())
                    .fetch_one(&mut *transaction)
                    .await?;
                if lease != 1 {
                    return Err(RelayError::PermissionDenied(
                        "控制租约无效、已过期或不属于当前设备".into(),
                    ));
                }
                sqlx::query("UPDATE room_participants SET next_input_sequence=next_input_sequence+1 WHERE room_id=? AND device_id=? AND next_input_sequence=?")
                    .bind(&frame.room_id)
                    .bind(&auth.device_id)
                    .bind(participant_sequence)
                    .execute(&mut *transaction)
                    .await?;
            }
            _ => return Err(RelayError::Validation("终端帧方向无效".into())),
        }
        transaction.commit().await?;
        let socket_json = serde_json::to_string(&ServerSocketMessage::Frame {
            frame: Box::new(frame.clone()),
        })
        .map_err(|_| RelayError::Internal)?;
        let broadcast = BroadcastFrame {
            json: socket_json,
            direction: frame.direction,
            sequence: Some(frame.sequence),
            host_device_id: room.host_device_id,
        };
        let _ = self.sender(&frame.room_id).send(broadcast);
        Ok(envelope)
    }

    fn sender(&self, room_id: &str) -> broadcast::Sender<BroadcastFrame> {
        let mut hubs = self.hubs.lock();
        hubs.entry(room_id.into())
            .or_insert_with(|| broadcast::channel(1024).0)
            .clone()
    }

    fn broadcast_control(
        &self,
        room_id: &str,
        host_device_id: &str,
        lease: Option<ControlLeaseOutput>,
    ) -> RelayResult<()> {
        let payload = serde_json::to_string(&ServerSocketMessage::Control { lease })
            .map_err(|_| RelayError::Internal)?;
        let _ = self.sender(room_id).send(BroadcastFrame {
            json: payload,
            direction: "control".into(),
            sequence: None,
            host_device_id: host_device_id.into(),
        });
        Ok(())
    }

    async fn room_participants(
        &self,
        room_id: &str,
    ) -> RelayResult<Vec<TerminalParticipantOutput>> {
        let rows = sqlx::query("SELECT p.member_id,p.device_id,m.role,p.joined_at FROM room_participants p JOIN terminal_rooms r ON r.id=p.room_id JOIN members m ON m.id=p.member_id AND m.workspace_id=r.workspace_id JOIN devices d ON d.id=p.device_id AND d.member_id=p.member_id AND d.workspace_id=r.workspace_id WHERE p.room_id=? AND p.removed_at IS NULL AND r.status='active' AND m.status='active' AND d.status='active' ORDER BY p.joined_at,p.device_id")
            .bind(room_id)
            .fetch_all(&self.store.pool)
            .await?;
        if rows.len() > MAX_ROOM_PARTICIPANTS as usize {
            return Err(RelayError::Internal);
        }
        Ok(rows
            .into_iter()
            .map(|row| TerminalParticipantOutput {
                member_id: row.get(0),
                device_id: row.get(1),
                role: row.get(2),
                joined_at: row.get(3),
            })
            .collect())
    }

    async fn current_control(&self, room_id: &str) -> RelayResult<Option<ControlLeaseOutput>> {
        let row = sqlx::query("SELECT lease_id,member_id,device_id,generation,expires_at FROM room_control_leases WHERE room_id=? AND expires_at>?")
            .bind(room_id)
            .bind(Utc::now().to_rfc3339())
            .fetch_optional(&self.store.pool)
            .await?;
        Ok(row.map(|row| ControlLeaseOutput {
            lease_id: row.get(0),
            member_id: row.get(1),
            device_id: row.get(2),
            generation: row.get::<i64, _>(3) as u64,
            expires_at: row.get(4),
        }))
    }

    async fn broadcast_participants(&self, room_id: &str, host_device_id: &str) -> RelayResult<()> {
        let participants = self.room_participants(room_id).await?;
        let payload = serde_json::to_string(&ServerSocketMessage::Participants { participants })
            .map_err(|_| RelayError::Internal)?;
        let _ = self.sender(room_id).send(BroadcastFrame {
            json: payload,
            direction: "participants".into(),
            sequence: None,
            host_device_id: host_device_id.into(),
        });
        Ok(())
    }

    pub async fn handle_socket(
        &self,
        socket: WebSocket,
        token: String,
        room_id: String,
        after_sequence: u64,
    ) {
        let mut shutdown = self.shutdown.subscribe();
        if *shutdown.borrow() {
            return;
        }
        let Ok(auth) = self.store.authenticate_device(&token).await else {
            return;
        };
        if self.authorize_room(&auth, &room_id).await.is_err() {
            return;
        }
        let _active_socket = self.metrics.begin_websocket();
        let sender = self.sender(&room_id);
        let mut receiver = sender.subscribe();
        let (mut sink, mut stream) = socket.split();
        let latest_output_sequence: i64 = match sqlx::query_scalar(
            "SELECT next_output_sequence-1 FROM terminal_rooms WHERE id=? AND status='active'",
        )
        .bind(&room_id)
        .fetch_one(&self.store.pool)
        .await
        {
            Ok(value) => value,
            Err(_) => return,
        };
        let replay_cutoff = latest_output_sequence as u64;
        match self
            .replay(&auth, &room_id, after_sequence, replay_cutoff)
            .await
        {
            Ok(frames) => {
                for frame in frames {
                    let Ok(payload) = serde_json::to_string(&ServerSocketMessage::Frame {
                        frame: Box::new(frame),
                    }) else {
                        return;
                    };
                    if sink.send(Message::Text(payload.into())).await.is_err() {
                        return;
                    }
                }
            }
            Err(error) => {
                let payload = socket_error(&error);
                let _ = sink.send(Message::Text(payload.into())).await;
                return;
            }
        }
        let next_input_sequence: i64 = match sqlx::query_scalar(
            "SELECT next_input_sequence FROM room_participants WHERE room_id=? AND device_id=? AND removed_at IS NULL",
        )
        .bind(&room_id)
        .bind(&auth.device_id)
        .fetch_one(&self.store.pool)
        .await
        {
            Ok(value) => value,
            Err(_) => return,
        };
        let Ok(ready) = serde_json::to_string(&ServerSocketMessage::Ready {
            latest_output_sequence: replay_cutoff,
            next_input_sequence: next_input_sequence as u64,
        }) else {
            return;
        };
        if sink.send(Message::Text(ready.into())).await.is_err() {
            return;
        }
        let participants = match self.room_participants(&room_id).await {
            Ok(participants) => participants,
            Err(_) => return,
        };
        let Ok(participants) =
            serde_json::to_string(&ServerSocketMessage::Participants { participants })
        else {
            return;
        };
        if sink.send(Message::Text(participants.into())).await.is_err() {
            return;
        }
        let lease = match self.current_control(&room_id).await {
            Ok(lease) => lease,
            Err(_) => return,
        };
        let Ok(control) = serde_json::to_string(&ServerSocketMessage::Control { lease }) else {
            return;
        };
        if sink.send(Message::Text(control.into())).await.is_err() {
            return;
        }
        loop {
            tokio::select! {
                incoming = stream.next() => {
                    let Some(Ok(message)) = incoming else { break; };
                    match message {
                        Message::Text(text) if text.len() <= MAX_ENVELOPE_BYTES => {
                            let current = match self.store.authenticate_device(&token).await {
                                Ok(value) => value,
                                Err(error) => {
                                    let _ = sink.send(Message::Text(socket_error(&error).into())).await;
                                    break;
                                }
                            };
                            let parsed = serde_json::from_str::<ClientSocketMessage>(&text)
                                .map_err(|_| RelayError::Validation("WebSocket 消息格式无效".into()));
                            let result = match parsed {
                                Ok(ClientSocketMessage::Frame { frame }) => {
                                    let direction = frame.direction.clone();
                                    let sequence = frame.sequence;
                                    self.process_frame(&current, frame).await.map(|_| (direction, sequence))
                                },
                                Err(error) => Err(error),
                            };
                            match result {
                                Ok((direction, sequence)) => {
                                    let Ok(accepted) = serde_json::to_string(&ServerSocketMessage::Accepted { direction, sequence }) else { break; };
                                    if sink.send(Message::Text(accepted.into())).await.is_err() { break; }
                                }
                                Err(error) => {
                                    if sink.send(Message::Text(socket_error(&error).into())).await.is_err() { break; }
                                }
                            }
                        }
                        Message::Ping(value) => {
                            let current = match self.store.authenticate_device(&token).await {
                                Ok(value) => value,
                                Err(_) => break,
                            };
                            if self.authorize_room(&current, &room_id).await.is_err() {
                                break;
                            }
                            if sink.send(Message::Pong(value)).await.is_err() { break; }
                        }
                        Message::Close(_) => break,
                        Message::Text(_) | Message::Binary(_) => {
                            let error = RelayError::Validation("WebSocket 消息超过限制或类型无效".into());
                            if sink.send(Message::Text(socket_error(&error).into())).await.is_err() { break; }
                        }
                        Message::Pong(_) => {}
                    }
                }
                broadcast = receiver.recv() => {
                    let frame = match broadcast {
                        Ok(frame) => frame,
                        Err(broadcast::error::RecvError::Lagged(_)) => continue,
                        Err(broadcast::error::RecvError::Closed) => break,
                    };
                    let current = match self.store.authenticate_device(&token).await {
                        Ok(value) => value,
                        Err(_) => break,
                    };
                    if self.authorize_room(&current, &room_id).await.is_err() { break; }
                    if frame.direction == "output"
                        && frame.sequence.is_some_and(|sequence| sequence <= replay_cutoff)
                    {
                        continue;
                    }
                    if frame.direction == "input" && current.device_id != frame.host_device_id {
                        continue;
                    }
                    if sink.send(Message::Text(frame.json.into())).await.is_err() { break; }
                }
                changed = shutdown.changed() => {
                    if changed.is_err() || *shutdown.borrow() {
                        let _ = sink.send(Message::Close(None)).await;
                        break;
                    }
                }
            }
        }
        drop(receiver);
        if sender.receiver_count() == 0 {
            self.hubs.lock().remove(&room_id);
        }
    }
}

async fn prune_replay(
    transaction: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    room_id: &str,
) -> RelayResult<()> {
    let cutoff = (Utc::now() - ChronoDuration::minutes(REPLAY_MINUTES)).to_rfc3339();
    sqlx::query("DELETE FROM relay_frames WHERE room_id=? AND created_at<?")
        .bind(room_id)
        .bind(cutoff)
        .execute(&mut **transaction)
        .await?;
    sqlx::query("DELETE FROM relay_frames WHERE room_id=? AND sequence NOT IN (SELECT sequence FROM relay_frames WHERE room_id=? ORDER BY sequence DESC LIMIT ?)")
        .bind(room_id)
        .bind(room_id)
        .bind(MAX_REPLAY_FRAMES)
        .execute(&mut **transaction)
        .await?;
    let rows = sqlx::query(
        "SELECT sequence,encoded_bytes FROM relay_frames WHERE room_id=? ORDER BY sequence",
    )
    .bind(room_id)
    .fetch_all(&mut **transaction)
    .await?;
    let mut total: i64 = rows.iter().map(|row| row.get::<i64, _>(1)).sum();
    for row in rows {
        if total <= MAX_REPLAY_BYTES {
            break;
        }
        let sequence: i64 = row.get(0);
        let bytes: i64 = row.get(1);
        sqlx::query("DELETE FROM relay_frames WHERE room_id=? AND sequence=?")
            .bind(room_id)
            .bind(sequence)
            .execute(&mut **transaction)
            .await?;
        total = total.saturating_sub(bytes);
    }
    Ok(())
}

fn validate_invitation_shape(invitation: &TeamTerminalInvitation) -> RelayResult<()> {
    if serde_jcs::to_vec(invitation)
        .map_err(|_| RelayError::Validation("房间邀请无法规范化".into()))?
        .len()
        > MAX_ENVELOPE_BYTES
        || invitation.schema_version != 1
        || invitation.key_epoch < 1
        || invitation.next_input_sequence == 0
    {
        return Err(RelayError::Validation(
            "房间邀请版本、epoch 或大小无效".into(),
        ));
    }
    for (value, label) in [
        (&invitation.room_id, "房间 ID"),
        (&invitation.workspace_id, "工作区 ID"),
        (&invitation.host_member_id, "主持成员 ID"),
        (&invitation.host_device_id, "主持设备 ID"),
        (&invitation.recipient_member_id, "接收成员 ID"),
        (&invitation.recipient_device_id, "接收设备 ID"),
    ] {
        validate_uuid(value, label)?;
    }
    let created = DateTime::parse_from_rfc3339(&invitation.created_at)
        .map_err(|_| RelayError::Validation("房间邀请创建时间无效".into()))?
        .with_timezone(&Utc);
    let expires = DateTime::parse_from_rfc3339(&invitation.expires_at)
        .map_err(|_| RelayError::Validation("房间邀请过期时间无效".into()))?
        .with_timezone(&Utc);
    if expires <= Utc::now() || expires <= created || expires - created > ChronoDuration::minutes(5)
    {
        return Err(RelayError::Validation("房间邀请时间范围无效".into()));
    }
    let ephemeral = invitation
        .ephemeral_public_key
        .strip_prefix("x25519:")
        .and_then(|value| URL_SAFE_NO_PAD.decode(value).ok());
    let nonce = URL_SAFE_NO_PAD.decode(&invitation.key_nonce).ok();
    let wrapped = URL_SAFE_NO_PAD.decode(&invitation.wrapped_room_key).ok();
    if ephemeral.as_deref().is_none_or(|value| value.len() != 32)
        || nonce.as_deref().is_none_or(|value| value.len() != 12)
        || wrapped.as_deref().is_none_or(|value| value.len() != 48)
    {
        return Err(RelayError::Validation(
            "房间邀请公钥、nonce 或封装密钥长度无效".into(),
        ));
    }
    Ok(())
}

fn validate_frame_shape(frame: &TeamTerminalEncryptedFrame) -> RelayResult<()> {
    if serde_jcs::to_vec(frame)
        .map_err(|_| RelayError::Validation("终端帧无法规范化".into()))?
        .len()
        > MAX_ENVELOPE_BYTES
        || frame.schema_version != 1
        || frame.sequence == 0
        || frame.kind != "terminal"
    {
        return Err(RelayError::Validation(
            "终端帧版本、类型、序号或大小无效".into(),
        ));
    }
    for (value, label) in [
        (&frame.workspace_id, "工作区 ID"),
        (&frame.room_id, "房间 ID"),
        (&frame.sender_member_id, "发送成员 ID"),
        (&frame.sender_device_id, "发送设备 ID"),
    ] {
        validate_uuid(value, label)?;
    }
    let nonce = URL_SAFE_NO_PAD
        .decode(&frame.nonce)
        .map_err(|_| RelayError::Validation("终端帧 nonce 编码无效".into()))?;
    let ciphertext = URL_SAFE_NO_PAD
        .decode(&frame.ciphertext)
        .map_err(|_| RelayError::Validation("终端帧密文编码无效".into()))?;
    if nonce.len() != 12 || ciphertext.len() <= 16 || ciphertext.len() > MAX_FRAME_BYTES + 16 {
        return Err(RelayError::Validation("终端帧 nonce 或密文长度无效".into()));
    }
    Ok(())
}

fn decode_signing_key(value: &str) -> RelayResult<VerifyingKey> {
    let encoded = value
        .strip_prefix("ed25519:")
        .ok_or_else(|| RelayError::Validation("设备签名公钥格式无效".into()))?;
    let bytes: [u8; 32] = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| RelayError::Validation("设备签名公钥编码无效".into()))?
        .try_into()
        .map_err(|_| RelayError::Validation("设备签名公钥长度无效".into()))?;
    VerifyingKey::from_bytes(&bytes).map_err(|_| RelayError::Validation("设备签名公钥无效".into()))
}

fn decode_signature(value: Option<&str>) -> RelayResult<Signature> {
    let encoded = value
        .and_then(|value| value.strip_prefix("ed25519:"))
        .ok_or_else(|| RelayError::Validation("签名缺失或格式无效".into()))?;
    Signature::from_slice(
        &URL_SAFE_NO_PAD
            .decode(encoded)
            .map_err(|_| RelayError::Validation("签名编码无效".into()))?,
    )
    .map_err(|_| RelayError::Validation("签名长度无效".into()))
}

fn verify_invitation_signature(
    invitation: &TeamTerminalInvitation,
    signing_public_key: &str,
) -> RelayResult<()> {
    let mut unsigned = invitation.clone();
    unsigned.signature = None;
    let payload = serde_jcs::to_vec(&unsigned)
        .map_err(|_| RelayError::Validation("房间邀请无法规范化".into()))?;
    decode_signing_key(signing_public_key)?
        .verify(
            &payload,
            &decode_signature(invitation.signature.as_deref())?,
        )
        .map_err(|_| RelayError::Authentication("房间邀请签名验证失败".into()))
}

fn verify_frame_signature(
    frame: &TeamTerminalEncryptedFrame,
    signing_public_key: &str,
) -> RelayResult<()> {
    let mut unsigned = frame.clone();
    unsigned.signature = None;
    let payload = serde_jcs::to_vec(&unsigned)
        .map_err(|_| RelayError::Validation("终端帧无法规范化".into()))?;
    decode_signing_key(signing_public_key)?
        .verify(&payload, &decode_signature(frame.signature.as_deref())?)
        .map_err(|_| RelayError::Authentication("终端帧签名验证失败".into()))
}

fn socket_error(error: &RelayError) -> String {
    let (code, message) = match error {
        RelayError::Validation(message) => ("validation", message.clone()),
        RelayError::Authentication(message) => ("authentication", message.clone()),
        RelayError::PermissionDenied(message) => ("permissionDenied", message.clone()),
        RelayError::NotFound(message) => ("notFound", message.clone()),
        RelayError::Conflict(message) => ("conflict", message.clone()),
        RelayError::Unavailable(message) => ("unavailable", message.clone()),
        RelayError::Storage(_) | RelayError::Internal => ("internal", "团队服务暂时不可用".into()),
    };
    serde_json::to_string(&ServerSocketMessage::Error {
        code: code.into(),
        message,
    })
    .unwrap_or_else(|_| {
        serde_json::json!({"type":"error","code":"internal","message":"团队服务暂时不可用"})
            .to_string()
    })
}
