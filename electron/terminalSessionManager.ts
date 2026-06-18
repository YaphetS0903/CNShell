import os from "node:os";
import { BrowserWindow } from "electron";
import { spawn, type IPty } from "node-pty";
import { Client, type ClientChannel } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { connectSshClient } from "./sshConnectionConfig.js";
import type { SessionLogStore } from "./sessionLogStore.js";
import type {
  StartTerminalSessionRequest,
  TerminalSessionResizeRequest,
  TerminalSessionStarted
} from "./sessionTypes.js";

const MIN_COLS = 20;
const MIN_ROWS = 6;
const MAX_COLS = 500;
const MAX_ROWS = 200;

interface LocalTerminalSession {
  kind: "local";
  pty: IPty;
}

interface SshTerminalSession {
  kind: "ssh";
  client: Client;
  gateways: Client[];
  stream?: ClientChannel;
}

type TerminalSession = LocalTerminalSession | SshTerminalSession;

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
  private readonly sessions = new Map<string, TerminalSession>();

  constructor(
    private readonly window: BrowserWindow,
    private readonly knownHostsStore: KnownHostsStore | null,
    private readonly credentialStore: CredentialStore | null,
    private readonly sessionLogStore: SessionLogStore | null
  ) {}

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

    this.sessions.set(request.id, { kind: "local", pty });

    pty.onData((data) => {
      this.sessionLogStore?.append(request.id, data);
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

    if (session.kind === "local") {
      session.pty.write(data);
      return true;
    }

    session.stream?.write(data);
    return true;
  }

  resizeSession(request: TerminalSessionResizeRequest) {
    const session = this.sessions.get(request.id);
    if (!session) {
      return false;
    }

    const cols = clampTerminalSize(request.cols, MIN_COLS, MAX_COLS);
    const rows = clampTerminalSize(request.rows, MIN_ROWS, MAX_ROWS);

    if (session.kind === "local") {
      session.pty.resize(cols, rows);
      return true;
    }

    session.stream?.setWindow(rows, cols, rows * 18, cols * 9);
    return true;
  }

  stopSession(id: string) {
    const session = this.sessions.get(id);
    if (!session) {
      return false;
    }

    if (session.kind === "local") {
      session.pty.kill();
    } else {
      session.stream?.close();
      session.client.end();
      for (const gateway of session.gateways) {
        gateway.end();
      }
    }

    this.sessions.delete(id);
    return true;
  }

  startSshSession(request: StartTerminalSessionRequest): Promise<TerminalSessionStarted> {
    this.stopSession(request.id);

    const ssh = request.ssh;
    if (!ssh) {
      throw new Error("SSH configuration is required.");
    }

    const cols = clampTerminalSize(request.cols, MIN_COLS, MAX_COLS);
    const rows = clampTerminalSize(request.rows, MIN_ROWS, MAX_ROWS);
    const client = new Client();

    this.sessions.set(request.id, { kind: "ssh", client, gateways: [] });

    return new Promise((resolve, reject) => {
      let settled = false;

      const fail = (error: Error) => {
        this.window.webContents.send("terminal:error", { id: request.id, message: error.message });
        if (!settled) {
          settled = true;
          this.sessions.delete(request.id);
          reject(error);
        }
      };

      const startShell = (gateways: Client[]) => {
        client.on("close", () => {
          if (this.sessions.has(request.id)) {
            this.sessions.delete(request.id);
            for (const gateway of gateways) {
              gateway.end();
            }
            this.window.webContents.send("terminal:exit", { id: request.id, exitCode: 0 });
          }
        });

        client.shell(
            {
              term: "xterm-256color",
              cols,
              rows
            },
            (error, stream) => {
              if (error) {
                fail(error);
                return;
              }

              this.sessions.set(request.id, { kind: "ssh", client, gateways, stream });

              stream.on("data", (data: Buffer) => {
                const output = data.toString("utf8");
                this.sessionLogStore?.append(request.id, output);
                this.window.webContents.send("terminal:data", { id: request.id, data: output });
              });

              stream.stderr.on("data", (data: Buffer) => {
                const output = data.toString("utf8");
                this.sessionLogStore?.append(request.id, output);
                this.window.webContents.send("terminal:data", { id: request.id, data: output });
              });

              stream.on("close", () => {
                this.sessions.delete(request.id);
                client.end();
                for (const gateway of gateways) {
                  gateway.end();
                }
                this.window.webContents.send("terminal:exit", { id: request.id, exitCode: 0 });
              });

              if (!settled) {
                settled = true;
                resolve({ id: request.id });
              }
            }
          );
      };

      connectSshClient(client, {
        ssh,
        credentialStore: this.credentialStore,
        knownHostsStore: this.knownHostsStore,
        onHostKeyVerification: (event) => {
          this.window.webContents.send("terminal:host-key-verification", {
            id: request.id,
            ...event
          });
        }
      })
        .then(({ gateways }) => startShell(gateways))
        .catch(fail);
    });
  }

  stopAll() {
    for (const id of this.sessions.keys()) {
      this.stopSession(id);
    }
  }
}
