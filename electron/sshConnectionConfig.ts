import type { ConnectConfig } from "ssh2";
import { Client } from "ssh2";
import net from "node:net";
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

export function connectSshClient(
  client: Client,
  options: BuildSshConnectConfigOptions
): Promise<{ client: Client; gateways: Client[] }> {
  const gateways = options.ssh.gateways ?? [];
  if (gateways.length === 0) {
    return connectClient(client, buildSshConnectConfig(options)).then(() => ({ client, gateways: [] }));
  }

  return connectThroughGateways(client, options);
}

function connectThroughGateways(
  targetClient: Client,
  options: BuildSshConnectConfigOptions
): Promise<{ client: Client; gateways: Client[] }> {
  return new Promise((resolve, reject) => {
    const activeGateways: Client[] = [];

    const closeGateways = () => {
      for (const gateway of [...activeGateways].reverse()) {
        gateway.end();
      }
    };

    const fail = (error: Error) => {
      targetClient.end();
      closeGateways();
      reject(error);
    };

    const connectNext = (index: number, previousClient?: Client) => {
      const gateway = options.ssh.gateways?.[index];
      if (!gateway) {
        connectTargetThroughGateway(targetClient, options, previousClient)
          .then(() => resolve({ client: targetClient, gateways: activeGateways }))
          .catch(fail);
        return;
      }

      const gatewayClient = new Client();
      const gatewaySsh: SshSessionConfig = {
        connectionId: options.ssh.connectionId,
        host: gateway.host,
        port: gateway.port,
        username: gateway.username,
        password: options.ssh.password,
        privateKey: options.ssh.privateKey,
        passphrase: options.ssh.passphrase,
        useSavedCredential: options.ssh.useSavedCredential,
        readyTimeout: options.ssh.readyTimeout
      };

      connectClientThrough(gatewayClient, gatewaySsh, previousClient, options)
        .then(() => {
          activeGateways.push(gatewayClient);
          connectNext(index + 1, gatewayClient);
        })
        .catch(fail);
    };

    connectNext(0);
  });
}

function connectTargetThroughGateway(
  targetClient: Client,
  options: BuildSshConnectConfigOptions,
  previousClient: Client | undefined
) {
  if (!previousClient) {
    return connectClient(targetClient, buildSshConnectConfig(options));
  }

  return openForwardedSocket(previousClient, options.ssh.host, options.ssh.port).then((sock) =>
    connectClient(targetClient, {
      ...buildSshConnectConfig(options),
      sock
    })
  );
}

function connectClientThrough(
  client: Client,
  ssh: SshSessionConfig,
  previousClient: Client | undefined,
  options: BuildSshConnectConfigOptions
) {
  const gatewayOptions: BuildSshConnectConfigOptions = {
    ...options,
    ssh
  };

  if (!previousClient) {
    return connectClient(client, buildSshConnectConfig(gatewayOptions));
  }

  return openForwardedSocket(previousClient, ssh.host, ssh.port).then((sock) =>
    connectClient(client, {
      ...buildSshConnectConfig(gatewayOptions),
      sock
    })
  );
}

function connectClient(client: Client, config: ConnectConfig) {
  return checkTcpReachable(config).then(() => new Promise<void>((resolve, reject) => {
    let settled = false;

    client
      .once("ready", () => {
        if (!settled) {
          settled = true;
          resolve();
        }
      })
      .once("error", (error) => {
        if (!settled) {
          settled = true;
          reject(error);
        }
      })
      .connect(config);
  }));
}

function checkTcpReachable(config: ConnectConfig) {
  if (config.sock) {
    return Promise.resolve();
  }

  const host = typeof config.host === "string" ? config.host : "";
  const port = typeof config.port === "number" ? config.port : 22;
  if (!host) {
    return Promise.resolve();
  }

  const timeoutMs = Math.min(Math.max(config.readyTimeout ?? 15000, 3000), 6000);
  return new Promise<void>((resolve, reject) => {
    let settled = false;
    const socket = net.createConnection({ host, port });

    const finish = (error?: Error) => {
      if (settled) {
        return;
      }

      settled = true;
      socket.destroy();
      if (error) {
        reject(error);
        return;
      }

      resolve();
    };

    socket.setTimeout(timeoutMs);
    socket.once("connect", () => finish());
    socket.once("timeout", () => {
      finish(
        new Error(
          `无法连接到 ${host}:${port}。TCP 端口不可达，请检查云服务器安全组、防火墙、sshd 是否启动，或确认 SSH 端口是否正确。`
        )
      );
    });
    socket.once("error", (error) => {
      finish(
        new Error(
          `无法连接到 ${host}:${port}（${error.message}）。请检查云服务器安全组、防火墙、sshd 是否启动，或确认 SSH 端口是否正确。`
        )
      );
    });
  });
}

function openForwardedSocket(client: Client, host: string, port: number) {
  return new Promise<ConnectConfig["sock"]>((resolve, reject) => {
    client.forwardOut("127.0.0.1", 0, host, port, (error, stream) => {
      if (error) {
        reject(error);
        return;
      }

      resolve(stream as ConnectConfig["sock"]);
    });
  });
}
