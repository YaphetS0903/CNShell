import os from "node:os";
import { BrowserWindow } from "electron";
import { spawn, type IPty } from "node-pty";
import type {
  StartTerminalSessionRequest,
  TerminalSessionResizeRequest,
  TerminalSessionStarted
} from "./sessionTypes.js";

const MIN_COLS = 20;
const MIN_ROWS = 6;
const MAX_COLS = 500;
const MAX_ROWS = 200;

function clampTerminalSize(value: number, min: number, max: number) {
  if (!Number.isFinite(value)) {
    return min;
  }

  return Math.max(min, Math.min(max, Math.floor(value)));
}

function resolveShell() {
  if (process.platform === "win32") {
    return process.env.ComSpec ?? "powershell.exe";
  }

  return process.env.SHELL ?? "/bin/bash";
}

export class TerminalSessionManager {
  private readonly sessions = new Map<string, IPty>();

  constructor(private readonly window: BrowserWindow) {}

  startLocalSession(request: StartTerminalSessionRequest): TerminalSessionStarted {
    this.stopSession(request.id);

    const cols = clampTerminalSize(request.cols, MIN_COLS, MAX_COLS);
    const rows = clampTerminalSize(request.rows, MIN_ROWS, MAX_ROWS);
    const shell = resolveShell();
    const shellArgs = process.platform === "win32" && shell.toLowerCase().includes("powershell") ? ["-NoLogo"] : [];

    const pty = spawn(shell, shellArgs, {
      name: "xterm-256color",
      cols,
      rows,
      cwd: request.cwd ?? os.homedir(),
      env: {
        ...process.env,
        TERM: "xterm-256color",
        COLORTERM: "truecolor"
      }
    });

    this.sessions.set(request.id, pty);

    pty.onData((data) => {
      this.window.webContents.send("terminal:data", { id: request.id, data });
    });

    pty.onExit(({ exitCode, signal }) => {
      this.sessions.delete(request.id);
      this.window.webContents.send("terminal:exit", { id: request.id, exitCode, signal });
    });

    return {
      id: request.id,
      pid: pty.pid
    };
  }

  writeToSession(id: string, data: string) {
    const session = this.sessions.get(id);
    if (!session) {
      return false;
    }

    session.write(data);
    return true;
  }

  resizeSession(request: TerminalSessionResizeRequest) {
    const session = this.sessions.get(request.id);
    if (!session) {
      return false;
    }

    session.resize(clampTerminalSize(request.cols, MIN_COLS, MAX_COLS), clampTerminalSize(request.rows, MIN_ROWS, MAX_ROWS));
    return true;
  }

  stopSession(id: string) {
    const session = this.sessions.get(id);
    if (!session) {
      return false;
    }

    session.kill();
    this.sessions.delete(id);
    return true;
  }

  stopAll() {
    for (const id of this.sessions.keys()) {
      this.stopSession(id);
    }
  }
}
