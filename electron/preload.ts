import { contextBridge, ipcRenderer } from "electron";
import type {
  CNshellApi,
  CredentialStatus,
  SaveCredentialRequest,
  HostKeyVerificationEvent,
  StartTerminalSessionRequest,
  TerminalDataEvent,
  TerminalErrorEvent,
  TerminalExitEvent,
  TerminalSessionResizeRequest,
  TerminalSessionStarted
} from "../src/shared/ipc.js";

const api = {
  getVersion: () => ipcRenderer.invoke("app:get-version") as Promise<string>,
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
  }
} satisfies CNshellApi;

contextBridge.exposeInMainWorld("cnshell", api);
