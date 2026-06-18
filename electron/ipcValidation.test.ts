import { describe, expect, it } from "vitest";
import {
  IpcValidationError,
  validateSaveCredential,
  validateStartRelay,
  validateStartTerminalSession,
  validateStartTunnel,
  validateTerminalWrite,
  validateWriteRemoteFile
} from "./ipcValidation.js";
import type { SshSessionConfig } from "../src/shared/ipc.js";

const ssh: SshSessionConfig = {
  connectionId: "prod-web-01",
  host: "10.24.18.11",
  port: 22,
  username: "deploy",
  password: "secret",
  useSavedCredential: false
};

describe("IPC validation contracts", () => {
  it("accepts a valid SSH terminal session request", () => {
    const request = validateStartTerminalSession({
      id: "tab-1",
      kind: "ssh",
      cols: 120,
      rows: 32,
      ssh
    });

    expect(request.ssh?.host).toBe("10.24.18.11");
    expect(request.cols).toBe(120);
  });

  it("rejects SSH terminal sessions without SSH config", () => {
    expect(() =>
      validateStartTerminalSession({
        id: "tab-1",
        kind: "ssh",
        cols: 120,
        rows: 32
      })
    ).toThrow(IpcValidationError);
  });

  it("limits terminal writes to bounded strings", () => {
    expect(validateTerminalWrite("tab-1", "uptime\r")).toEqual({ id: "tab-1", data: "uptime\r" });
    expect(() => validateTerminalWrite("tab-1", "x".repeat(1024 * 1024 + 1))).toThrow("data is too long");
  });

  it("normalizes dynamic tunnel targets and validates port ranges", () => {
    const dynamicTunnel = validateStartTunnel({
      id: "tunnel-1",
      ssh,
      mode: "dynamic",
      bindHost: "127.0.0.1",
      bindPort: 1080,
      targetHost: "",
      targetPort: 0
    });

    expect(dynamicTunnel.targetHost).toBeUndefined();
    expect(() =>
      validateStartTunnel({
        id: "tunnel-2",
        ssh,
        mode: "local",
        bindHost: "127.0.0.1",
        bindPort: 70000,
        targetHost: "127.0.0.1",
        targetPort: 80
      })
    ).toThrow(IpcValidationError);
  });

  it("requires relay target and relay ports", () => {
    expect(
      validateStartRelay({
        id: "relay-1",
        ssh,
        relayHost: "0.0.0.0",
        relayPort: 18080,
        targetHost: "127.0.0.1",
        targetPort: 8080
      })
    ).toMatchObject({ relayPort: 18080, targetPort: 8080 });

    expect(() =>
      validateStartRelay({
        id: "relay-1",
        ssh,
        relayHost: "0.0.0.0",
        relayPort: 18080,
        targetHost: "",
        targetPort: 8080
      })
    ).toThrow(IpcValidationError);
  });

  it("redacts none of the credential shape but enforces secret size contracts", () => {
    const request = validateSaveCredential({
      connectionId: "prod-web-01",
      secret: {
        password: "secret",
        privateKey: "-----BEGIN OPENSSH PRIVATE KEY-----",
        passphrase: "phrase"
      }
    });

    expect(request.secret.passphrase).toBe("phrase");
    expect(() =>
      validateSaveCredential({
        connectionId: "prod-web-01",
        secret: { privateKey: "x".repeat(512 * 1024 + 1) }
      })
    ).toThrow(IpcValidationError);
  });

  it("caps remote file writes", () => {
    expect(
      validateWriteRemoteFile({
        ssh,
        remotePath: "/etc/example.conf",
        content: "ok"
      })
    ).toMatchObject({ remotePath: "/etc/example.conf" });

    expect(() =>
      validateWriteRemoteFile({
        ssh,
        remotePath: "/tmp/huge",
        content: "x".repeat(5 * 1024 * 1024 + 1)
      })
    ).toThrow(IpcValidationError);
  });
});
