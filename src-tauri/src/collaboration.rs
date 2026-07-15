use crate::{
    db::Database,
    error::{AppError, AppResult},
    models::{TeamControlLease, TeamTerminalFrame, TeamTerminalParticipant, TeamTerminalRoom},
    team,
};
use base64::{Engine as _, engine::general_purpose::STANDARD};
use chrono::{Duration as ChronoDuration, Utc};
use parking_lot::Mutex;
use sqlx::Row;
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};

const MAX_ROOMS: usize = 16;
const MAX_PARTICIPANTS: usize = 64;
const MAX_FRAME_BYTES: usize = 64 * 1024;
const MIN_LEASE_SECONDS: u64 = 10;
const MAX_LEASE_SECONDS: u64 = 5 * 60;

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
}

#[derive(Clone, Default)]
pub struct CollaborationManager {
    rooms: Arc<Mutex<HashMap<String, RoomState>>>,
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
        Ok(room.snapshot())
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
        Ok(room.snapshot())
    }

    pub fn close_all(&self) {
        let mut rooms = self.rooms.lock();
        for room in rooms.values_mut() {
            room.status = "closed".into();
            room.control_lease = None;
        }
    }

    pub fn close_terminal(&self, terminal_session_id: &str) -> Vec<(String, String)> {
        let mut rooms = self.rooms.lock();
        let mut closed = Vec::new();
        for room in rooms.values_mut().filter(|room| {
            room.terminal_session_id == terminal_session_id && room.status == "active"
        }) {
            room.status = "closed".into();
            room.control_lease = None;
            closed.push((room.workspace_id.clone(), room.id.clone()));
        }
        closed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        models::{CreateTeamWorkspaceInput, SaveTeamMemberInput},
        team,
    };
    use tempfile::tempdir;

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
}
