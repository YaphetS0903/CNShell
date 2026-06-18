export type TerminalSessionKind = "local" | "ssh";

export interface StartTerminalSessionRequest {
  id: string;
  kind: TerminalSessionKind;
  cols: number;
  rows: number;
  cwd?: string;
  ssh?: SshSessionConfig;
}

export interface TerminalSessionStarted {
  id: string;
  pid?: number;
}

export interface TerminalSessionResizeRequest {
  id: string;
  cols: number;
  rows: number;
}

export interface TerminalDataEvent {
  id: string;
  data: string;
}

export interface TerminalExitEvent {
  id: string;
  exitCode: number;
  signal?: number;
}

export interface TerminalErrorEvent {
  id: string;
  message: string;
}

export interface SshSessionConfig {
  host: string;
  port: number;
  username: string;
  password?: string;
  privateKey?: string;
  passphrase?: string;
  readyTimeout?: number;
}

export interface CNshellApi {
  getVersion: () => Promise<string>;
  terminal: {
    start: (request: StartTerminalSessionRequest) => Promise<TerminalSessionStarted>;
    write: (id: string, data: string) => Promise<boolean>;
    resize: (request: TerminalSessionResizeRequest) => Promise<boolean>;
    stop: (id: string) => Promise<boolean>;
    onData: (callback: (event: TerminalDataEvent) => void) => () => void;
    onExit: (callback: (event: TerminalExitEvent) => void) => () => void;
    onError: (callback: (event: TerminalErrorEvent) => void) => () => void;
  };
}
