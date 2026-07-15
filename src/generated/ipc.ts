// Generated from src-tauri/src/models.rs by scripts/generate-ipc-types.mjs.
// Do not edit directly; run npm run generate:ipc.

export interface BackgroundTask {
  id: string;
  kind: string;
  status: string;
  result: unknown | null;
  error: string | null;
  createdAt: string;
}

export interface ConnectionProfile {
  id: string;
  folderId: string | null;
  protocol: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: string;
  privateKeyPath: string | null;
  certificatePath: string | null;
  hostKeyPolicy: string;
  note: string;
  tags: string[];
  encoding: string;
  startupCommand: string | null;
  proxyId: string | null;
  environment: Record<string, string>;
  hasCredential: boolean;
  createdAt: string;
  updatedAt: string;
  lastConnectedAt: string | null;
}

export interface SaveConnectionInput {
  id: string;
  folderId?: string | null;
  protocol: string;
  name: string;
  host: string;
  port: number;
  username: string;
  authType: string;
  privateKeyPath?: string | null;
  certificatePath?: string | null;
  hostKeyPolicy: string;
  note: string;
  tags: string[];
  encoding: string;
  startupCommand?: string | null;
  proxyId?: string | null;
  environment: Record<string, string>;
  credential?: string | null;
}

export interface ProxyProfile {
  id: string;
  name: string;
  type: string;
  host: string;
  port: number;
  username: string | null;
  jumpConnectionId: string | null;
  hasCredential: boolean;
}

export interface SaveProxyInput {
  id: string;
  name: string;
  type: string;
  host: string;
  port: number;
  username?: string | null;
  jumpConnectionId?: string | null;
  credential?: string | null;
}

export interface PortForward {
  id: string;
  connectionId: string;
  type: string;
  bindHost: string;
  bindPort: number;
  destinationHost: string | null;
  destinationPort: number | null;
  autoStart: boolean;
  status: string | null;
  error: string | null;
}

export interface CommandSnippet {
  id: string;
  name: string;
  command: string;
  description: string;
  tags: string[];
  sortOrder: number;
}

export interface Folder {
  id: string;
  name: string;
  parentId: string | null;
  sortOrder: number;
}

export interface ConnectionDiagnostic {
  stage: string;
  ok: boolean;
  message: string;
  latencyMs?: number | null;
  fingerprint?: string | null;
  algorithm?: string | null;
}

export interface SshCertificateInfo {
  path: string;
  certificateType: string;
  keyId: string;
  serial: string;
  signingCa: string;
  validFrom: string;
  validTo: string;
  principals: string[];
  validNow: boolean;
  status: string;
}

export interface Fido2Identity {
  keyType: string;
  comment: string;
  fingerprint: string;
}

export interface TouchIdSyncStatus {
  supported: boolean;
  saved: boolean;
  message: string;
}

export interface TerminalSession {
  id: string;
  connectionId: string;
  sessionType: string;
  title: string;
  status: string;
  startedAt: string;
  lastError: string | null;
}

export interface TerminalOutput {
  sessionId: string;
  dataBase64: string;
}

export interface TerminalStatus {
  sessionId: string;
  status: string;
  lastError: string | null;
  attempt: number | null;
}

export interface SessionLogStatus {
  sessionId: string;
  active: boolean;
  path: string | null;
  format: string | null;
  lineTimestamps: boolean;
  startedAt: string | null;
  bytesWritten: number;
  error: string | null;
}

export interface BatchTargetResult {
  connectionId: string;
  name: string;
  status: string;
  stdout: string;
  stderr: string;
  exitCode: number | null;
  durationMs: number | null;
  error: string | null;
}

export interface BatchExecution {
  id: string;
  command: string;
  status: string;
  createdAt: string;
  targets: BatchTargetResult[];
}

export interface ExternalEditSession {
  id: string;
  remotePath: string;
  localPath: string;
  expectedModifiedAt: number | null;
  startedAt: string;
}

export interface ExternalEditSnapshot {
  id: string;
  content: string;
  expectedModifiedAt: number | null;
}

export interface OpenSshHost {
  alias: string;
  hostname: string;
  user: string | null;
  port: number;
  identityFile: string | null;
  proxyJump: string | null;
  source: string;
  warnings: string[];
}

export interface GeneratedSshKey {
  privateKeyPath: string;
  publicKeyPath: string;
  publicKey: string;
  fingerprint: string;
}

export interface ProtocolCapability {
  id: string;
  label: string;
  available: boolean;
  executable: string | null;
  message: string;
  securityWarning: string | null;
}

export interface ConnectionProtocolOptions {
  connectionId: string;
  agentForwarding: boolean;
  x11Enabled: boolean;
  moshEnabled: boolean;
  moshPortStart: number;
  moshPortEnd: number;
}

