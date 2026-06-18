import type {
  AppSnapshot,
  JumpHostConfig
} from "../src/domain/models.js";
import type {
  CheckForUpdatesRequest,
  CollectMetricsRequest,
  CreateRemoteDirectoryRequest,
  DeleteRemotePathRequest,
  DisableCredentialVaultRequest,
  EnableCredentialVaultRequest,
  ExportCloudSyncRequest,
  HostKeyVerificationEvent,
  KillProcessRequest,
  ListProcessesRequest,
  ListRemoteDirectoryRequest,
  OpenRdpRequest,
  ReadAuditLogRequest,
  ReadErrorReportRequest,
  ReadRemoteFileRequest,
  ReadSessionLogRequest,
  SaveCredentialRequest,
  RendererErrorReportRequest,
  RenameRemotePathRequest,
  SshSessionConfig,
  StartRelayRequest,
  StartTerminalSessionRequest,
  StartTunnelRequest,
  TerminalSessionResizeRequest,
  TransferFileRequest,
  UnlockCredentialVaultRequest,
  WriteRemoteFileRequest
} from "../src/shared/ipc.js";

const MAX_ID_LENGTH = 160;
const MAX_HOST_LENGTH = 255;
const MAX_PATH_LENGTH = 4096;
const MAX_SECRET_LENGTH = 512 * 1024;
const MAX_TERMINAL_WRITE_LENGTH = 1024 * 1024;
const MAX_REMOTE_FILE_WRITE_LENGTH = 5 * 1024 * 1024;
const MAX_LOG_QUERY_LENGTH = 200;

type UnknownRecord = Record<string, unknown>;

export class IpcValidationError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "IpcValidationError";
  }
}

function assertRecord(value: unknown, name: string): UnknownRecord {
  if (!value || typeof value !== "object" || Array.isArray(value)) {
    throw new IpcValidationError(`${name} must be an object.`);
  }

  return value as UnknownRecord;
}

function assertString(value: unknown, name: string, maxLength: number, allowEmpty = false) {
  if (typeof value !== "string") {
    throw new IpcValidationError(`${name} must be a string.`);
  }

  const trimmed = value.trim();
  if (!allowEmpty && trimmed.length === 0) {
    throw new IpcValidationError(`${name} is required.`);
  }

  if (value.length > maxLength) {
    throw new IpcValidationError(`${name} is too long.`);
  }

  return value;
}

function assertOptionalString(value: unknown, name: string, maxLength: number) {
  if (value === undefined) {
    return undefined;
  }

  return assertString(value, name, maxLength, true);
}

function assertBoolean(value: unknown, name: string) {
  if (value === undefined) {
    return undefined;
  }

  if (typeof value !== "boolean") {
    throw new IpcValidationError(`${name} must be a boolean.`);
  }

  return value;
}

function assertInteger(value: unknown, name: string, min: number, max: number) {
  if (!Number.isInteger(value) || Number(value) < min || Number(value) > max) {
    throw new IpcValidationError(`${name} must be an integer between ${min} and ${max}.`);
  }

  return Number(value);
}

function assertPort(value: unknown, name: string) {
  return assertInteger(value, name, 1, 65535);
}

function assertEnum<T extends string>(value: unknown, name: string, options: readonly T[]) {
  if (typeof value !== "string" || !options.includes(value as T)) {
    throw new IpcValidationError(`${name} is invalid.`);
  }

  return value as T;
}

function assertArray(value: unknown, name: string, maxLength: number) {
  if (!Array.isArray(value)) {
    throw new IpcValidationError(`${name} must be an array.`);
  }

  if (value.length > maxLength) {
    throw new IpcValidationError(`${name} has too many items.`);
  }

  return value;
}

function validateId(value: unknown, name = "id") {
  return assertString(value, name, MAX_ID_LENGTH);
}

function validatePath(value: unknown, name: string) {
  return assertString(value, name, MAX_PATH_LENGTH);
}

