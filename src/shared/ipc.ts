import type { AppSnapshot, RemoteFileEntry, ServerMetric } from "../domain/models.js";

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
  updatedAt?: string;
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

export interface CollectMetricsRequest {
  ssh: SshSessionConfig;
}

export interface CollectMetricsResult {
  metrics: ServerMetric[];
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

export interface CNshellApi {
  getVersion: () => Promise<string>;
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
  };
  sftp: {
    listDirectory: (request: ListRemoteDirectoryRequest) => Promise<RemoteDirectoryListing>;
    transferFile: (request: TransferFileRequest) => Promise<TransferFileResult>;
  };
  metrics: {
    collect: (request: CollectMetricsRequest) => Promise<CollectMetricsResult>;
  };
  tunnels: {
    start: (request: StartTunnelRequest) => Promise<TunnelInfo>;
    stop: (id: string) => Promise<boolean>;
  };
}
