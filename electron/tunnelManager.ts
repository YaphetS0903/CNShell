import net from "node:net";
import { Client } from "ssh2";
import type { ClientChannel, TcpConnectionDetails } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { connectSshClient } from "./sshConnectionConfig.js";
import type { StartTunnelRequest, TunnelInfo } from "../src/shared/ipc.js";

interface ActiveTunnel {
  info: TunnelInfo;
  client: Client;
  gateways: Client[];
  close: () => void;
}

interface SocksDestination {
  host: string;
  port: number;
}

export class TunnelManager {
  private readonly tunnels = new Map<string, ActiveTunnel>();

  constructor(
    private readonly knownHostsStore: KnownHostsStore | null,
    private readonly credentialStore: CredentialStore | null
  ) {}

  start(request: StartTunnelRequest): Promise<TunnelInfo> {
    if (this.tunnels.has(request.id)) {
      this.stop(request.id);
    }

    const client = new Client();
    const info: TunnelInfo = {
      id: request.id,
      mode: request.mode,
      bindHost: request.bindHost,
      bindPort: request.bindPort,
      targetHost: request.targetHost,
      targetPort: request.targetPort,
      status: "starting"
    };

    return new Promise((resolve, reject) => {
      let settled = false;
      const fail = (error: Error) => {
        if (!settled) {
          settled = true;
          client.end();
          reject(error);
        }
      };

      connectSshClient(client, {
        ssh: request.ssh,
        credentialStore: this.credentialStore,
        knownHostsStore: this.knownHostsStore,
        onHostKeyVerification: (event) => {
          fail(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
        }
      })
        .then(({ gateways }) => {
          if (request.mode === "remote") {
            this.startRemoteTunnel(request, client, gateways, info, resolve, fail);
            return;
          }

          this.startLocalServerTunnel(request, client, gateways, info, resolve, fail);
        })
        .catch(fail);
    });
  }

  stop(id: string) {
    const tunnel = this.tunnels.get(id);
    if (!tunnel) {
      return false;
    }

    tunnel.close();
    this.tunnels.delete(id);
    return true;
  }

  private startLocalServerTunnel(
    request: StartTunnelRequest,
    client: Client,
    gateways: Client[],
    info: TunnelInfo,
    resolve: (info: TunnelInfo) => void,
    reject: (error: Error) => void
  ) {
    const server = net.createServer((socket) => {
      if (request.mode === "dynamic") {
        this.handleSocksConnection(socket, client);
        return;
      }

      if (!request.targetHost || !request.targetPort) {
        socket.destroy(new Error("Local tunnel target is missing."));
        return;
      }

      this.forwardSocket(socket, client, request.targetHost, request.targetPort);
    });

    server.on("error", (error) => {
      info.status = "error";
      info.message = error.message;
      reject(error);
    });

    server.listen(request.bindPort, request.bindHost, () => {
      const address = server.address();
      info.status = "running";
      info.bindPort = typeof address === "object" && address ? address.port : request.bindPort;
      this.tunnels.set(request.id, {
        info,
        client,
        gateways,
        close: () => {
          server.close();
          client.end();
          for (const gateway of gateways) {
            gateway.end();
          }
        }
      });
      resolve(info);
    });
  }

  private startRemoteTunnel(
    request: StartTunnelRequest,
    client: Client,
    gateways: Client[],
    info: TunnelInfo,
    resolve: (info: TunnelInfo) => void,
    reject: (error: Error) => void
  ) {
    if (!request.targetHost || !request.targetPort) {
      reject(new Error("Remote tunnel target is missing."));
      return;
    }

    const onTcpConnection = (details: TcpConnectionDetails, accept: () => ClientChannel, rejectConnection: () => void) => {
      if (details.destPort !== info.bindPort) {
        return;
      }

      const localSocket = net.connect(request.targetPort ?? 0, request.targetHost);
      localSocket.on("connect", () => {
        const channel = accept();
        channel.pipe(localSocket);
        localSocket.pipe(channel);
      });
      localSocket.on("error", () => {
        rejectConnection();
        localSocket.destroy();
      });
    };

    client.on("tcp connection", onTcpConnection);
    client.forwardIn(request.bindHost, request.bindPort, (error, assignedPort) => {
      if (error) {
        client.off("tcp connection", onTcpConnection);
        reject(error);
        return;
      }

      info.status = "running";
      info.bindPort = assignedPort;
      this.tunnels.set(request.id, {
        info,
        client,
        gateways,
        close: () => {
          client.off("tcp connection", onTcpConnection);
          client.unforwardIn(request.bindHost, info.bindPort, () => {
            client.end();
            for (const gateway of gateways) {
              gateway.end();
            }
          });
        }
      });
      resolve(info);
    });
  }

  private handleSocksConnection(socket: net.Socket, client: Client) {
    socket.once("data", (greeting) => {
      if (greeting.length < 3 || greeting[0] !== 0x05) {
        socket.destroy();
        return;
      }

      socket.write(Buffer.from([0x05, 0x00]));
      socket.once("data", (request) => {
        const destination = this.parseSocksDestination(request);
        if (!destination) {
          socket.write(Buffer.from([0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]));
          socket.destroy();
          return;
        }

        client.forwardOut(
          socket.remoteAddress ?? "127.0.0.1",
          socket.remotePort ?? 0,
          destination.host,
          destination.port,
          (error, stream) => {
            if (error) {
              socket.write(Buffer.from([0x05, 0x01, 0x00, 0x01, 0, 0, 0, 0, 0, 0]));
              socket.destroy();
              return;
            }

            socket.write(Buffer.from([0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]));
            socket.pipe(stream);
            stream.pipe(socket);
          }
        );
      });
    });
  }

  private forwardSocket(socket: net.Socket, client: Client, targetHost: string, targetPort: number) {
    client.forwardOut(socket.remoteAddress ?? "127.0.0.1", socket.remotePort ?? 0, targetHost, targetPort, (error, stream) => {
      if (error) {
        socket.destroy(error);
        return;
      }

      socket.pipe(stream);
      stream.pipe(socket);
    });
  }

  private parseSocksDestination(request: Buffer): SocksDestination | null {
    if (request.length < 7 || request[0] !== 0x05 || request[1] !== 0x01) {
      return null;
    }

    const addressType = request[3];
    if (addressType === 0x01) {
      if (request.length < 10) {
        return null;
      }

      return {
        host: Array.from(request.subarray(4, 8)).join("."),
        port: request.readUInt16BE(8)
      };
    }

    if (addressType === 0x03) {
      const hostLength = request[4];
      const portOffset = 5 + hostLength;
      if (request.length < portOffset + 2) {
        return null;
      }

      return {
        host: request.subarray(5, portOffset).toString("utf8"),
        port: request.readUInt16BE(portOffset)
      };
    }

    if (addressType === 0x04) {
      if (request.length < 22) {
        return null;
      }

      const parts: string[] = [];
      for (let index = 0; index < 8; index += 1) {
        parts.push(request.readUInt16BE(4 + index * 2).toString(16));
      }

      return {
        host: parts.join(":"),
        port: request.readUInt16BE(20)
      };
    }

    return null;
  }
}