function validateGateway(value: unknown): JumpHostConfig {
  const gateway = assertRecord(value, "gateway");
  return {
    id: validateId(gateway.id, "gateway.id"),
    name: assertString(gateway.name, "gateway.name", 120),
    host: assertString(gateway.host, "gateway.host", MAX_HOST_LENGTH),
    port: assertPort(gateway.port, "gateway.port"),
    username: assertString(gateway.username, "gateway.username", 120)
  };
}

function validateSshConfig(value: unknown): SshSessionConfig {
  const ssh = assertRecord(value, "ssh");
  const gateways = ssh.gateways === undefined ? undefined : assertArray(ssh.gateways, "ssh.gateways", 8).map(validateGateway);
  return {
    connectionId: validateId(ssh.connectionId, "ssh.connectionId"),
    host: assertString(ssh.host, "ssh.host", MAX_HOST_LENGTH),
    port: assertPort(ssh.port, "ssh.port"),
    username: assertString(ssh.username, "ssh.username", 120),
    password: assertOptionalString(ssh.password, "ssh.password", MAX_SECRET_LENGTH),
    privateKey: assertOptionalString(ssh.privateKey, "ssh.privateKey", MAX_SECRET_LENGTH),
    passphrase: assertOptionalString(ssh.passphrase, "ssh.passphrase", MAX_SECRET_LENGTH),
    useSavedCredential: assertBoolean(ssh.useSavedCredential, "ssh.useSavedCredential"),
    readyTimeout:
      ssh.readyTimeout === undefined ? undefined : assertInteger(ssh.readyTimeout, "ssh.readyTimeout", 1000, 120000),
    gateways
  };
}

export function validateAppSnapshot(value: unknown): AppSnapshot {
  const snapshot = assertRecord(value, "snapshot");
  assertArray(snapshot.connections, "snapshot.connections", 500);
  assertArray(snapshot.sessions, "snapshot.sessions", 500);
  assertArray(snapshot.quickCommands, "snapshot.quickCommands", 500);
  assertArray(snapshot.remoteFiles, "snapshot.remoteFiles", 5000);
  assertArray(snapshot.serverMetrics, "snapshot.serverMetrics", 200);
  if (snapshot.systemInfo !== undefined) {
    assertRecord(snapshot.systemInfo, "snapshot.systemInfo");
  }
  if (snapshot.keyMappingProfiles !== undefined) {
    assertArray(snapshot.keyMappingProfiles, "snapshot.keyMappingProfiles", 200);
  }
  if (snapshot.scriptRecordings !== undefined) {
    assertArray(snapshot.scriptRecordings, "snapshot.scriptRecordings", 200);
  }
  if (snapshot.remoteProcesses !== undefined) {
    assertArray(snapshot.remoteProcesses, "snapshot.remoteProcesses", 1000);
  }

  return value as AppSnapshot;
}

export function validateConnectionId(value: unknown) {
  return validateId(value, "connectionId");
}

export function validateSessionId(value: unknown) {
  return validateId(value, "sessionId");
}

export function validateIpcId(value: unknown) {
  return validateId(value);
}

export function validateTerminalWrite(id: unknown, data: unknown) {
  return {
    id: validateId(id),
    data: assertString(data, "data", MAX_TERMINAL_WRITE_LENGTH, true)
  };
}

export function validateStartTerminalSession(value: unknown): StartTerminalSessionRequest {
  const request = assertRecord(value, "request");
  const kind = assertEnum(request.kind, "kind", ["local", "ssh"] as const);
  const ssh = kind === "ssh" ? validateSshConfig(request.ssh) : undefined;
  if (kind === "ssh" && !ssh) {
    throw new IpcValidationError("ssh is required for SSH terminal sessions.");
  }

  return {
    id: validateId(request.id),
    kind,
    cols: assertInteger(request.cols, "cols", 2, 1000),
    rows: assertInteger(request.rows, "rows", 2, 1000),
    cwd: assertOptionalString(request.cwd, "cwd", MAX_PATH_LENGTH),
    ssh
  };
}

