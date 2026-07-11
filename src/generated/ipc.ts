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

export interface ProcessInfo {
  pid: number;
  user: string;
  cpuPercent: number;
  memoryPercent: number;
  command: string;
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

export interface AppSettings {
  theme: string;
  monitorIntervalMs: number;
  rememberCommandHistory: boolean;
  confirmCloseActiveSession: boolean;
  showHiddenFiles: boolean;
  showWelcomeHelp: boolean;
}
