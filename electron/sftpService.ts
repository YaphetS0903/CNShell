import path from "node:path";
import { Client, type FileEntryWithStats } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { buildSshConnectConfig } from "./sshConnectionConfig.js";
import type {
  ListRemoteDirectoryRequest,
  RemoteDirectoryListing,
  TransferFileRequest,
  TransferFileResult
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
    const client = new Client();
    const directoryPath = path.posix.normalize(request.path || "/");

    return new Promise((resolve, reject) => {
      const closeClient = () => client.end();

      client
        .on("ready", () => {
          client.sftp((sftpError, sftp) => {
            if (sftpError) {
              closeClient();
              reject(sftpError);
              return;
            }

            sftp.readdir(directoryPath, (readError, entries) => {
              closeClient();
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
          });
        })
        .on("error", reject)
        .connect(
          buildSshConnectConfig({
            ssh: request.ssh,
            credentialStore: this.credentialStore,
            knownHostsStore: this.knownHostsStore,
            onHostKeyVerification: (event) => {
              reject(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
            }
          })
        );
    });
  }

  transferFile(request: TransferFileRequest): Promise<TransferFileResult> {
    const client = new Client();

    return new Promise((resolve, reject) => {
      const closeClient = () => client.end();

      client
        .on("ready", () => {
          client.sftp((sftpError, sftp) => {
            if (sftpError) {
              closeClient();
              reject(sftpError);
              return;
            }

            const done = (transferError: Error | null | undefined) => {
              closeClient();
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
          });
        })
        .on("error", reject)
        .connect(
          buildSshConnectConfig({
            ssh: request.ssh,
            credentialStore: this.credentialStore,
            knownHostsStore: this.knownHostsStore,
            onHostKeyVerification: (event) => {
              reject(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
            }
          })
        );
    });
  }
}