export interface AutomationPlan {
  id: string;
  name: string;
  connectionId: string;
  steps: AutomationStep[];
}

export interface AutomationStep {
  id: string;
  kind: string;
  command: string | null;
  pattern: string | null;
  timeoutSeconds: number | null;
  action: string | null;
  direction: string | null;
  localPath: string | null;
  remotePath: string | null;
}

export interface AutomationStepResult {
  stepId: string;
  kind: string;
  status: string;
  startedAt: string;
  durationMs: number;
  output: string;
  error: string | null;
}

export interface AutomationRun {
  runId: string;
  planId: string;
  status: string;
  currentStep: string | null;
  results: AutomationStepResult[];
}

export interface AutomationSchedule {
  id: string;
  plan: AutomationPlan;
  scheduleType: string;
  expression: string;
  enabled: boolean;
  misfirePolicy: string;
  nextRunAt: string | null;
  lastRunAt: string | null;
}

export interface PythonAutomationManifest {
  connectionId: string;
  permissions: string[];
  allowedLocalPaths: string[];
}

export interface PythonAutomationRequest {
  id: string;
  name: string;
  source: string;
  manifest: PythonAutomationManifest;
}

export interface PythonAutomationPreview {
  scriptHash: string;
  targetConnectionId: string;
  permissions: string[];
  steps: AutomationStep[];
  warnings: string[];
}

export interface SyncOptions {
  includeHosts: boolean;
  includePrivateKeyPaths: boolean;
  includeCredentials: boolean;
}

export interface SyncResult {
  path: string;
  conflictCopy: string | null;
  connectionCount: number;
  encrypted: boolean;
}

export interface WebDavProfile {
  id: string;
  name: string;
  url: string;
  username: string;
  hasCredential: boolean;
  syncOnStartup: boolean;
  hasSyncPassphrase: boolean;
  syncOptions: SyncOptions;
}

export interface SaveWebDavProfileInput {
  id: string;
  name: string;
  url: string;
  username: string;
  password?: string | null;
  syncOnStartup: boolean;
  syncOptions: SyncOptions;
  syncPassphrase?: string | null;
}

export interface WebDavSyncProgress {
  profileId: string;
  phase: string;
  transferredBytes: number;
  totalBytes: number;
}

export interface AiProviderProfile {
  id: string;
  name: string;
  endpoint: string;
  model: string;
  hasApiKey: boolean;
}

export interface SaveAiProviderInput {
  id: string;
  name: string;
  endpoint: string;
  model: string;
  apiKey?: string | null;
}

export interface AiPreviewInput {
  providerId: string;
  kind: string;
  content: string;
}

export interface AiRequestPreview {
  requestId: string;
  providerName: string;
  endpoint: string;
  model: string;
  kind: string;
  redactedContent: string;
  redactions: string[];
  expiresAt: string;
}

export interface AiAssistantResult {
  requestId: string;
  kind: string;
  model: string;
  content: string;
}

export interface PluginManifest {
  id: string;
  name: string;
  version: string;
  apiVersion: number;
  entrypoint: string;
  permissions: string[];
  networkDomains: string[];
  publisher: string | null;
  signature: string | null;
}

export interface PluginPermissionReport {
  manifest: PluginManifest;
  valid: boolean;
  signatureStatus: string;
  requestedPermissions: string[];
  defaultGrantedPermissions: string[];
  deniedPermissions: string[];
  warnings: string[];
}

export interface PluginInstallRecord {
  id: string;
  name: string;
  version: string;
  manifestPath: string;
  digest: string;
  entrypointDigest: string;
  publisherId: string | null;
  signatureStatus: string;
  requestedPermissions: string[];
  deniedPermissions: string[];
  grantedPermissions: string[];
  enabled: boolean;
  executable: boolean;
  installedAt: string;
  updatedAt: string;
}

export interface PluginPublisherRoot {
  id: string;
  name: string;
  publicKey: string;
  fingerprint: string;
  enabled: boolean;
  installedAt: string;
  updatedAt: string;
}

export interface PluginRunResult {
  pluginId: string;
  version: string;
  statusCode: number;
  fuelConsumed: number;
  durationMs: number;
}

export interface TeamWorkspace {
  id: string;
  name: string;
  localMemberId: string;
  localRole: string;
  keyEpoch: number;
  createdAt: string;
  updatedAt: string;
}

export interface TeamMember {
  id: string;
  workspaceId: string;
  displayName: string;
  role: string;
  status: string;
  joinedAt: string;
  updatedAt: string;
  removedAt: string | null;
}

export interface CreateTeamWorkspaceInput {
  name: string;
  ownerName: string;
}

