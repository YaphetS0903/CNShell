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
    pub next_run_at: Option<String>,
    pub last_run_at: Option<String>,
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
