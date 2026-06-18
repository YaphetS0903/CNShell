import type { AppSnapshot, JumpHostConfig, RemoteFileEntry, RemoteProcess, ServerMetric, SystemInfo } from "../domain/models.js";

export type AppLanguage = "zh-CN" | "en-US";

export type TerminalSessionKind = "local" | "ssh";

export interface StartTerminalSessionRequest {
  id: string;
  kind: TerminalSessionKind;
  cols: number;
  rows: number;
  cwd?: string;
  ssh?: SshSessionConfig;
}

export interface TerminalSessionStarted {
  id: string;
  pid?: number;
}

export interface TerminalSessionResizeRequest {
  id: string;
  cols: number;
  rows: number;
}

export interface TerminalDataEvent {
  id: string;
  data: string;
}

export interface TerminalExitEvent {
  id: string;
  exitCode: number;
  signal?: number;
}

export interface TerminalErrorEvent {
  id: string;
  message: string;
}

export type HostKeyTrustStatus = "trusted" | "unknown" | "changed";

export interface HostKeyVerificationEvent {
  id: string;
  status: HostKeyTrustStatus;
  host: string;
  port: number;
  fingerprint: string;
  keyBase64: string;
  expectedFingerprint?: string;
}

export interface SshSessionConfig {
  connectionId: string;
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  passphrase?: string;
  useSavedCredential?: boolean;
  readyTimeout?: number;
  gateways?: JumpHostConfig[];
}

export interface CredentialSecret {
  password?: string;
  privateKey?: string;
  passphrase?: string;
}

export interface SaveCredentialRequest {
  connectionId: string;
  secret: CredentialSecret;
}

export interface CredentialStatus {
  connectionId: string;
  hasCredential: boolean;
  encryptionAvailable: boolean;
  protection: "system" | "master" | "none";
  vaultLocked: boolean;
  updatedAt?: string;
}

export interface CredentialVaultStatus {
  mode: "system" | "master";
  locked: boolean;
  encryptionAvailable: boolean;
  updatedAt?: string;
}

export interface EnableCredentialVaultRequest {
  masterPassword: string;
}

export interface UnlockCredentialVaultRequest {
  masterPassword: string;
}

export interface DisableCredentialVaultRequest {
  masterPassword?: string;
}

export interface ImportPrivateKeyResult {
  ok: boolean;
  path?: string;
  fileName?: string;
  privateKey?: string;
}

export interface ListRemoteDirectoryRequest {
  ssh: SshSessionConfig;
  path: string;
}

export interface RemoteDirectoryListing {
  path: string;
  entries: RemoteFileEntry[];
}

export interface TransferFileRequest {
  ssh: SshSessionConfig;
  direction: "upload" | "download";
  localPath: string;
  remotePath: string;
}

export interface TransferFileResult {
  ok: boolean;
}

export interface ReadRemoteFileRequest {
  ssh: SshSessionConfig;
  remotePath: string;
}

export interface ReadRemoteFileResult {
  remotePath: string;
  content: string;
}

export interface WriteRemoteFileRequest {
  ssh: SshSessionConfig;
  remotePath: string;
  content: string;
}

export interface WriteRemoteFileResult {
  ok: boolean;
}

export interface CreateRemoteDirectoryRequest {
  ssh: SshSessionConfig;
  remotePath: string;
}

export interface RenameRemotePathRequest {
  ssh: SshSessionConfig;
  oldPath: string;
  newPath: string;
}

export interface DeleteRemotePathRequest {
  ssh: SshSessionConfig;
  remotePath: string;
}

export interface RemotePathOperationResult {
  ok: boolean;
}

export interface CollectMetricsRequest {
  ssh: SshSessionConfig;
}

export interface CollectMetricsResult {
  metrics: ServerMetric[];
  systemInfo?: SystemInfo;
}

export interface ListProcessesRequest {
  ssh: SshSessionConfig;
}

export interface ListProcessesResult {
  processes: RemoteProcess[];
}

export interface KillProcessRequest {
  ssh: SshSessionConfig;
  pid: number;
  signal?: "TERM" | "KILL";
}

export interface KillProcessResult {
  ok: boolean;
}

export type TunnelMode = "local" | "remote" | "dynamic";

export interface StartTunnelRequest {
  id: string;
  ssh: SshSessionConfig;
  mode: TunnelMode;
  bindHost: string;
  bindPort: number;
  targetHost?: string;
  targetPort?: number;
}

export interface TunnelInfo {
  id: string;
  mode: TunnelMode;
  bindHost: string;
  bindPort: number;
  targetHost?: string;
  targetPort?: number;
  status: "starting" | "running" | "stopped" | "error";
  message?: string;
}

export interface StartRelayRequest {
  id: string;
  ssh: SshSessionConfig;
  relayHost: string;
  relayPort: number;
  targetHost: string;
  targetPort: number;
}

export interface RelayInfo {
  id: string;
  relayHost: string;
  relayPort: number;
  targetHost: string;
  targetPort: number;
  status: "starting" | "running" | "stopped" | "error";
  message?: string;
}

export interface ReadSessionLogRequest {
  sessionId: string;
  query?: string;
  limit?: number;
}

export interface ReadSessionLogResult {
  lines: string[];
}

