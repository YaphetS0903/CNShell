import os from "node:os";
import { BrowserWindow } from "electron";
import { spawn, type IPty } from "node-pty";
import { Client, type ClientChannel } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
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
    private readonly credentialStore: CredentialStore | null
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

    const savedSecret = ssh.useSavedCredential ? this.credentialStore?.loadSecret(ssh.connectionId) : undefined;
    const password = ssh.password || savedSecret?.password;
    const privateKey = ssh.privateKey || savedSecret?.privateKey;
    const passphrase = ssh.passphrase || savedSecret?.passphrase;

    if (!password && !privateKey) {
      throw new Error("SSH password or private key is required.");
    }

    const cols = clampTerminalSize(request.cols, MIN_COLS, MAX_COLS);
    const rows = clampTerminalSize(request.rows, MIN_ROWS, MAX_ROWS);
    const client = new Client();

    this.sessions.set(request.id, { kind: "ssh", client });

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

      client
        .on("ready", () => {
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

              this.sessions.set(request.id, { kind: "ssh", client, stream });

              stream.on("data", (data: Buffer) => {
                this.window.webContents.send("terminal:data", { id: request.id, data: data.toString("utf8") });
              });

              stream.stderr.on("data", (data: Buffer) => {
                this.window.webContents.send("terminal:data", { id: request.id, data: data.toString("utf8") });
              });

              stream.on("close", () => {
                this.sessions.delete(request.id);
                client.end();
                this.window.webContents.send("terminal:exit", { id: request.id, exitCode: 0 });
              });

              if (!settled) {
                settled = true;
                resolve({ id: request.id });
              }
            }
          );
        })
        .on("error", fail)
        .on("close", () => {
          if (this.sessions.has(request.id)) {
            this.sessions.delete(request.id);
            this.window.webContents.send("terminal:exit", { id: request.id, exitCode: 0 });
          }
        });

      client.connect({
        host: ssh.host,
        port: ssh.port,
        username: ssh.username,
        password,
        privateKey,
        passphrase,
        readyTimeout: ssh.readyTimeout ?? 15000,
        keepaliveInterval: 15000,
        hostVerifier: (key: Buffer) => {
          if (!this.knownHostsStore) {
            return false;
          }

          const verification = this.knownHostsStore.verifyHostKey(ssh.host, ssh.port, key);
          if (verification.status === "trusted") {
            return true;
          }

          this.window.webContents.send("terminal:host-key-verification", {
            id: request.id,
            ...verification,
            keyBase64: key.toString("base64")
          });

          return false;
        }
      });
    });
  }

  stopAll() {
    for (const id of this.sessions.keys()) {
      this.stopSession(id);
    }
  }
}
