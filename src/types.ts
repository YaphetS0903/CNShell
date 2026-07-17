import type {
  BackgroundTask as GeneratedBackgroundTask,
  BatchExecution,
  BatchTargetResult,
  AppSettings as GeneratedAppSettings,
  CommandSnippet as GeneratedCommandSnippet,
  ConnectionDiagnostic as GeneratedConnectionDiagnostic,
  ConnectionProfile as GeneratedConnectionProfile,
  DiskInfo,
  ExternalEditSession,
  ExternalEditSnapshot,
  Folder,
  GeneratedSshKey,
  MonitorSnapshot,
  NetworkInfo,
  NetworkDiagnosticResult,
  NetworkSocket,
  NetworkSocketReport,
  OpenSshHost,
  PortForward as GeneratedPortForward,
  ProcessInfo,
  ProtocolCapability,
  ConnectionProtocolOptions,
  AutomationPlan,
  AutomationSchedule,
  AutomationStep,
  AutomationRun,
  AutomationStepResult,
  PythonAutomationManifest,
  PythonAutomationPreview,
  PythonAutomationRequest,
  SaveWebDavProfileInput,
  WebDavProfile,
  WebDavSyncProgress,
  AiAssistantResult,
  AiPreviewInput,
  AiProviderProfile,
  AiRequestPreview,
  SaveAiProviderInput,
  PluginPermissionReport,
  PluginInstallRecord,
  PluginAuditEvent,
  PluginPublisherRoot,
  PluginRunResult,
  PluginRunInput,
  PluginCredentialProxyRequest,
  PluginTerminalInputRequest,
  CreateTeamWorkspaceInput,
  SaveTeamMemberInput,
  TeamAuditEvent,
  TeamMember,
  TeamPermissionReport,
  TeamWorkspace,
  TeamDevice,
  TeamShareExportInput,
  TeamSharePreview,
  TeamTerminalClientRoom,
  TeamTerminalEncryptedFrame,
  TeamTerminalFrame,
  TeamTerminalInvitation,
  TeamTerminalRoom,
  TeamTerminalParticipant,
  TeamControlLease,
  AcceptTeamRelayInvitationInput,
  CreateTeamRelayInvitationInput,
  SaveTeamRelayProfileInput,
  ResendTeamRelayVerificationInput,
  TeamRelayAccountInput,
  TeamRelayAccountRegistration,
  TeamRelayInvitation,
  TeamRelayProfile,
  TeamRelayWorkspaceBinding,
  TeamRelayTerminalEvent,
  TeamRelayTerminalInvitation,
  TeamRelayTerminalSession,
  UpdateTeamRelayMemberInput,
  VerifyTeamRelayAccountInput,
  SyncOptions,
  SyncResult,
  ProxyProfile as GeneratedProxyProfile,
  RdpPreflight,
  RdpConnectionOptions as GeneratedRdpConnectionOptions,
  RdpDisplay,
  SerialDeviceInfo,
  SerialConnectionOptions,
  SerialTransferEvent,
  SshCertificateInfo,
  Fido2Identity,
  PlatformCapabilities,
  TouchIdSyncStatus,
  SessionLogStatus,
  RemoteFile as GeneratedRemoteFile,
  SaveConnectionInput as GeneratedSaveConnectionInput,
  SaveProxyInput as GeneratedSaveProxyInput,
  SystemInfo,
  TerminalOutput,
  TerminalSession as GeneratedTerminalSession,
  TerminalStatus as GeneratedTerminalStatus,
  TransferInput as GeneratedTransferInput,
  TransferTask as GeneratedTransferTask,
  ZmodemEvent as GeneratedZmodemEvent,
} from "./generated/ipc";

export type Protocol = "ssh" | "rdp" | "local" | "telnet" | "serial";
export type AuthType = "none" | "password" | "privateKey" | "sshCertificate" | "sshAgent" | "fido2Agent";
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

