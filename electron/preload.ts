import { contextBridge, ipcRenderer } from "electron";
import type {
  CNshellApi,
  CollectMetricsRequest,
  CollectMetricsResult,
  CredentialStatus,
  SaveCredentialRequest,
  HostKeyVerificationEvent,
  KillProcessRequest,
  KillProcessResult,
  ListRemoteDirectoryRequest,
  ListProcessesRequest,
  ListProcessesResult,
  OpenRdpRequest,
  OpenRdpResult,
  RemoteDirectoryListing,
  ReadRemoteFileRequest,
  ReadRemoteFileResult,
  ReadSessionLogRequest,
  ReadSessionLogResult,
  StartTerminalSessionRequest,
  TerminalDataEvent,
  TerminalErrorEvent,
  TerminalExitEvent,
  TerminalSessionResizeRequest,
  TerminalSessionStarted,
  StartTunnelRequest,
  TunnelInfo,
  TransferFileRequest,
  TransferFileResult,
  WriteRemoteFileRequest,
  WriteRemoteFileResult
} from "../src/shared/ipc.js";
import type { AppSnapshot } from "../src/domain/models.js";

const api = {
  getVersion: () => ipcRenderer.invoke("app:get-version") as Promise<string>,
  workspace: {
    load: () => ipcRenderer.invoke("workspace:load") as Promise<AppSnapshot | null>,
    save: (snapshot) => ipcRenderer.invoke("workspace:save", snapshot) as Promise<boolean>
  },
  terminal: {
    start: (request: StartTerminalSessionRequest) =>
      ipcRenderer.invoke("terminal:start", request) as Promise<TerminalSessionStarted>,
    write: (id: string, data: string) => ipcRenderer.invoke("terminal:write", id, data) as Promise<boolean>,
    resize: (request: TerminalSessionResizeRequest) =>
      ipcRenderer.invoke("terminal:resize", request) as Promise<boolean>,
    stop: (id: string) => ipcRenderer.invoke("terminal:stop", id) as Promise<boolean>,
    trustHost: (event: HostKeyVerificationEvent) =>
      ipcRenderer.invoke("terminal:trust-host", event) as Promise<boolean>,
    onData: (callback: (event: TerminalDataEvent) => void) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: TerminalDataEvent) => callback(payload);
      ipcRenderer.on("terminal:data", listener);
      return () => ipcRenderer.off("terminal:data", listener);
    },
    onExit: (callback: (event: TerminalExitEvent) => void) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: TerminalExitEvent) => callback(payload);
      ipcRenderer.on("terminal:exit", listener);
      return () => ipcRenderer.off("terminal:exit", listener);
    },
    onError: (callback: (event: TerminalErrorEvent) => void) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: TerminalErrorEvent) => callback(payload);
      ipcRenderer.on("terminal:error", listener);
      return () => ipcRenderer.off("terminal:error", listener);
    },
    onHostKeyVerification: (callback: (event: HostKeyVerificationEvent) => void) => {
      const listener = (_event: Electron.IpcRendererEvent, payload: HostKeyVerificationEvent) => callback(payload);
      ipcRenderer.on("terminal:host-key-verification", listener);
      return () => ipcRenderer.off("terminal:host-key-verification", listener);
    }
  },
  credentials: {
    status: (connectionId: string) =>
      ipcRenderer.invoke("credentials:status", connectionId) as Promise<CredentialStatus>,
    save: (request: SaveCredentialRequest) =>
      ipcRenderer.invoke("credentials:save", request) as Promise<CredentialStatus>,
    delete: (connectionId: string) =>
      ipcRenderer.invoke("credentials:delete", connectionId) as Promise<CredentialStatus>
  },
  sftp: {
    listDirectory: (request: ListRemoteDirectoryRequest) =>
      ipcRenderer.invoke("sftp:list-directory", request) as Promise<RemoteDirectoryListing>,
    transferFile: (request: TransferFileRequest) =>
      ipcRenderer.invoke("sftp:transfer-file", request) as Promise<TransferFileResult>,
    readFile: (request: ReadRemoteFileRequest) =>
      ipcRenderer.invoke("sftp:read-file", request) as Promise<ReadRemoteFileResult>,
    writeFile: (request: WriteRemoteFileRequest) =>
      ipcRenderer.invoke("sftp:write-file", request) as Promise<WriteRemoteFileResult>
  },
  metrics: {
    collect: (request: CollectMetricsRequest) =>
      ipcRenderer.invoke("metrics:collect", request) as Promise<CollectMetricsResult>,
    listProcesses: (request: ListProcessesRequest) =>
      ipcRenderer.invoke("metrics:list-processes", request) as Promise<ListProcessesResult>,
    killProcess: (request: KillProcessRequest) =>
      ipcRenderer.invoke("metrics:kill-process", request) as Promise<KillProcessResult>
  },
  tunnels: {
    start: (request: StartTunnelRequest) => ipcRenderer.invoke("tunnels:start", request) as Promise<TunnelInfo>,
    stop: (id: string) => ipcRenderer.invoke("tunnels:stop", id) as Promise<boolean>
  },
  logs: {
    readSession: (request: ReadSessionLogRequest) =>
      ipcRenderer.invoke("logs:read-session", request) as Promise<ReadSessionLogResult>
  },
  rdp: {
    open: (request: OpenRdpRequest) => ipcRenderer.invoke("rdp:open", request) as Promise<OpenRdpResult>
  }
} satisfies CNshellApi;

contextBridge.exposeInMainWorld("cnshell", api);
