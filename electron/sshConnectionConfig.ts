import type { ConnectConfig } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import type { SshSessionConfig } from "../src/shared/ipc.js";

interface BuildSshConnectConfigOptions {
  ssh: SshSessionConfig;
  credentialStore: CredentialStore | null;
  knownHostsStore: KnownHostsStore | null;
  onHostKeyVerification?: (event: {
    status: "unknown" | "changed";
    host: string;
    port: number;
    fingerprint: string;
    keyBase64: string;
    expectedFingerprint?: string;
  }) => void;
}

export function buildSshConnectConfig({
  ssh,
  credentialStore,
  knownHostsStore,
  onHostKeyVerification
}: BuildSshConnectConfigOptions): ConnectConfig {
  const savedSecret = ssh.useSavedCredential ? credentialStore?.loadSecret(ssh.connectionId) : undefined;
  const password = ssh.password || savedSecret?.password;
  const privateKey = ssh.privateKey || savedSecret?.privateKey;
  const passphrase = ssh.passphrase || savedSecret?.passphrase;

  if (!password && !privateKey) {
    throw new Error("SSH password or private key is required.");
  }

  return {
    host: ssh.host,
    port: ssh.port,
    username: ssh.username,
    password,
    privateKey,
    passphrase,
    readyTimeout: ssh.readyTimeout ?? 15000,
    keepaliveInterval: 15000,
    hostVerifier: (key: Buffer) => {
      if (!knownHostsStore) {
        return false;
      }

      const verification = knownHostsStore.verifyHostKey(ssh.host, ssh.port, key);
      if (verification.status === "trusted") {
        return true;
      }

      onHostKeyVerification?.({
        status: verification.status,
        host: verification.host,
        port: verification.port,
        fingerprint: verification.fingerprint,
        expectedFingerprint: verification.expectedFingerprint,
        keyBase64: key.toString("base64")
      });

      return false;
    }
  };
}