export type SaveConnectionInput = Omit<GeneratedSaveConnectionInput, "folderId" | "protocol" | "authType" | "privateKeyPath" | "certificatePath" | "hostKeyPolicy" | "startupCommand" | "proxyId"> & {
  folderId: string | null;
  protocol: Protocol;
  authType: AuthType;
  privateKeyPath: string | null;
  certificatePath: string | null;
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
export type { Fido2Identity, PlatformCapabilities, TouchIdSyncStatus };
export type RdpConnectionOptions = Omit<GeneratedRdpConnectionOptions,"displayMode"|"scaleMode"|"quality"|"audioMode"> & {
  displayMode:"window"|"fullscreen";
  scaleMode:"dynamic"|"fit"|"native";
  quality:"auto"|"lowBandwidth"|"balanced"|"highQuality";
  audioMode:"off"|"local"|"remote";
};

export type ConnectionDiagnostic = Omit<GeneratedConnectionDiagnostic, "stage"> & {
  stage: "dns" | "tcp" | "proxy" | "hostKey" | "authentication" | "shell" | "complete";
};
export type TerminalSession = Omit<GeneratedTerminalSession, "status" | "sessionType"> & {
  status: SessionStatus;
  sessionType: "terminal" | "mosh" | "rdp" | "local" | "telnet" | "serial";
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
export type ZmodemEvent = Omit<GeneratedZmodemEvent, "direction" | "status"> & {
  direction: "upload" | "download";
  status: "awaitingAuthorization" | "running" | "completed" | "failed" | "cancelled";
};
export type TerminalFontFamily = "system" | "menlo" | "monaco" | "courier";
export type TerminalCursorStyle = "block" | "underline" | "bar";
export type TerminalColorScheme = "cnshell" | "classic" | "solarizedDark" | "light";
export type TerminalPreferences = Omit<import("./generated/ipc").TerminalPreferences, "fontFamily" | "cursorStyle" | "colorScheme"> & {
  fontFamily: TerminalFontFamily;
  cursorStyle: TerminalCursorStyle;
  colorScheme: TerminalColorScheme;
};
export type AppSettings = Omit<GeneratedAppSettings, "theme" | "terminal" | "terminalOverrides"> & {
  theme: "system" | "dark" | "light" | "highContrast";
  terminal: TerminalPreferences;
  terminalOverrides: Record<string, TerminalPreferences>;
};

export type { AcceptTeamRelayInvitationInput, AiAssistantResult, AiPreviewInput, AiProviderProfile, AiRequestPreview, AutomationPlan, AutomationRun, AutomationSchedule, AutomationStep, AutomationStepResult, BatchExecution, BatchTargetResult, ConnectionProtocolOptions, CreateTeamRelayInvitationInput, CreateTeamWorkspaceInput, DiskInfo, ExternalEditSession, ExternalEditSnapshot, Folder, GeneratedSshKey, MonitorSnapshot, NetworkDiagnosticResult, NetworkInfo, NetworkSocket, NetworkSocketReport, OpenSshHost, PluginPermissionReport, PluginInstallRecord, PluginAuditEvent, PluginPublisherRoot, PluginRunInput, PluginRunResult, PluginCredentialProxyRequest, PluginTerminalInputRequest, ProcessInfo, ProtocolCapability, PythonAutomationManifest, PythonAutomationPreview, PythonAutomationRequest, RdpDisplay, RdpPreflight, ResendTeamRelayVerificationInput, SaveTeamMemberInput, SaveTeamRelayProfileInput, SerialDeviceInfo, SerialConnectionOptions, SerialTransferEvent, SaveAiProviderInput, SaveWebDavProfileInput, SessionLogStatus, SshCertificateInfo, SyncOptions, SyncResult, SystemInfo, TeamAuditEvent, TeamControlLease, TeamDevice, TeamMember, TeamPermissionReport, TeamRelayAccountInput, TeamRelayAccountRegistration, TeamRelayInvitation, TeamRelayProfile, TeamRelayTerminalEvent, TeamRelayTerminalInvitation, TeamRelayTerminalSession, TeamRelayWorkspaceBinding, TeamShareExportInput, TeamSharePreview, TeamTerminalClientRoom, TeamTerminalEncryptedFrame, TeamTerminalFrame, TeamTerminalInvitation, TeamTerminalParticipant, TeamTerminalRoom, TeamWorkspace, TerminalOutput, UpdateTeamRelayMemberInput, VerifyTeamRelayAccountInput, WebDavProfile, WebDavSyncProgress };

export const defaultSettings: AppSettings = {
  theme: "system",
  monitorIntervalMs: 2000,
  rememberCommandHistory: true,
  confirmCloseActiveSession: true,
  showHiddenFiles: false,
  showWelcomeHelp: true,
  terminal: {
    fontFamily: "system",
    fontSize: 13,
    lineHeight: 1.25,
    scrollback: 10000,
    cursorStyle: "bar",
    cursorBlink: true,
    colorScheme: "cnshell",
  },
  terminalOverrides: {},
};

export function normalizeAppSettings(value: Partial<AppSettings> | null | undefined): AppSettings {
  return {
    ...defaultSettings,
    ...(value ?? {}),
    terminal: { ...defaultSettings.terminal, ...(value?.terminal ?? {}) },
    terminalOverrides: value?.terminalOverrides ?? {},
  };
}