export function validateTerminalResize(value: unknown): TerminalSessionResizeRequest {
  const request = assertRecord(value, "request");
  return {
    id: validateId(request.id),
    cols: assertInteger(request.cols, "cols", 2, 1000),
    rows: assertInteger(request.rows, "rows", 2, 1000)
  };
}

export function validateHostKeyVerification(value: unknown): HostKeyVerificationEvent {
  const event = assertRecord(value, "event");
  return {
    id: validateId(event.id),
    status: assertEnum(event.status, "status", ["trusted", "unknown", "changed"] as const),
    host: assertString(event.host, "host", MAX_HOST_LENGTH),
    port: assertPort(event.port, "port"),
    fingerprint: assertString(event.fingerprint, "fingerprint", 160),
    keyBase64: assertString(event.keyBase64, "keyBase64", MAX_SECRET_LENGTH),
    expectedFingerprint: assertOptionalString(event.expectedFingerprint, "expectedFingerprint", 160)
  };
}

export function validateSaveCredential(value: unknown): SaveCredentialRequest {
  const request = assertRecord(value, "request");
  const secret = assertRecord(request.secret, "secret");
  return {
    connectionId: validateConnectionId(request.connectionId),
    secret: {
      password: assertOptionalString(secret.password, "secret.password", MAX_SECRET_LENGTH),
      privateKey: assertOptionalString(secret.privateKey, "secret.privateKey", MAX_SECRET_LENGTH),
      passphrase: assertOptionalString(secret.passphrase, "secret.passphrase", MAX_SECRET_LENGTH)
    }
  };
}

export function validateEnableVault(value: unknown): EnableCredentialVaultRequest {
  const request = assertRecord(value, "request");
  return {
    masterPassword: assertString(request.masterPassword, "masterPassword", MAX_SECRET_LENGTH)
  };
}

export function validateUnlockVault(value: unknown): UnlockCredentialVaultRequest {
  const request = assertRecord(value, "request");
  return {
    masterPassword: assertString(request.masterPassword, "masterPassword", MAX_SECRET_LENGTH)
  };
}

export function validateDisableVault(value: unknown): DisableCredentialVaultRequest {
  const request = assertRecord(value, "request");
  return {
    masterPassword: assertOptionalString(request.masterPassword, "masterPassword", MAX_SECRET_LENGTH)
  };
}

export function validateListRemoteDirectory(value: unknown): ListRemoteDirectoryRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    path: validatePath(request.path, "path")
  };
}

export function validateTransferFile(value: unknown): TransferFileRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    direction: assertEnum(request.direction, "direction", ["upload", "download"] as const),
    localPath: validatePath(request.localPath, "localPath"),
    remotePath: validatePath(request.remotePath, "remotePath")
  };
}

export function validateReadRemoteFile(value: unknown): ReadRemoteFileRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    remotePath: validatePath(request.remotePath, "remotePath")
  };
}

export function validateWriteRemoteFile(value: unknown): WriteRemoteFileRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    remotePath: validatePath(request.remotePath, "remotePath"),
    content: assertString(request.content, "content", MAX_REMOTE_FILE_WRITE_LENGTH, true)
  };
}

export function validateCreateRemoteDirectory(value: unknown): CreateRemoteDirectoryRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    remotePath: validatePath(request.remotePath, "remotePath")
  };
}

export function validateRenameRemotePath(value: unknown): RenameRemotePathRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    oldPath: validatePath(request.oldPath, "oldPath"),
    newPath: validatePath(request.newPath, "newPath")
  };
}

export function validateDeleteRemotePath(value: unknown): DeleteRemotePathRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    remotePath: validatePath(request.remotePath, "remotePath")
  };
}

export function validateCollectMetrics(value: unknown): CollectMetricsRequest {
  const request = assertRecord(value, "request");
  return { ssh: validateSshConfig(request.ssh) };
}

