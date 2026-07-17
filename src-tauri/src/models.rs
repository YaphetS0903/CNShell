use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundTask {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub result: Option<serde_json::Value>,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProfile {
    pub id: String,
    pub folder_id: Option<String>,
    pub protocol: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub host_key_policy: String,
    pub note: String,
    #[sqlx(skip)]
    pub tags: Vec<String>,
    pub encoding: String,
    pub startup_command: Option<String>,
    pub proxy_id: Option<String>,
    #[sqlx(skip)]
    pub environment: std::collections::BTreeMap<String, String>,
    #[sqlx(skip)]
    pub has_credential: bool,
    pub created_at: String,
    pub updated_at: String,
    pub last_connected_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveConnectionInput {
    pub id: String,
    pub folder_id: Option<String>,
    pub protocol: String,
    pub name: String,
    pub host: String,
    pub port: i64,
    pub username: String,
    pub auth_type: String,
    pub private_key_path: Option<String>,
    pub certificate_path: Option<String>,
    pub host_key_policy: String,
    pub note: String,
    pub tags: Vec<String>,
    pub encoding: String,
    pub startup_command: Option<String>,
    pub proxy_id: Option<String>,
    pub environment: std::collections::BTreeMap<String, String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct ProxyProfile {
    pub id: String,
    pub name: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub jump_connection_id: Option<String>,
    #[sqlx(skip)]
    pub has_credential: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveProxyInput {
    pub id: String,
    pub name: String,
    #[serde(rename = "type")]
    pub proxy_type: String,
    pub host: String,
    pub port: i64,
    pub username: Option<String>,
    pub jump_connection_id: Option<String>,
    pub credential: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct PortForward {
    pub id: String,
    pub connection_id: String,
    #[sqlx(rename = "type")]
    #[serde(rename = "type")]
    pub forward_type: String,
    pub bind_host: String,
    pub bind_port: i64,
    pub destination_host: Option<String>,
    pub destination_port: Option<i64>,
    pub auto_start: bool,
    #[sqlx(skip)]
    pub status: Option<String>,
    #[sqlx(skip)]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSnippet {
    pub id: String,
    pub name: String,
    pub command: String,
    pub description: String,
    pub tags: Vec<String>,
    pub sort_order: i64,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Folder {
    pub id: String,
    pub name: String,
    pub parent_id: Option<String>,
    pub sort_order: i64,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionDiagnostic {
    pub stage: String,
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u128>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SshCertificateInfo {
    pub path: String,
    pub certificate_type: String,
    pub key_id: String,
    pub serial: String,
    pub signing_ca: String,
    pub valid_from: String,
    pub valid_to: String,
    pub principals: Vec<String>,
    pub valid_now: bool,
    pub status: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct Fido2Identity {
    pub key_type: String,
    pub comment: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TouchIdSyncStatus {
    pub supported: bool,
    pub saved: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalSession {
    pub id: String,
    pub connection_id: String,
    pub session_type: String,
    pub title: String,
    pub status: String,
    pub started_at: String,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalOutput {
    pub session_id: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalStatus {
    pub session_id: String,
    pub status: String,
    pub last_error: Option<String>,
    pub attempt: Option<u8>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionLogStatus {
    pub session_id: String,
    pub active: bool,
    pub path: Option<String>,
    pub format: Option<String>,
    pub line_timestamps: bool,
    pub started_at: Option<String>,
    pub bytes_written: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTargetResult {
    pub connection_id: String,
    pub name: String,
    pub status: String,
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<i32>,
    pub duration_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchExecution {
    pub id: String,
    pub command: String,
    pub status: String,
    pub created_at: String,
    pub targets: Vec<BatchTargetResult>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditSession {
    pub id: String,
    pub remote_path: String,
    pub local_path: String,
    pub expected_modified_at: Option<u64>,
    pub started_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalEditSnapshot {
    pub id: String,
    pub content: String,
    pub expected_modified_at: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenSshHost {
    pub alias: String,
    pub hostname: String,
    pub user: Option<String>,
    pub port: u16,
    pub identity_file: Option<String>,
    pub proxy_jump: Option<String>,
    pub source: String,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GeneratedSshKey {
    pub private_key_path: String,
    pub public_key_path: String,
    pub public_key: String,
    pub fingerprint: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProtocolCapability {
    pub id: String,
    pub label: String,
    pub available: bool,
    pub executable: Option<String>,
    pub message: String,
    pub security_warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformFeatureCapability {
    pub available: bool,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct PlatformCapabilities {
    pub operating_system: String,
    pub architecture: String,
    pub display_name: String,
    pub shortcut_modifier: String,
    pub credential_store_name: String,
    pub file_manager_name: String,
    pub biometric_name: String,
    pub rdp: PlatformFeatureCapability,
    pub mosh: PlatformFeatureCapability,
    pub kermit: PlatformFeatureCapability,
    pub x11: PlatformFeatureCapability,
    pub ssh_agent: PlatformFeatureCapability,
    pub biometric: PlatformFeatureCapability,
    pub serial: PlatformFeatureCapability,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionProtocolOptions {
    pub connection_id: String,
    pub agent_forwarding: bool,
    #[serde(default)]
    pub x11_enabled: bool,
    #[serde(default)]
    pub mosh_enabled: bool,
    #[serde(default = "default_mosh_port_start")]
    pub mosh_port_start: u16,
    #[serde(default = "default_mosh_port_end")]
    pub mosh_port_end: u16,
}

fn default_mosh_port_start() -> u16 {
    60000
}

fn default_mosh_port_end() -> u16 {
    60010
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationPlan {
    pub id: String,
    pub name: String,
    pub connection_id: String,
    pub steps: Vec<AutomationStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStep {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub command: Option<String>,
    #[serde(default)]
    pub pattern: Option<String>,
    #[serde(default)]
    pub timeout_seconds: Option<u64>,
    #[serde(default)]
    pub action: Option<String>,
    #[serde(default)]
    pub direction: Option<String>,
    #[serde(default)]
    pub local_path: Option<String>,
    #[serde(default)]
    pub remote_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationStepResult {
    pub step_id: String,
    pub kind: String,
    pub status: String,
    pub started_at: String,
    pub duration_ms: u64,
    pub output: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationRun {
    pub run_id: String,
    pub plan_id: String,
    pub status: String,
    pub current_step: Option<String>,
    pub results: Vec<AutomationStepResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AutomationSchedule {
    pub id: String,
    pub plan: AutomationPlan,
    pub schedule_type: String,
    pub expression: String,
    pub enabled: bool,
    #[serde(default = "default_misfire_policy")]
    pub misfire_policy: String,
    #[serde(default = "default_automation_time_zone")]
    pub time_zone: String,
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
    #[serde(default)]
    pub last_occurrence_key: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonAutomationManifest {
    pub connection_id: String,
    pub permissions: Vec<String>,
    #[serde(default)]
    pub allowed_local_paths: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonAutomationRequest {
    pub id: String,
    pub name: String,
    pub source: String,
    pub manifest: PythonAutomationManifest,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonAutomationPreview {
    pub script_hash: String,
    pub target_connection_id: String,
    pub permissions: Vec<String>,
    pub steps: Vec<AutomationStep>,
    pub warnings: Vec<String>,
}

fn default_misfire_policy() -> String {
    "skip".into()
}

fn default_automation_time_zone() -> String {
    "UTC".into()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SyncOptions {
    pub include_hosts: bool,
    pub include_private_key_paths: bool,
    pub include_credentials: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SyncResult {
    pub path: String,
    pub conflict_copy: Option<String>,
    pub connection_count: usize,
    pub encrypted: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavProfile {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub has_credential: bool,
    pub sync_on_startup: bool,
    pub has_sync_passphrase: bool,
    pub sync_options: SyncOptions,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveWebDavProfileInput {
    pub id: String,
    pub name: String,
    pub url: String,
    pub username: String,
    pub password: Option<String>,
    #[serde(default)]
    pub sync_on_startup: bool,
    #[serde(default)]
    pub sync_options: SyncOptions,
    pub sync_passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WebDavSyncProgress {
    pub profile_id: String,
    pub phase: String,
    pub transferred_bytes: u64,
    pub total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiProviderProfile {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub model: String,
    pub has_api_key: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAiProviderInput {
    pub id: String,
    pub name: String,
    pub endpoint: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AiPreviewInput {
    pub provider_id: String,
    pub kind: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiRequestPreview {
    pub request_id: String,
    pub provider_name: String,
    pub endpoint: String,
    pub model: String,
    pub kind: String,
    pub redacted_content: String,
    pub redactions: Vec<String>,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AiAssistantResult {
    pub request_id: String,
    pub kind: String,
    pub model: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginManifest {
    pub id: String,
    pub name: String,
    pub version: String,
    pub api_version: u32,
    pub entrypoint: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    #[serde(default)]
    pub network_domains: Vec<String>,
    pub publisher: Option<String>,
    pub signature: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPermissionReport {
    pub manifest: PluginManifest,
    pub valid: bool,
    pub signature_status: String,
    pub requested_permissions: Vec<String>,
    pub default_granted_permissions: Vec<String>,
    pub denied_permissions: Vec<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginInstallRecord {
    pub id: String,
    pub name: String,
    pub version: String,
    pub manifest_path: String,
    pub digest: String,
    #[serde(default)]
    pub entrypoint_digest: String,
    #[serde(default)]
    pub publisher_id: Option<String>,
    pub signature_status: String,
    pub requested_permissions: Vec<String>,
    #[serde(default)]
    pub network_domains: Vec<String>,
    pub denied_permissions: Vec<String>,
    #[serde(default)]
    pub granted_permissions: Vec<String>,
    pub enabled: bool,
    pub executable: bool,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginEnableInput {
    pub id: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginPublisherRoot {
    pub id: String,
    pub name: String,
    pub public_key: String,
    pub fingerprint: String,
    pub enabled: bool,
    pub installed_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginRunResult {
    pub plugin_id: String,
    pub version: String,
    pub status_code: i32,
    pub fuel_consumed: u64,
    pub duration_ms: u64,
    pub logs: Vec<String>,
    pub credential_proxy_request: Option<PluginCredentialProxyRequest>,
    pub terminal_input_request: Option<PluginTerminalInputRequest>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PluginRunInput {
    pub id: String,
    pub connection_id: Option<String>,
    pub selected_text: Option<String>,
    pub network_url: Option<String>,
    pub directory_path: Option<String>,
    pub directory_relative_path: Option<String>,
    pub terminal_session_id: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginCredentialProxyRequest {
    pub request_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub connection_id: String,
    pub connection_name: String,
    pub operation: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginTerminalInputRequest {
    pub request_id: String,
    pub plugin_id: String,
    pub plugin_name: String,
    pub session_id: String,
    pub data: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamWorkspace {
    pub id: String,
    pub name: String,
    pub local_member_id: String,
    pub local_role: String,
    pub key_epoch: i64,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    pub id: String,
    pub workspace_id: String,
    pub display_name: String,
    pub role: String,
    pub status: String,
    pub joined_at: String,
    pub updated_at: String,
    pub removed_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateTeamWorkspaceInput {
    pub name: String,
    pub owner_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveTeamMemberInput {
    pub workspace_id: String,
    pub member_id: Option<String>,
    pub display_name: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamPermissionReport {
    pub workspace_id: String,
    pub member_id: String,
    pub role: String,
    pub permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamAuditEvent {
    pub id: String,
    pub workspace_id: String,
    pub actor_member_id: String,
    pub action: String,
    pub target_type: String,
    pub target_id: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TeamDevice {
    pub id: String,
    pub workspace_id: String,
    pub member_id: String,
    pub name: String,
    pub encryption_public_key: String,
    pub signing_public_key: String,
    pub fingerprint: String,
    pub is_local: bool,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub revoked_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamShareExportInput {
    pub workspace_id: String,
    pub connection_id: String,
    pub recipient_device_ids: Vec<String>,
    pub include_credential: bool,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamSharePreview {
    pub request_id: String,
    pub workspace_id: String,
    pub sender_member_id: String,
    pub connection_name: String,
    pub protocol: String,
    pub host: String,
    pub has_credential: bool,
    pub key_epoch: i64,
    pub expires_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTerminalParticipant {
    pub member_id: String,
    pub device_id: String,
    pub role: String,
    pub joined_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamControlLease {
    pub id: String,
    pub member_id: String,
    pub device_id: String,
    pub expires_at: String,
    pub generation: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTerminalRoom {
    pub id: String,
    pub workspace_id: String,
    pub terminal_session_id: String,
    pub host_member_id: String,
    pub host_device_id: String,
    pub key_epoch: i64,
    pub status: String,
    pub participants: Vec<TeamTerminalParticipant>,
    pub control_lease: Option<TeamControlLease>,
    pub next_output_sequence: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTerminalFrame {
    pub room_id: String,
    pub sequence: u64,
    pub kind: String,
    pub data_base64: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamTerminalClientRoom {
    pub room_id: String,
    pub workspace_id: String,
    pub key_epoch: i64,
    pub host_member_id: String,
    pub host_device_id: String,
    pub local_member_id: String,
    pub local_device_id: String,
    pub next_output_sequence: u64,
    pub next_input_sequence: u64,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayProfile {
    pub id: String,
    pub name: String,
    pub base_url: String,
    pub account_id: Option<String>,
    pub account_email: Option<String>,
    pub has_account_session: bool,
    pub account_session_expires_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SaveTeamRelayProfileInput {
    pub id: String,
    pub name: String,
    pub base_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TeamRelayAccountInput {
    pub profile_id: String,
    pub email: String,
    pub password: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct VerifyTeamRelayAccountInput {
    pub profile_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ResendTeamRelayVerificationInput {
    pub profile_id: String,
    pub email: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayAccountRegistration {
    pub profile: TeamRelayProfile,
    pub verification_required: bool,
    pub verification_expires_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayWorkspaceBinding {
    pub workspace_id: String,
    pub profile_id: String,
    pub profile_name: String,
    pub base_url: String,
    pub account_id: String,
    pub device_session_expires_at: Option<String>,
    pub last_synced_at: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CreateTeamRelayInvitationInput {
    pub workspace_id: String,
    pub email: String,
    pub role: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayInvitation {
    pub invitation_id: String,
    pub token: String,
    pub member_id: String,
    pub email: String,
    pub role: String,
    pub expires_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AcceptTeamRelayInvitationInput {
    pub profile_id: String,
    pub token: String,
    pub device_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct UpdateTeamRelayMemberInput {
    pub workspace_id: String,
    pub member_id: String,
    pub role: String,
    pub status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayTerminalInvitation {
    pub room_id: String,
    pub invitation: TeamTerminalInvitation,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayTerminalSession {
    pub room_id: String,
    pub workspace_id: String,
    pub mode: String,
    pub terminal_session_id: Option<String>,
    pub local_member_id: String,
    pub local_device_id: String,
    pub status: String,
    pub last_error: Option<String>,
    pub last_output_sequence: u64,
    pub participants: Vec<TeamTerminalParticipant>,
    pub control_lease: Option<TeamControlLease>,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRelayTerminalEvent {
    pub room_id: String,
    pub kind: String,
    pub session: TeamRelayTerminalSession,
    pub sequence: Option<u64>,
    pub data_base64: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PluginAuditEvent {
    pub id: String,
    pub plugin_id: String,
    pub action: String,
    pub detail: String,
    pub digest: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteFile {
    pub name: String,
    pub path: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: Option<u64>,
    pub permissions: String,
    pub owner: Option<u32>,
    pub group: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct TransferTask {
    pub id: String,
    pub session_id: String,
    pub direction: String,
    pub source: String,
    pub destination: String,
    pub total_bytes: i64,
    pub transferred_bytes: i64,
    pub status: String,
    pub conflict_policy: String,
    pub error: Option<String>,
    pub created_at: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransferInput {
    pub session_id: String,
    pub direction: String,
    pub source: String,
    pub destination: String,
    pub conflict_policy: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ZmodemEvent {
    pub id: String,
    pub session_id: String,
    pub direction: String,
    pub status: String,
    pub file_name: Option<String>,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SerialTransferEvent {
    pub id: String,
    pub session_id: String,
    pub protocol: String,
    pub direction: String,
    pub status: String,
    pub file_name: Option<String>,
    pub total_bytes: Option<u64>,
    pub transferred_bytes: u64,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessInfo {
    pub pid: u32,
    pub started_at: String,
    pub user: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSocket {
    pub protocol: String,
    pub state: String,
    pub local_address: String,
    pub peer_address: String,
    pub process: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkSocketReport {
    pub items: Vec<NetworkSocket>,
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkDiagnosticResult {
    pub kind: String,
    pub target: String,
    pub output: String,
    pub duration_ms: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiskInfo {
    pub filesystem: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInfo {
    pub interface_name: String,
    pub rx_bytes_per_second: u64,
    pub tx_bytes_per_second: u64,
    pub rx_total_bytes: u64,
    pub tx_total_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MonitorSnapshot {
    pub session_id: String,
    pub timestamp: i64,
    pub hostname: String,
    pub ip: String,
    pub uptime_seconds: u64,
    pub load: [f64; 3],
    pub cpu_percent: f64,
    pub memory_used_bytes: u64,
    pub memory_total_bytes: u64,
    pub swap_used_bytes: u64,
    pub swap_total_bytes: u64,
    pub latency_ms: Option<f64>,
    pub processes: Vec<ProcessInfo>,
    pub disks: Vec<DiskInfo>,
    pub networks: Vec<NetworkInfo>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NetworkInterface {
    pub name: String,
    pub addresses: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SystemInfo {
    pub hostname: String,
    pub os: String,
    pub kernel: String,
    pub architecture: String,
    pub cpu_model: String,
    pub cpu_cores: u32,
    pub memory_total_bytes: u64,
    pub interfaces: Vec<NetworkInterface>,
    pub disks: Vec<DiskInfo>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RdpPreflight {
    pub available: bool,
    pub executable: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RdpConnectionOptions {
    pub connection_id: String,
    pub display_mode: String,
    pub display_id: Option<u32>,
    pub scale_mode: String,
    pub quality: String,
    pub clipboard: bool,
    pub audio_mode: String,
    pub microphone: bool,
    pub drive_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct RdpDisplay {
    pub id: u32,
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerialDeviceInfo {
    pub path: String,
    pub kind: String,
    pub label: String,
    pub vendor_id: Option<u16>,
    pub product_id: Option<u16>,
    pub serial_number: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SerialConnectionOptions {
    pub connection_id: String,
    pub data_bits: u8,
    pub parity: String,
    pub stop_bits: u8,
    pub flow_control: String,
    pub dtr: bool,
    pub rts: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPreferences {
    pub font_family: String,
    pub font_size: u16,
    pub line_height: f64,
    pub scrollback: u32,
    pub cursor_style: String,
    pub cursor_blink: bool,
    pub color_scheme: String,
}

impl Default for TerminalPreferences {
    fn default() -> Self {
        Self {
            font_family: "system".into(),
            font_size: 13,
            line_height: 1.25,
            scrollback: 10_000,
            cursor_style: "bar".into(),
            cursor_blink: true,
            color_scheme: "cnshell".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettings {
    pub theme: String,
    pub monitor_interval_ms: u64,
    pub remember_command_history: bool,
    pub confirm_close_active_session: bool,
    pub show_hidden_files: bool,
    #[serde(default = "default_true")]
    pub show_welcome_help: bool,
    #[serde(default)]
    pub terminal: TerminalPreferences,
    #[serde(default)]
    pub terminal_overrides: std::collections::BTreeMap<String, TerminalPreferences>,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: "system".into(),
            monitor_interval_ms: 2000,
            remember_command_history: true,
            confirm_close_active_session: true,
            show_hidden_files: false,
            show_welcome_help: true,
            terminal: TerminalPreferences::default(),
            terminal_overrides: Default::default(),
        }
    }
}
fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proxy_profile_uses_the_frontend_contract_field_name() {
        let profile = ProxyProfile {
            id: "proxy".into(),
            name: "SOCKS".into(),
            proxy_type: "socks5".into(),
            host: "127.0.0.1".into(),
            port: 1080,
            username: None,
            jump_connection_id: None,
            has_credential: false,
        };
        let value = serde_json::to_value(profile).unwrap();
        assert_eq!(value["type"], "socks5");
        assert!(value.get("proxyType").is_none());
    }
}
