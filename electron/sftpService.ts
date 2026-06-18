import path from "node:path";
import { Client, type FileEntryWithStats, type SFTPWrapper } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { connectSshClient } from "./sshConnectionConfig.js";
import type {
  ListRemoteDirectoryRequest,
  ReadRemoteFileRequest,
  ReadRemoteFileResult,
  RemoteDirectoryListing,
  TransferFileRequest,
  TransferFileResult,
  WriteRemoteFileRequest,
  WriteRemoteFileResult
} from "../src/shared/ipc.js";
import type { RemoteFileEntry } from "../src/domain/models.js";

function toRemotePath(parent: string, filename: string) {
  if (parent === "/") {
    return `/${filename}`;
  }

  return `${parent.replace(/\/$/, "")}/${filename}`;
}

function toModeString(entry: FileEntryWithStats) {
  return entry.longname.split(/\s+/).at(0) ?? "";
}

function toRemoteFileEntry(parent: string, entry: FileEntryWithStats): RemoteFileEntry {
  const type = entry.attrs.isDirectory() ? "directory" : entry.attrs.isSymbolicLink() ? "symlink" : "file";

  return {
    id: toRemotePath(parent, entry.filename),
    name: entry.filename,
    path: toRemotePath(parent, entry.filename),
    type,
    size: entry.attrs.size,
    modifiedAt: new Date(entry.attrs.mtime * 1000).toISOString(),
    mode: toModeString(entry)
  };
}

export class SftpService {
  constructor(
    private readonly knownHostsStore: KnownHostsStore | null,
    private readonly credentialStore: CredentialStore | null
  ) {}

  listDirectory(request: ListRemoteDirectoryRequest): Promise<RemoteDirectoryListing> {
    const directoryPath = path.posix.normalize(request.path || "/");

    return this.withSftp(request.ssh, (sftp) =>
      new Promise((resolve, reject) => {
        sftp.readdir(directoryPath, (readError, entries) => {
          if (readError) {
            reject(readError);
            return;
          }

          resolve({
            path: directoryPath,
            entries: entries
              .filter((entry) => entry.filename !== "." && entry.filename !== "..")
              .map((entry) => toRemoteFileEntry(directoryPath, entry))
              .sort((a, b) => {
                if (a.type !== b.type) {
                  return a.type === "directory" ? -1 : 1;
                }

                return a.name.localeCompare(b.name);
              })
          });
        });
      })
    );
  }

  transferFile(request: TransferFileRequest): Promise<TransferFileResult> {
    return this.withSftp(
      request.ssh,
      (sftp) =>
        new Promise((resolve, reject) => {
          const done = (transferError: Error | null | undefined) => {
            if (transferError) {
              reject(transferError);
              return;
            }

            resolve({ ok: true });
          };

          if (request.direction === "upload") {
            sftp.fastPut(request.localPath, request.remotePath, done);
          } else {
            sftp.fastGet(request.remotePath, request.localPath, done);
          }
        })
    );
  }

  readFile(request: ReadRemoteFileRequest): Promise<ReadRemoteFileResult> {
    return this.withSftp(
      request.ssh,
      (sftp) =>
        new Promise((resolve, reject) => {
          sftp.readFile(request.remotePath, "utf8", (error, content) => {
            if (error) {
              reject(error);
              return;
            }

            resolve({
              remotePath: request.remotePath,
              content: content.toString()
            });
          });
        })
    );
  }

  writeFile(request: WriteRemoteFileRequest): Promise<WriteRemoteFileResult> {
    return this.withSftp(
      request.ssh,
      (sftp) =>
        new Promise((resolve, reject) => {
          sftp.writeFile(request.remotePath, request.content, "utf8", (error) => {
            if (error) {
              reject(error);
              return;
            }

            resolve({ ok: true });
          });
        })
    );
  }

  private withSftp<T>(ssh: TransferFileRequest["ssh"], action: (sftp: SFTPWrapper) => Promise<T>) {
    const client = new Client();

    return new Promise<T>((resolve, reject) => {
      let gateways: Client[] = [];
      const closeClient = () => {
        client.end();
        for (const gateway of gateways) {
          gateway.end();
        }
      };

      connectSshClient(client, {
        ssh,
        credentialStore: this.credentialStore,
        knownHostsStore: this.knownHostsStore,
        onHostKeyVerification: (event) => {
          reject(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
        }
      })
        .then((connected) => {
          gateways = connected.gateways;
          client.sftp((sftpError, sftp) => {
            if (sftpError) {
              closeClient();
              reject(sftpError);
              return;
            }

            action(sftp).then(resolve).catch(reject).finally(closeClient);
          });
        })
        .catch(reject);
    });
  }
}