export interface SaveTeamMemberInput {
  workspaceId: string;
  memberId: string | null;
  displayName: string;
  role: string;
}

export interface TeamPermissionReport {
  workspaceId: string;
  memberId: string;
  role: string;
  permissions: string[];
}

export interface TeamAuditEvent {
  id: string;
  workspaceId: string;
  actorMemberId: string;
  action: string;
  targetType: string;
  targetId: string;
  createdAt: string;
}

export interface PluginAuditEvent {
  id: string;
  pluginId: string;
  action: string;
  detail: string;
  digest: string;
  createdAt: string;
}

export interface RemoteFile {
  name: string;
  path: string;
  kind: string;
  size: number;
  modifiedAt: number | null;
  permissions: string;
  owner: number | null;
  group: number | null;
}

export interface TransferTask {
  id: string;
  sessionId: string;
  direction: string;
  source: string;
  destination: string;
  totalBytes: number;
  transferredBytes: number;
  status: string;
  conflictPolicy: string;
  error: string | null;
  createdAt: string;
}

export interface TransferInput {
  sessionId: string;
  direction: string;
  source: string;
  destination: string;
  conflictPolicy: string;
}

export interface ZmodemEvent {
  id: string;
  sessionId: string;
  direction: string;
  status: string;
  fileName: string | null;
  totalBytes: number | null;
  transferredBytes: number;
  error: string | null;
}

export interface SerialTransferEvent {
  id: string;
  sessionId: string;
  protocol: string;
  direction: string;
  status: string;
  fileName: string | null;
  totalBytes: number | null;
  transferredBytes: number;
  error: string | null;
}

export interface ProcessInfo {
  pid: number;
  startedAt: string;
  user: string;
  cpuPercent: number;
  memoryPercent: number;
  command: string;
}

export interface NetworkSocket {
  protocol: string;
  state: string;
  localAddress: string;
  peerAddress: string;
  process: string;
}

export interface NetworkSocketReport {
  items: NetworkSocket[];
  warning: string | null;
}

export interface NetworkDiagnosticResult {
  kind: string;
  target: string;
  output: string;
  durationMs: number;
}

export interface DiskInfo {
  filesystem: string;
  mountPoint: string;
  totalBytes: number;
  usedBytes: number;
  availableBytes: number;
  usedPercent: number;
}

export interface NetworkInfo {
  interfaceName: string;
  rxBytesPerSecond: number;
  txBytesPerSecond: number;
  rxTotalBytes: number;
  txTotalBytes: number;
}

export interface MonitorSnapshot {
  sessionId: string;
  timestamp: number;
  hostname: string;
  ip: string;
  uptimeSeconds: number;
  load: [number, number, number];
  cpuPercent: number;
  memoryUsedBytes: number;
  memoryTotalBytes: number;
  swapUsedBytes: number;
  swapTotalBytes: number;
  latencyMs: number | null;
  processes: ProcessInfo[];
  disks: DiskInfo[];
  networks: NetworkInfo[];
  warnings: string[];
}

export interface NetworkInterface {
  name: string;
  addresses: string[];
}

export interface SystemInfo {
  hostname: string;
  os: string;
  kernel: string;
  architecture: string;
  cpuModel: string;
  cpuCores: number;
  memoryTotalBytes: number;
  interfaces: NetworkInterface[];
  disks: DiskInfo[];
}

export interface RdpPreflight {
  available: boolean;
  executable: string | null;
  message: string;
}

export interface RdpConnectionOptions {
  connectionId: string;
  displayMode: string;
  displayId: number | null;
  scaleMode: string;
  quality: string;
  clipboard: boolean;
  audioMode: string;
  microphone: boolean;
  drivePath: string | null;
}

export interface RdpDisplay {
  id: number;
  name: string;
  width: number;
  height: number;
  primary: boolean;
}

export interface SerialDeviceInfo {
  path: string;
  kind: string;
  label: string;
  vendorId: number | null;
  productId: number | null;
  serialNumber: string | null;
  manufacturer: string | null;
  product: string | null;
}

export interface SerialConnectionOptions {
  connectionId: string;
  dataBits: number;
  parity: string;
  stopBits: number;
  flowControl: string;
  dtr: boolean;
  rts: boolean;
}

export interface TerminalPreferences {
  fontFamily: string;
  fontSize: number;
  lineHeight: number;
  scrollback: number;
  cursorStyle: string;
  cursorBlink: boolean;
  colorScheme: string;
}

export interface AppSettings {
  theme: string;
  monitorIntervalMs: number;
  rememberCommandHistory: boolean;
  confirmCloseActiveSession: boolean;
  showHiddenFiles: boolean;
  showWelcomeHelp: boolean;
  terminal: TerminalPreferences;
  terminalOverrides: Record<string, TerminalPreferences>;
}
