use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterAccountInput {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct LoginInput {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSessionOutput {
    pub account_id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceRegistration {
    pub id: String,
    pub name: String,
    pub encryption_public_key: String,
    pub signing_public_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BootstrapWorkspaceInput {
    pub workspace_id: String,
    pub workspace_name: String,
    pub member_id: String,
    pub device: DeviceRegistration,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSessionOutput {
    pub workspace_id: String,
    pub member_id: String,
    pub device_id: String,
    pub role: String,
    pub key_epoch: i64,
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateWorkspaceInvitationInput {
    pub email: String,
    pub role: String,
    pub member_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceInvitationOutput {
    pub invitation_id: String,
    pub token: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptWorkspaceInvitationInput {
    pub token: String,
    pub device: DeviceRegistration,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceMemberView {
    pub id: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceDeviceView {
    pub id: String,
    pub member_id: String,
    pub name: String,
    pub encryption_public_key: String,
    pub signing_public_key: String,
    pub fingerprint: String,
    pub status: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub id: String,
    pub name: String,
    pub key_epoch: i64,
    pub members: Vec<WorkspaceMemberView>,
    pub devices: Vec<WorkspaceDeviceView>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RelayAuditEvent {
    pub id: String,
    pub workspace_id: String,
    pub actor_member_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub created_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateMemberInput {
    pub role: String,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DeviceChallengeInput {
    pub workspace_id: String,
    pub device_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceChallengeOutput {
    pub challenge_id: String,
    pub challenge: String,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateDeviceSessionInput {
    pub challenge_id: String,
    pub challenge: String,
    pub signature: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeamTerminalInvitation {
    pub schema_version: u32,
    pub room_id: String,
    pub workspace_id: String,
    pub key_epoch: i64,
    pub host_member_id: String,
    pub host_device_id: String,
    pub recipient_member_id: String,
    pub recipient_device_id: String,
    pub ephemeral_public_key: String,
    pub key_nonce: String,
    pub wrapped_room_key: String,
    pub replay_from_sequence: u64,
    pub next_input_sequence: u64,
    pub created_at: String,
    pub expires_at: String,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeamTerminalEncryptedFrame {
    pub schema_version: u32,
    pub workspace_id: String,
    pub room_id: String,
    pub key_epoch: i64,
    pub sender_member_id: String,
    pub sender_device_id: String,
    pub direction: String,
    pub kind: String,
    pub sequence: u64,
    pub lease_id: Option<String>,
    pub lease_generation: u64,
    pub nonce: String,
    pub ciphertext: String,
    pub signature: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateRoomInput {
    pub room_id: String,
    pub key_epoch: i64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoomView {
    pub id: String,
    pub workspace_id: String,
    pub host_member_id: String,
    pub host_device_id: String,
    pub key_epoch: i64,
    pub status: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RouteRoomInvitationInput {
    pub invitation: TeamTerminalInvitation,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutedRoomInvitation {
    pub room_id: String,
    pub invitation: TeamTerminalInvitation,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct GrantControlInput {
    pub device_id: String,
    pub duration_seconds: u64,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ControlLeaseOutput {
    pub lease_id: String,
    pub member_id: String,
    pub device_id: String,
    pub generation: u64,
    pub expires_at: String,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase", deny_unknown_fields)]
pub enum ClientSocketMessage {
    Frame { frame: TeamTerminalEncryptedFrame },
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum ServerSocketMessage {
    Frame {
        frame: Box<TeamTerminalEncryptedFrame>,
    },
    Error {
        code: String,
        message: String,
    },
}
