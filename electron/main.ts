import { app, BrowserWindow, dialog, ipcMain } from "electron";
import fs from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { CloudSyncService } from "./cloudSyncService.js";
import { CredentialStore } from "./credentialStore.js";
import { KnownHostsStore } from "./knownHostsStore.js";
import { MetricsService } from "./metricsService.js";
import { SessionLogStore } from "./sessionLogStore.js";
import { SftpService } from "./sftpService.js";
import { TerminalSessionManager } from "./terminalSessionManager.js";
import { TunnelManager } from "./tunnelManager.js";
import { WorkspaceStore } from "./workspaceStore.js";
import {
  validateAppSnapshot,
  validateCollectMetrics,
  validateConnectionId,
  validateDisableVault,
  validateEnableVault,
  validateExportCloudSync,
  validateHostKeyVerification,
  validateIpcId,
  validateKillProcess,
  validateListProcesses,
  validateListRemoteDirectory,
  validateOpenRdp,
  validateReadRemoteFile,
  validateReadSessionLog,
  validateSaveCredential,
  validateStartRelay,
  validateStartTerminalSession,
  validateStartTunnel,
  validateTerminalResize,
  validateTerminalWrite,
  validateTransferFile,
  validateUnlockVault,
  validateWriteRemoteFile
} from "./ipcValidation.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isDev = Boolean(process.env.VITE_DEV_SERVER_URL);
const MAX_PRIVATE_KEY_BYTES = 256 * 1024;
let terminalSessionManager: TerminalSessionManager | null = null;
let knownHostsStore: KnownHostsStore | null = null;
let credentialStore: CredentialStore | null = null;
let workspaceStore: WorkspaceStore | null = null;
let sftpService: SftpService | null = null;
let metricsService: MetricsService | null = null;
let sessionLogStore: SessionLogStore | null = null;
let tunnelManager: TunnelManager | null = null;
let cloudSyncService: CloudSyncService | null = null;

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

  terminalSessionManager = new TerminalSessionManager(window, knownHostsStore, credentialStore, sessionLogStore);

  if (isDev && process.env.VITE_DEV_SERVER_URL) {
    void window.loadURL(process.env.VITE_DEV_SERVER_URL);
    window.webContents.openDevTools({ mode: "detach" });
  } else {
    void window.loadFile(path.join(__dirname, "../../dist/index.html"));
  }
}

async function importPrivateKeyFile() {
  const result = await dialog.showOpenDialog({
    title: "Import SSH private key",
    properties: ["openFile"],
    filters: [
      { name: "SSH Private Keys", extensions: ["pem", "key", "ppk", "openssh", "id_rsa", "id_ed25519", "*"] },
      { name: "All Files", extensions: ["*"] }
    ]
  });

  if (result.canceled || result.filePaths.length === 0) {
    return { ok: false };
  }

  const keyPath = result.filePaths[0];
  const stat = fs.statSync(keyPath);
  if (stat.size > MAX_PRIVATE_KEY_BYTES) {
    throw new Error("Private key file is too large.");
  }

  return {
    ok: true,
    path: keyPath,
    fileName: path.basename(keyPath),
    privateKey: fs.readFileSync(keyPath, "utf8")
  };
}

