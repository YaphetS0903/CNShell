import fs from "node:fs";
import path from "node:path";
import { safeStorage } from "electron";
import type { CredentialSecret, CredentialStatus, SaveCredentialRequest } from "../src/shared/ipc.js";

interface StoredCredential {
  id: string;
  connectionId: string;
  encryptedSecretBase64: string;
  updatedAt: string;
}

export class CredentialStore {
  private readonly filePath: string;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "credentials.json");
  }

  getStatus(connectionId: string): CredentialStatus {
    const credential = this.findByConnectionId(connectionId);

    return {
      connectionId,
      hasCredential: Boolean(credential),
      encryptionAvailable: safeStorage.isEncryptionAvailable(),
      updatedAt: credential?.updatedAt
    };
  }

  save(request: SaveCredentialRequest): CredentialStatus {
    this.assertEncryptionAvailable();

    const entries = this.read().filter((entry) => entry.connectionId !== request.connectionId);
    const updatedAt = new Date().toISOString();
    const encryptedSecret = safeStorage.encryptString(JSON.stringify(request.secret));

    entries.push({
      id: request.connectionId,
      connectionId: request.connectionId,
      encryptedSecretBase64: encryptedSecret.toString("base64"),
      updatedAt
    });

    this.write(entries);
    return this.getStatus(request.connectionId);
  }

  delete(connectionId: string): CredentialStatus {
    const entries = this.read().filter((entry) => entry.connectionId !== connectionId);
    this.write(entries);
    return this.getStatus(connectionId);
  }

  loadSecret(connectionId: string): CredentialSecret | undefined {
    this.assertEncryptionAvailable();

    const credential = this.findByConnectionId(connectionId);
    if (!credential) {
      return undefined;
    }

    const decrypted = safeStorage.decryptString(Buffer.from(credential.encryptedSecretBase64, "base64"));
    return JSON.parse(decrypted) as CredentialSecret;
  }

  private assertEncryptionAvailable() {
    if (!safeStorage.isEncryptionAvailable()) {
      throw new Error("System credential encryption is not available on this machine.");
    }
  }

  private findByConnectionId(connectionId: string) {
    return this.read().find((entry) => entry.connectionId === connectionId);
  }

  private read(): StoredCredential[] {
    if (!fs.existsSync(this.filePath)) {
      return [];
    }

    try {
      const parsed = JSON.parse(fs.readFileSync(this.filePath, "utf8")) as StoredCredential[];
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  private write(entries: StoredCredential[]) {
    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    fs.writeFileSync(this.filePath, JSON.stringify(entries, null, 2));
  }
}