export interface ReadAuditLogRequest {
  query?: string;
  limit?: number;
}

export interface ReadAuditLogResult {
  lines: string[];
}

export interface ReadErrorReportRequest {
  query?: string;
  limit?: number;
}

export interface ReadErrorReportResult {
  lines: string[];
}

export interface RendererErrorReportRequest {
  message: string;
  stack?: string;
  componentStack?: string;
}

export interface OpenRdpRequest {
  host: string;
  port: number;
  username?: string;
}

export interface OpenRdpResult {
  ok: boolean;
}

export interface ExportCloudSyncRequest {
  snapshot: AppSnapshot;
}

export interface CloudSyncResult {
  ok: boolean;
  path?: string;
  importedSnapshot?: AppSnapshot;
}

export type UpdateStatusState =
  | "idle"
  | "checking"
  | "available"
  | "not-available"
  | "downloading"
  | "downloaded"
  | "error";

export interface UpdateStatus {
  state: UpdateStatusState;
  channel: string;
  version?: string;
  message?: string;
  percent?: number;
}

export interface CheckForUpdatesRequest {
  channel?: string;
}

export interface CNshellApi {
  getVersion: () => Promise<string>;
  setLanguage: (language: AppLanguage) => Promise<boolean>;
  workspace: {
    load: () => Promise<AppSnapshot | null>;
    save: (snapshot: AppSnapshot) => Promise<boolean>;
  };
  terminal: {
    start: (request: StartTerminalSessionRequest) => Promise<TerminalSessionStarted>;
    write: (id: string, data: string) => Promise<boolean>;
    resize: (request: TerminalSessionResizeRequest) => Promise<boolean>;
    stop: (id: string) => Promise<boolean>;
    trustHost: (event: HostKeyVerificationEvent) => Promise<boolean>;
    onData: (callback: (event: TerminalDataEvent) => void) => () => void;
    onExit: (callback: (event: TerminalExitEvent) => void) => () => void;
    onError: (callback: (event: TerminalErrorEvent) => void) => () => void;
    onHostKeyVerification: (callback: (event: HostKeyVerificationEvent) => void) => () => void;
  };
  credentials: {
    status: (connectionId: string) => Promise<CredentialStatus>;
    save: (request: SaveCredentialRequest) => Promise<CredentialStatus>;
    delete: (connectionId: string) => Promise<CredentialStatus>;
    vaultStatus: () => Promise<CredentialVaultStatus>;
    enableVault: (request: EnableCredentialVaultRequest) => Promise<CredentialVaultStatus>;
    unlockVault: (request: UnlockCredentialVaultRequest) => Promise<CredentialVaultStatus>;
    disableVault: (request: DisableCredentialVaultRequest) => Promise<CredentialVaultStatus>;
    lockVault: () => Promise<CredentialVaultStatus>;
    importPrivateKey: () => Promise<ImportPrivateKeyResult>;
  };
  sftp: {
    listDirectory: (request: ListRemoteDirectoryRequest) => Promise<RemoteDirectoryListing>;
    transferFile: (request: TransferFileRequest) => Promise<TransferFileResult>;
    readFile: (request: ReadRemoteFileRequest) => Promise<ReadRemoteFileResult>;
    writeFile: (request: WriteRemoteFileRequest) => Promise<WriteRemoteFileResult>;
    createDirectory: (request: CreateRemoteDirectoryRequest) => Promise<RemotePathOperationResult>;
    renamePath: (request: RenameRemotePathRequest) => Promise<RemotePathOperationResult>;
    deletePath: (request: DeleteRemotePathRequest) => Promise<RemotePathOperationResult>;
  };
  metrics: {
    collect: (request: CollectMetricsRequest) => Promise<CollectMetricsResult>;
    listProcesses: (request: ListProcessesRequest) => Promise<ListProcessesResult>;
    killProcess: (request: KillProcessRequest) => Promise<KillProcessResult>;
  };
  tunnels: {
    start: (request: StartTunnelRequest) => Promise<TunnelInfo>;
    stop: (id: string) => Promise<boolean>;
  };
  relay: {
    start: (request: StartRelayRequest) => Promise<RelayInfo>;
    stop: (id: string) => Promise<boolean>;
  };
  logs: {
    readSession: (request: ReadSessionLogRequest) => Promise<ReadSessionLogResult>;
    readAudit: (request: ReadAuditLogRequest) => Promise<ReadAuditLogResult>;
    readErrors: (request: ReadErrorReportRequest) => Promise<ReadErrorReportResult>;
    reportRendererError: (request: RendererErrorReportRequest) => Promise<boolean>;
  };
  rdp: {
    open: (request: OpenRdpRequest) => Promise<OpenRdpResult>;
  };
  cloudSync: {
    exportSettings: (request: ExportCloudSyncRequest) => Promise<CloudSyncResult>;
    importSettings: () => Promise<CloudSyncResult>;
  };
  updates: {
    status: () => Promise<UpdateStatus>;
    check: (request?: CheckForUpdatesRequest) => Promise<UpdateStatus>;
    quitAndInstall: () => Promise<boolean>;
    onStatus: (callback: (status: UpdateStatus) => void) => () => void;
  };
}
