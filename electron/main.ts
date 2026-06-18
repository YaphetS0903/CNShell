import { app, BrowserWindow, dialog, ipcMain, Menu } from "electron";
import fs from "node:fs";
import { spawn } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";
import type { AppLanguage } from "../src/shared/ipc.js";
import { AuditLogStore, sanitizeAuditTarget, summarizeTerminalWrite, summarizeWorkspace } from "./auditLogStore.js";
import { CloudSyncService } from "./cloudSyncService.js";
import { CredentialStore } from "./credentialStore.js";
import { ErrorReportStore } from "./errorReportStore.js";
import { KnownHostsStore } from "./knownHostsStore.js";
import { MetricsService } from "./metricsService.js";
import { SessionLogStore } from "./sessionLogStore.js";
import { SftpService } from "./sftpService.js";
import { TerminalSessionManager } from "./terminalSessionManager.js";
import { TunnelManager } from "./tunnelManager.js";
import { UpdateService } from "./updateService.js";
import { WorkspaceStore } from "./workspaceStore.js";
import {
  validateAppSnapshot,
  validateCheckForUpdates,
  validateCollectMetrics,
  validateConnectionId,
  validateCreateRemoteDirectory,
  validateDeleteRemotePath,
  validateDisableVault,
  validateEnableVault,
  validateExportCloudSync,
  validateHostKeyVerification,
  validateIpcId,
  validateKillProcess,
  validateListProcesses,
  validateListRemoteDirectory,
  validateOpenRdp,
  validateReadAuditLog,
  validateReadErrorReport,
  validateReadRemoteFile,
  validateReadSessionLog,
  validateRendererErrorReport,
  validateRenameRemotePath,
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
let auditLogStore: AuditLogStore | null = null;
let updateService: UpdateService | null = null;
let errorReportStore: ErrorReportStore | null = null;
let currentLanguage: AppLanguage = "zh-CN";

function configureApplicationMenu(language: AppLanguage = "zh-CN") {
  currentLanguage = language;
  const zh = language === "zh-CN";
  const template: Electron.MenuItemConstructorOptions[] = [
    {
      label: zh ? "文件" : "File",
      submenu: [{ role: "quit", label: zh ? "退出" : "Quit" }]
    },
    {
      label: zh ? "编辑" : "Edit",
      submenu: [
        { role: "undo", label: zh ? "撤销" : "Undo" },
        { role: "redo", label: zh ? "重做" : "Redo" },
        { type: "separator" },
        { role: "cut", label: zh ? "剪切" : "Cut" },
        { role: "copy", label: zh ? "复制" : "Copy" },
        { role: "paste", label: zh ? "粘贴" : "Paste" },
        { role: "selectAll", label: zh ? "全选" : "Select All" }
      ]
    },
    {
      label: zh ? "视图" : "View",
      submenu: [
        { role: "reload", label: zh ? "重新加载" : "Reload" },
        { role: "toggleDevTools", label: zh ? "开发者工具" : "Developer Tools" },
        { type: "separator" },
        { role: "resetZoom", label: zh ? "实际大小" : "Actual Size" },
        { role: "zoomIn", label: zh ? "放大" : "Zoom In" },
        { role: "zoomOut", label: zh ? "缩小" : "Zoom Out" },
        { type: "separator" },
        { role: "togglefullscreen", label: zh ? "切换全屏" : "Toggle Full Screen" }
      ]
    },
    {
      label: zh ? "窗口" : "Window",
      submenu: [
        { role: "minimize", label: zh ? "最小化" : "Minimize" },
        { role: "close", label: zh ? "关闭窗口" : "Close Window" }
      ]
    },
    {
      label: zh ? "帮助" : "Help",
      submenu: [
        {
          label: zh ? "关于 CNshell" : "About CNshell",
          click: () => {
            const focusedWindow = BrowserWindow.getFocusedWindow();
            if (focusedWindow) {
              void dialog.showMessageBox(focusedWindow, {
                type: "info",
                title: "CNshell",
                message: "CNshell",
                detail: zh ? `版本 ${app.getVersion()}` : `Version ${app.getVersion()}`
              });
            }
          }
        }
      ]
    }
  ];

  Menu.setApplicationMenu(Menu.buildFromTemplate(template));
}

function recordMainError(error: unknown) {
  errorReportStore?.record("main", error instanceof Error ? error : new Error(String(error)));
}

process.on("uncaughtException", recordMainError);
process.on("unhandledRejection", recordMainError);

function createMainWindow() {
  const window = new BrowserWindow({
    width: 1440,
    height: 900,
    minWidth: 1120,
    minHeight: 720,
    title: "CNshell",
    icon: path.join(__dirname, "../../build/icon.ico"),
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
  updateService = new UpdateService(window);

  if (isDev && process.env.VITE_DEV_SERVER_URL) {
    void window.loadURL(process.env.VITE_DEV_SERVER_URL);
    window.webContents.openDevTools({ mode: "detach" });
  } else {
    void window.loadFile(path.join(__dirname, "../../dist/index.html"));
  }
}

async function importPrivateKeyFile() {
  const zh = currentLanguage === "zh-CN";
  const result = await dialog.showOpenDialog({
    title: zh ? "导入 SSH 私钥" : "Import SSH private key",
    properties: ["openFile"],
    filters: [
      { name: zh ? "SSH 私钥" : "SSH Private Keys", extensions: ["pem", "key", "ppk", "openssh", "id_rsa", "id_ed25519", "*"] },
      { name: zh ? "所有文件" : "All Files", extensions: ["*"] }
    ]
  });

  if (result.canceled || result.filePaths.length === 0) {
    return { ok: false };
  }

  const keyPath = result.filePaths[0];
  const stat = fs.statSync(keyPath);
  if (stat.size > MAX_PRIVATE_KEY_BYTES) {
    throw new Error(zh ? "私钥文件过大。" : "Private key file is too large.");
  }

  return {
    ok: true,
    path: keyPath,
    fileName: path.basename(keyPath),
    privateKey: fs.readFileSync(keyPath, "utf8")
  };
}

async function withAudit<T>(
  action: string,
  target: string | undefined,
  details: unknown,
  operation: () => T | Promise<T>
): Promise<T> {
  try {
    const result = await operation();
    auditLogStore?.record({
      action,
      status: "ok",
      target: target ? sanitizeAuditTarget(target) : undefined,
      details
    });
    return result;
  } catch (error) {
    auditLogStore?.record({
      action,
      status: "error",
      target: target ? sanitizeAuditTarget(target) : undefined,
      details,
      error: error instanceof Error ? error.message : String(error)
    });
    throw error;
  }
}

app.whenReady().then(() => {
  configureApplicationMenu("zh-CN");
  knownHostsStore = new KnownHostsStore(app.getPath("userData"));
  credentialStore = new CredentialStore(app.getPath("userData"));
  workspaceStore = new WorkspaceStore(app.getPath("userData"));
  sftpService = new SftpService(knownHostsStore, credentialStore);
  metricsService = new MetricsService(knownHostsStore, credentialStore);
  sessionLogStore = new SessionLogStore(app.getPath("userData"));
  tunnelManager = new TunnelManager(knownHostsStore, credentialStore);
  cloudSyncService = new CloudSyncService();
  auditLogStore = new AuditLogStore(app.getPath("userData"));
  errorReportStore = new ErrorReportStore(app.getPath("userData"));
  ipcMain.handle("app:get-version", () => app.getVersion());
  ipcMain.handle("app:set-language", (_event, language: unknown) => {
    const nextLanguage: AppLanguage = language === "en-US" ? "en-US" : "zh-CN";
    configureApplicationMenu(nextLanguage);
    return true;
  });
  ipcMain.handle("workspace:load", () => workspaceStore?.load() ?? null);
  ipcMain.handle("workspace:save", (_event, snapshot: unknown) => {
    const validatedSnapshot = validateAppSnapshot(snapshot);
    return withAudit("workspace.save", undefined, summarizeWorkspace(validatedSnapshot), () => {
      workspaceStore?.save(validatedSnapshot);
      return true;
    });
  });
  ipcMain.handle("terminal:start", (_event, payload: unknown) => {
    const request = validateStartTerminalSession(payload);
    return withAudit("terminal.start", request.id, { kind: request.kind, ssh: request.ssh }, () => {
      if (request.kind === "ssh") {
        return terminalSessionManager?.startSshSession(request);
      }

      return terminalSessionManager?.startLocalSession(request);
    });
  });
  ipcMain.handle("terminal:write", (_event, id: unknown, data: unknown) => {
    const request = validateTerminalWrite(id, data);
    return withAudit("terminal.write", request.id, summarizeTerminalWrite(request.id, request.data), () =>
      terminalSessionManager?.writeToSession(request.id, request.data)
    );
  });
  ipcMain.handle("terminal:resize", (_event, payload: unknown) =>
    terminalSessionManager?.resizeSession(validateTerminalResize(payload))
  );
  ipcMain.handle("terminal:stop", (_event, id: unknown) => {
    const sessionId = validateIpcId(id);
    return withAudit("terminal.stop", sessionId, { id: sessionId }, () => terminalSessionManager?.stopSession(sessionId));
  });
  ipcMain.handle("terminal:trust-host", (_event, payload: unknown) => {
    const event = validateHostKeyVerification(payload);
    return withAudit("terminal.trustHost", event.id, event, () => {
      knownHostsStore?.trustHost(event.host, event.port, event.fingerprint, event.keyBase64);
      return true;
    });
  });
  ipcMain.handle("credentials:status", (_event, connectionId: unknown) =>
    credentialStore?.getStatus(validateConnectionId(connectionId))
  );
  ipcMain.handle("credentials:save", (_event, payload: unknown) => {
    const request = validateSaveCredential(payload);
    return withAudit("credentials.save", request.connectionId, request, () => credentialStore?.save(request));
  });
  ipcMain.handle("credentials:delete", (_event, connectionId: unknown) => {
    const validatedConnectionId = validateConnectionId(connectionId);
    return withAudit("credentials.delete", validatedConnectionId, { connectionId: validatedConnectionId }, () =>
      credentialStore?.delete(validatedConnectionId)
    );
  });
  ipcMain.handle("credentials:vault-status", () => credentialStore?.getVaultStatus());
  ipcMain.handle("credentials:enable-vault", (_event, payload: unknown) => {
    const request = validateEnableVault(payload);
    return withAudit("credentials.enableVault", undefined, request, () => credentialStore?.enableVault(request));
  });
  ipcMain.handle("credentials:unlock-vault", (_event, payload: unknown) => {
    const request = validateUnlockVault(payload);
    return withAudit("credentials.unlockVault", undefined, request, () => credentialStore?.unlockVault(request));
  });
  ipcMain.handle("credentials:disable-vault", (_event, payload: unknown) => {
    const request = validateDisableVault(payload);
    return withAudit("credentials.disableVault", undefined, request, () => credentialStore?.disableVault(request));
  });
  ipcMain.handle("credentials:lock-vault", () =>
    withAudit("credentials.lockVault", undefined, undefined, () => credentialStore?.lockVault())
  );
  ipcMain.handle("credentials:import-private-key", () =>
    withAudit("credentials.importPrivateKey", undefined, undefined, () => importPrivateKeyFile())
  );
  ipcMain.handle("sftp:list-directory", (_event, payload: unknown) => {
    const request = validateListRemoteDirectory(payload);
    return withAudit("sftp.listDirectory", request.ssh.connectionId, request, () => sftpService?.listDirectory(request));
  });
  ipcMain.handle("sftp:transfer-file", (_event, payload: unknown) => {
    const request = validateTransferFile(payload);
    return withAudit("sftp.transferFile", request.ssh.connectionId, request, () => sftpService?.transferFile(request));
  });
  ipcMain.handle("sftp:read-file", (_event, payload: unknown) => {
    const request = validateReadRemoteFile(payload);
    return withAudit("sftp.readFile", request.ssh.connectionId, request, () => sftpService?.readFile(request));
  });
  ipcMain.handle("sftp:write-file", (_event, payload: unknown) => {
    const request = validateWriteRemoteFile(payload);
    return withAudit("sftp.writeFile", request.ssh.connectionId, request, () => sftpService?.writeFile(request));
  });
  ipcMain.handle("sftp:create-directory", (_event, payload: unknown) => {
    const request = validateCreateRemoteDirectory(payload);
    return withAudit("sftp.createDirectory", request.ssh.connectionId, request, () =>
      sftpService?.createDirectory(request)
    );
  });
  ipcMain.handle("sftp:rename-path", (_event, payload: unknown) => {
    const request = validateRenameRemotePath(payload);
    return withAudit("sftp.renamePath", request.ssh.connectionId, request, () => sftpService?.renamePath(request));
  });
  ipcMain.handle("sftp:delete-path", (_event, payload: unknown) => {
    const request = validateDeleteRemotePath(payload);
    return withAudit("sftp.deletePath", request.ssh.connectionId, request, () => sftpService?.deletePath(request));
  });
  ipcMain.handle("metrics:collect", (_event, payload: unknown) => {
    const request = validateCollectMetrics(payload);
    return withAudit("metrics.collect", request.ssh.connectionId, request, () => metricsService?.collect(request));
  });
  ipcMain.handle("metrics:list-processes", (_event, payload: unknown) => {
    const request = validateListProcesses(payload);
    return withAudit("metrics.listProcesses", request.ssh.connectionId, request, () => metricsService?.listProcesses(request));
  });
  ipcMain.handle("metrics:kill-process", (_event, payload: unknown) => {
    const request = validateKillProcess(payload);
    return withAudit("metrics.killProcess", request.ssh.connectionId, request, () => metricsService?.killProcess(request));
  });
  ipcMain.handle("tunnels:start", (_event, payload: unknown) => {
    const request = validateStartTunnel(payload);
    return withAudit("tunnels.start", request.id, request, () => tunnelManager?.start(request));
  });
  ipcMain.handle("tunnels:stop", (_event, id: unknown) => {
    const tunnelId = validateIpcId(id);
    return withAudit("tunnels.stop", tunnelId, { id: tunnelId }, () => tunnelManager?.stop(tunnelId));
  });
  ipcMain.handle("relay:start", (_event, payload: unknown) => {
    const request = validateStartRelay(payload);
    return withAudit("relay.start", request.id, request, () => tunnelManager?.startRelay(request));
  });
  ipcMain.handle("relay:stop", (_event, id: unknown) => {
    const relayId = validateIpcId(id);
    return withAudit("relay.stop", relayId, { id: relayId }, () => tunnelManager?.stop(relayId));
  });
  ipcMain.handle("logs:read-session", (_event, payload: unknown) => {
    const request = validateReadSessionLog(payload);
    return {
      lines: sessionLogStore?.read(request.sessionId, request.query, request.limit) ?? []
    };
  });
  ipcMain.handle("logs:read-audit", (_event, payload: unknown) => {
    const request = validateReadAuditLog(payload);
    return {
      lines: auditLogStore?.read(request.query, request.limit) ?? []
    };
  });
  ipcMain.handle("logs:read-errors", (_event, payload: unknown) => {
    const request = validateReadErrorReport(payload);
    return {
      lines: errorReportStore?.read(request.query, request.limit) ?? []
    };
  });
  ipcMain.handle("logs:report-renderer-error", (_event, payload: unknown) => {
    const request = validateRendererErrorReport(payload);
    errorReportStore?.record("renderer", request);
    auditLogStore?.record({
      action: "renderer.error",
      status: "error",
      details: { message: request.message }
    });
    return true;
  });
  ipcMain.handle("rdp:open", (_event, payload: unknown) => {
    const request = validateOpenRdp(payload);
    if (process.platform !== "win32") {
      throw new Error(currentLanguage === "zh-CN" ? "RDP 启动仅支持 Windows。" : "RDP launch is only available on Windows.");
    }

    return withAudit("rdp.open", request.host, request, () => {
      const target = `${request.host}:${request.port || 3389}`;
      const child = spawn("mstsc.exe", [`/v:${target}`], {
        detached: true,
        stdio: "ignore"
      });
      child.unref();
      return { ok: true };
    });
  });
  ipcMain.handle("cloud-sync:export", (_event, payload: unknown) => {
    const request = validateExportCloudSync(payload);
    return withAudit("cloudSync.export", undefined, summarizeWorkspace(request.snapshot), () =>
      cloudSyncService?.exportSettings(request)
    );
  });
  ipcMain.handle("cloud-sync:import", () =>
    withAudit("cloudSync.import", undefined, undefined, () => cloudSyncService?.importSettings())
  );
  ipcMain.handle("updates:status", () => updateService?.getStatus());
  ipcMain.handle("updates:check", (_event, payload: unknown) => {
    const request = validateCheckForUpdates(payload);
    return withAudit("updates.check", undefined, request, () => updateService?.check(request));
  });
  ipcMain.handle("updates:quit-and-install", () =>
    withAudit("updates.quitAndInstall", undefined, undefined, () => updateService?.quitAndInstall() ?? false)
  );
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