app.whenReady().then(() => {
  knownHostsStore = new KnownHostsStore(app.getPath("userData"));
  credentialStore = new CredentialStore(app.getPath("userData"));
  workspaceStore = new WorkspaceStore(app.getPath("userData"));
  sftpService = new SftpService(knownHostsStore, credentialStore);
  metricsService = new MetricsService(knownHostsStore, credentialStore);
  sessionLogStore = new SessionLogStore(app.getPath("userData"));
  tunnelManager = new TunnelManager(knownHostsStore, credentialStore);
  cloudSyncService = new CloudSyncService();
  ipcMain.handle("app:get-version", () => app.getVersion());
  ipcMain.handle("workspace:load", () => workspaceStore?.load() ?? null);
  ipcMain.handle("workspace:save", (_event, snapshot: unknown) => {
    workspaceStore?.save(validateAppSnapshot(snapshot));
    return true;
  });
  ipcMain.handle("terminal:start", (_event, payload: unknown) => {
    const request = validateStartTerminalSession(payload);
    if (request.kind === "ssh") {
      return terminalSessionManager?.startSshSession(request);
    }

    return terminalSessionManager?.startLocalSession(request);
  });
  ipcMain.handle("terminal:write", (_event, id: unknown, data: unknown) => {
    const request = validateTerminalWrite(id, data);
    return terminalSessionManager?.writeToSession(request.id, request.data);
  });
  ipcMain.handle("terminal:resize", (_event, payload: unknown) =>
    terminalSessionManager?.resizeSession(validateTerminalResize(payload))
  );
  ipcMain.handle("terminal:stop", (_event, id: unknown) => terminalSessionManager?.stopSession(validateIpcId(id)));
  ipcMain.handle("terminal:trust-host", (_event, payload: unknown) => {
    const event = validateHostKeyVerification(payload);
    knownHostsStore?.trustHost(event.host, event.port, event.fingerprint, event.keyBase64);
    return true;
  });
  ipcMain.handle("credentials:status", (_event, connectionId: unknown) =>
    credentialStore?.getStatus(validateConnectionId(connectionId))
  );
  ipcMain.handle("credentials:save", (_event, payload: unknown) => credentialStore?.save(validateSaveCredential(payload)));
  ipcMain.handle("credentials:delete", (_event, connectionId: unknown) =>
    credentialStore?.delete(validateConnectionId(connectionId))
  );
  ipcMain.handle("credentials:vault-status", () => credentialStore?.getVaultStatus());
  ipcMain.handle("credentials:enable-vault", (_event, payload: unknown) =>
    credentialStore?.enableVault(validateEnableVault(payload))
  );
  ipcMain.handle("credentials:unlock-vault", (_event, payload: unknown) =>
    credentialStore?.unlockVault(validateUnlockVault(payload))
  );
  ipcMain.handle("credentials:disable-vault", (_event, payload: unknown) =>
    credentialStore?.disableVault(validateDisableVault(payload))
  );
  ipcMain.handle("credentials:lock-vault", () => credentialStore?.lockVault());
  ipcMain.handle("credentials:import-private-key", () => importPrivateKeyFile());
  ipcMain.handle("sftp:list-directory", (_event, payload: unknown) =>
    sftpService?.listDirectory(validateListRemoteDirectory(payload))
  );
  ipcMain.handle("sftp:transfer-file", (_event, payload: unknown) =>
    sftpService?.transferFile(validateTransferFile(payload))
  );
  ipcMain.handle("sftp:read-file", (_event, payload: unknown) => sftpService?.readFile(validateReadRemoteFile(payload)));
  ipcMain.handle("sftp:write-file", (_event, payload: unknown) =>
    sftpService?.writeFile(validateWriteRemoteFile(payload))
  );
  ipcMain.handle("metrics:collect", (_event, payload: unknown) => metricsService?.collect(validateCollectMetrics(payload)));
  ipcMain.handle("metrics:list-processes", (_event, payload: unknown) =>
    metricsService?.listProcesses(validateListProcesses(payload))
  );
  ipcMain.handle("metrics:kill-process", (_event, payload: unknown) =>
    metricsService?.killProcess(validateKillProcess(payload))
  );
  ipcMain.handle("tunnels:start", (_event, payload: unknown) => tunnelManager?.start(validateStartTunnel(payload)));
  ipcMain.handle("tunnels:stop", (_event, id: unknown) => tunnelManager?.stop(validateIpcId(id)));
  ipcMain.handle("relay:start", (_event, payload: unknown) => tunnelManager?.startRelay(validateStartRelay(payload)));
  ipcMain.handle("relay:stop", (_event, id: unknown) => tunnelManager?.stop(validateIpcId(id)));
  ipcMain.handle("logs:read-session", (_event, payload: unknown) => {
    const request = validateReadSessionLog(payload);
    return {
      lines: sessionLogStore?.read(request.sessionId, request.query, request.limit) ?? []
    };
  });
  ipcMain.handle("rdp:open", (_event, payload: unknown) => {
    const request = validateOpenRdp(payload);
    if (process.platform !== "win32") {
      throw new Error("RDP launch is only available on Windows.");
    }

    const target = `${request.host}:${request.port || 3389}`;
    const child = spawn("mstsc.exe", [`/v:${target}`], {
      detached: true,
      stdio: "ignore"
    });
    child.unref();
    return { ok: true };
  });
  ipcMain.handle("cloud-sync:export", (_event, payload: unknown) =>
    cloudSyncService?.exportSettings(validateExportCloudSync(payload))
  );
  ipcMain.handle("cloud-sync:import", () => cloudSyncService?.importSettings());
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
