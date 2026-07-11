import type {
  BackgroundTask as GeneratedBackgroundTask,
  AppSettings as GeneratedAppSettings,
  CommandSnippet as GeneratedCommandSnippet,
  ConnectionDiagnostic as GeneratedConnectionDiagnostic,
  ConnectionProfile as GeneratedConnectionProfile,
  DiskInfo,
  Folder,
  MonitorSnapshot,
  NetworkInfo,
  PortForward as GeneratedPortForward,
  ProcessInfo,
  ProxyProfile as GeneratedProxyProfile,
  RdpPreflight,
  RemoteFile as GeneratedRemoteFile,
  SaveConnectionInput as GeneratedSaveConnectionInput,
  SaveProxyInput as GeneratedSaveProxyInput,
  SystemInfo,
  TerminalOutput,
  TerminalSession as GeneratedTerminalSession,
  TerminalStatus as GeneratedTerminalStatus,
  TransferInput as GeneratedTransferInput,
  TransferTask as GeneratedTransferTask,
} from "./generated/ipc";

export type Protocol = "ssh" | "rdp";
export type AuthType = "password" | "privateKey" | "sshAgent";
export type HostKeyPolicy = "strict" | "acceptNew";
export type SessionStatus = "connecting" | "online" | "reconnecting" | "failed" | "closed";
export type ProxyType = "socks5" | "http" | "sshJump";
export type TransferStatus = "queued" | "running" | "paused" | "completed" | "failed" | "cancelled";
export type TransferDirection = "upload" | "download";
export type ConflictPolicy = "ask" | "overwrite" | "skip" | "rename";
export type BackgroundTaskStatus = "queued" | "running" | "completed" | "failed" | "cancelled";
export type BackgroundTask = Omit<GeneratedBackgroundTask, "status"> & { status: BackgroundTaskStatus };

export type ConnectionProfile = Omit<GeneratedConnectionProfile, "protocol" | "authType" | "hostKeyPolicy"> & {
  protocol: Protocol;
  authType: AuthType;
  hostKeyPolicy: HostKeyPolicy;
};

export type SaveConnectionInput = Omit<GeneratedSaveConnectionInput, "folderId" | "protocol" | "authType" | "privateKeyPath" | "hostKeyPolicy" | "startupCommand" | "proxyId"> & {
  folderId: string | null;
  protocol: Protocol;
  authType: AuthType;
  privateKeyPath: string | null;
  hostKeyPolicy: HostKeyPolicy;
  startupCommand: string | null;
  proxyId: string | null;
};

export type ProxyProfile = Omit<GeneratedProxyProfile, "type"> & { type: ProxyType };
export type SaveProxyInput = Omit<GeneratedSaveProxyInput, "type"> & { type: ProxyType };
export type PortForward = Omit<GeneratedPortForward, "type" | "status"> & {
  type: "local" | "remote" | "dynamic";
  status: "stopped" | "running" | "failed" | null;
};
export type CommandSnippet = GeneratedCommandSnippet & { builtIn?: boolean };

export type ConnectionDiagnostic = Omit<GeneratedConnectionDiagnostic, "stage"> & {
  stage: "dns" | "tcp" | "proxy" | "hostKey" | "authentication" | "shell" | "complete";
};
export type TerminalSession = Omit<GeneratedTerminalSession, "status" | "sessionType"> & {
  status: SessionStatus;
  sessionType: "terminal" | "rdp";
};
export type TerminalStatus = Omit<GeneratedTerminalStatus, "status"> & { status: SessionStatus };
export type RemoteFile = Omit<GeneratedRemoteFile, "kind"> & { kind: "file" | "directory" | "symlink" | "other" };
export type TransferTask = Omit<GeneratedTransferTask, "direction" | "status" | "conflictPolicy"> & {
  direction: TransferDirection;
  status: TransferStatus;
  conflictPolicy: ConflictPolicy;
};
export type TransferInput = Omit<GeneratedTransferInput, "direction" | "conflictPolicy"> & {
  direction: TransferDirection;
  conflictPolicy: ConflictPolicy;
};
export type AppSettings = Omit<GeneratedAppSettings, "theme"> & {
  theme: "system" | "dark" | "light" | "highContrast";
};

export type { DiskInfo, Folder, MonitorSnapshot, NetworkInfo, ProcessInfo, RdpPreflight, SystemInfo, TerminalOutput };

export const defaultSettings: AppSettings = {
  theme: "system",
  monitorIntervalMs: 2000,
  rememberCommandHistory: true,
  confirmCloseActiveSession: true,
  showHiddenFiles: false,
  showWelcomeHelp: true,
};