export function validateListProcesses(value: unknown): ListProcessesRequest {
  const request = assertRecord(value, "request");
  return { ssh: validateSshConfig(request.ssh) };
}

export function validateKillProcess(value: unknown): KillProcessRequest {
  const request = assertRecord(value, "request");
  return {
    ssh: validateSshConfig(request.ssh),
    pid: assertInteger(request.pid, "pid", 1, 4_194_304),
    signal: request.signal === undefined ? undefined : assertEnum(request.signal, "signal", ["TERM", "KILL"] as const)
  };
}

export function validateStartTunnel(value: unknown): StartTunnelRequest {
  const request = assertRecord(value, "request");
  const mode = assertEnum(request.mode, "mode", ["local", "remote", "dynamic"] as const);
  return {
    id: validateId(request.id),
    ssh: validateSshConfig(request.ssh),
    mode,
    bindHost: assertString(request.bindHost, "bindHost", MAX_HOST_LENGTH),
    bindPort: assertPort(request.bindPort, "bindPort"),
    targetHost: mode === "dynamic" ? undefined : assertString(request.targetHost, "targetHost", MAX_HOST_LENGTH),
    targetPort: mode === "dynamic" ? undefined : assertPort(request.targetPort, "targetPort")
  };
}

export function validateStartRelay(value: unknown): StartRelayRequest {
  const request = assertRecord(value, "request");
  return {
    id: validateId(request.id),
    ssh: validateSshConfig(request.ssh),
    relayHost: assertString(request.relayHost, "relayHost", MAX_HOST_LENGTH),
    relayPort: assertPort(request.relayPort, "relayPort"),
    targetHost: assertString(request.targetHost, "targetHost", MAX_HOST_LENGTH),
    targetPort: assertPort(request.targetPort, "targetPort")
  };
}

export function validateReadSessionLog(value: unknown): ReadSessionLogRequest {
  const request = assertRecord(value, "request");
  return {
    sessionId: validateSessionId(request.sessionId),
    query: assertOptionalString(request.query, "query", MAX_LOG_QUERY_LENGTH),
    limit: request.limit === undefined ? undefined : assertInteger(request.limit, "limit", 1, 5000)
  };
}

export function validateReadAuditLog(value: unknown): ReadAuditLogRequest {
  const request = assertRecord(value, "request");
  return {
    query: assertOptionalString(request.query, "query", MAX_LOG_QUERY_LENGTH),
    limit: request.limit === undefined ? undefined : assertInteger(request.limit, "limit", 1, 5000)
  };
}

export function validateReadErrorReport(value: unknown): ReadErrorReportRequest {
  const request = assertRecord(value, "request");
  return {
    query: assertOptionalString(request.query, "query", MAX_LOG_QUERY_LENGTH),
    limit: request.limit === undefined ? undefined : assertInteger(request.limit, "limit", 1, 5000)
  };
}

export function validateRendererErrorReport(value: unknown): RendererErrorReportRequest {
  const request = assertRecord(value, "request");
  return {
    message: assertString(request.message, "message", 1000),
    stack: assertOptionalString(request.stack, "stack", 8000),
    componentStack: assertOptionalString(request.componentStack, "componentStack", 8000)
  };
}

export function validateOpenRdp(value: unknown): OpenRdpRequest {
  const request = assertRecord(value, "request");
  return {
    host: assertString(request.host, "host", MAX_HOST_LENGTH),
    port: assertPort(request.port || 3389, "port"),
    username: assertOptionalString(request.username, "username", 120)
  };
}

export function validateExportCloudSync(value: unknown): ExportCloudSyncRequest {
  const request = assertRecord(value, "request");
  return {
    snapshot: validateAppSnapshot(request.snapshot)
  };
}

export function validateCheckForUpdates(value: unknown): CheckForUpdatesRequest {
  if (value === undefined) {
    return {};
  }

  const request = assertRecord(value, "request");
  return {
    channel: assertOptionalString(request.channel, "channel", 40)
  };
}
