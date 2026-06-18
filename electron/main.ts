import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CredentialStore } from "./credentialStore.js";
import { KnownHostsStore } from "./knownHostsStore.js";
import { SftpService } from "./sftpService.js";
import { TerminalSessionManager } from "./terminalSessionManager.js";
import { WorkspaceStore } from "./workspaceStore.js";
import type {
  HostKeyVerificationEvent,
  ListRemoteDirectoryRequest,
  SaveCredentialRequest,
  TransferFileRequest
} from "../src/shared/ipc.js";
import type { AppSnapshot } from "../src/domain/models.js";
import type { StartTerminalSessionRequest, TerminalSessionResizeRequest } from "./sessionTypes.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isDev = Boolean(process.env.VITE_DEV_SERVER_URL);
let terminalSessionManager: TerminalSessionManager | null = null;
let knownHostsStore: KnownHostsStore | null = null;
let credentialStore: CredentialStore | null = null;
let workspaceStore: WorkspaceStore | null = null;
let sftpService: SftpService | null = null;

function createMainWindow() {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1120,
    minHeight: 720,
    title: "CNshell",
    backgroundColor: "#0b1117",
    titleBarStyle: "hiddenInset",
    webPreferences: {
      preload: path.join(__dirname, "preload.js"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: false
    }
  });

  terminalSessionManager = new TerminalSessionManager(window, knownHostsStore, credentialStore);

  if (isDev && process.env.VITE_DEV_SERVER_URL) {
    void window.loadURL(process.env.VITE_DEV_SERVER_URL);
    window.webContents.openDevTools({ mode: "detach" });
  } else {
    void window.loadFile(path.join(__dirname, "../../dist/index.html"));
  }
}

app.whenReady().then(() => {
  knownHostsStore = new KnownHostsStore(app.getPath("userData"));
  credentialStore = new CredentialStore(app.getPath("userData"));
  workspaceStore = new WorkspaceStore(app.getPath("userData"));
  sftpService = new SftpService(knownHostsStore, credentialStore);
  ipcMain.handle("app:get-version", () => app.getVersion());
  ipcMain.handle("workspace:load", () => workspaceStore?.load() ?? null);
  ipcMain.handle("workspace:save", (_event, snapshot: AppSnapshot) => {
    workspaceStore?.save(snapshot);
    return true;
  });
  ipcMain.handle("terminal:start", (_event, request: StartTerminalSessionRequest) => {
    if (request.kind === "ssh") {
      return terminalSessionManager?.startSshSession(request);
    }

    return terminalSessionManager?.startLocalSession(request);
  });
  ipcMain.handle("terminal:write", (_event, id: string, data: string) => terminalSessionManager?.writeToSession(id, data));
  ipcMain.handle("terminal:resize", (_event, request: TerminalSessionResizeRequest) =>
    terminalSessionManager?.resizeSession(request)
  );
  ipcMain.handle("terminal:stop", (_event, id: string) => terminalSessionManager?.stopSession(id));
  ipcMain.handle("terminal:trust-host", (_event, event: HostKeyVerificationEvent) => {
    knownHostsStore?.trustHost(event.host, event.port, event.fingerprint, event.keyBase64);
    return true;
  });
  ipcMain.handle("credentials:status", (_event, connectionId: string) => credentialStore?.getStatus(connectionId));
  ipcMain.handle("credentials:save", (_event, request: SaveCredentialRequest) => credentialStore?.save(request));
  ipcMain.handle("credentials:delete", (_event, connectionId: string) => credentialStore?.delete(connectionId));
  ipcMain.handle("sftp:list-directory", (_event, request: ListRemoteDirectoryRequest) =>
    sftpService?.listDirectory(request)
  );
  ipcMain.handle("sftp:transfer-file", (_event, request: TransferFileRequest) => sftpService?.transferFile(request));
  createMainWindow();

  app.on("activate", () => {
    if (BrowserWindow.getAllWindows().length === 0) {
      createMainWindow();
    }
  });
});

app.on("window-all-closed", () => {
  terminalSessionManager?.stopAll();
  if (process.platform !== "darwin") {
    app.quit();
  }
});
