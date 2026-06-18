export type ConnectionProtocol = "ssh" | "rdp" | "local";

export type AuthMethod = "password" | "privateKey" | "agent";

export interface ConnectionProfile {
  id: string;
  name: string;
  group: string;
  protocol: ConnectionProtocol;
  host: string;
  port: number;
  username: string;
  authMethod: AuthMethod;
  color: string;
  tags: string[];
  lastConnectedAt?: string;
  gateways?: JumpHostConfig[];
}

export interface JumpHostConfig {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
}

export type SessionStatus = "connected" | "connecting" | "disconnected" | "error";

export interface SessionTab {
  id: string;
  connectionId: string;
  title: string;
  cwd: string;
  status: SessionStatus;
  startedAt: string;
}

export interface QuickCommand {
  id: string;
  title: string;
  command: string;
  scope: "global" | "group" | "connection";
}

export interface KeyMappingProfile {
  id: string;
  name: string;
  enabled: boolean;
  rules: KeyMappingRule[];
}

export interface KeyMappingRule {
  id: string;
  key: string;
  send: string;
  description: string;
  enabled: boolean;
}

export interface RemoteFileEntry {
  id: string;
  name: string;
  path: string;
  type: "file" | "directory" | "symlink";
  size: number;
  modifiedAt: string;
  mode: string;
}

export type TransferDirection = "upload" | "download";

export type TransferStatus = "queued" | "running" | "completed" | "error";

export interface TransferJob {
  id: string;
  direction: TransferDirection;
  localPath: string;
  remotePath: string;
  status: TransferStatus;
  message?: string;
}

export interface ServerMetric {
  label: string;
  value: number;
  unit: "%" | "MB" | "GB" | "ms";
  trend: "up" | "down" | "flat";
}

export interface AppSnapshot {
  connections: ConnectionProfile[];
  sessions: SessionTab[];
  quickCommands: QuickCommand[];
  keyMappingProfiles: KeyMappingProfile[];
  remoteFiles: RemoteFileEntry[];
  serverMetrics: ServerMetric[];
}
