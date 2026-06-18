import { app, BrowserWindow, ipcMain } from "electron";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { KnownHostsStore } from "./knownHostsStore.js";
import { TerminalSessionManager } from "./terminalSessionManager.js";
import type { HostKeyVerificationEvent } from "../src/shared/ipc.js";
import type { StartTerminalSessionRequest, TerminalSessionResizeRequest } from "./sessionTypes.js";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const isDev = Boolean(process.env.VITE_DEV_SERVER_URL);
let terminalSessionManager: TerminalSessionManager | null = null;
let knownHostsStore: KnownHostsStore | null = null;

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

  terminalSessionManager = new TerminalSessionManager(window, knownHostsStore);

  if (isDev && process.env.VITE_DEV_SERVER_URL) {
    void window.loadURL(process.env.VITE_DEV_SERVER_URL);
    window.webContents.openDevTools({ mode: "detach" });
  } else {
    void window.loadFile(path.join(__dirname, "../../dist/index.html"));
  }
}

app.whenReady().then(() => {
  knownHostsStore = new KnownHostsStore(app.getPath("userData"));
  ipcMain.handle("app:get-version", () => app.getVersion());
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
