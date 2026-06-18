import fs from "node:fs";
import crypto from "node:crypto";
import path from "node:path";
import { safeStorage } from "electron";
import type {
  CredentialSecret,
  CredentialStatus,
  CredentialVaultStatus,
  DisableCredentialVaultRequest,
  EnableCredentialVaultRequest,
  SaveCredentialRequest,
  UnlockCredentialVaultRequest
} from "../src/shared/ipc.js";

interface StoredCredential {
  id: string;
  connectionId: string;
  encryptedSecretBase64: string;
  protection?: "system" | "master";
  updatedAt: string;
}

interface MasterVaultEnvelope {
  version: 1;
  kdf: "pbkdf2-sha256";
  iterations: number;
  saltBase64: string;
  verifierIvBase64: string;
  verifierTagBase64: string;
  verifierCiphertextBase64: string;
  updatedAt: string;
}

interface WrappedMasterSecret {
  version: 1;
  saltBase64: string;
  ivBase64: string;
  tagBase64: string;
  ciphertextBase64: string;
}

const MASTER_KDF_ITERATIONS = 210_000;
const MASTER_VERIFIER_TEXT = "CNshell credential vault";

export class CredentialStore {
  private readonly filePath: string;
  private readonly vaultPath: string;
  private masterKey: Buffer | null = null;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "credentials.json");
    this.vaultPath = path.join(userDataPath, "credential-vault.json");
  }

  getStatus(connectionId: string): CredentialStatus {
    const credential = this.findByConnectionId(connectionId);
    const vaultStatus = this.getVaultStatus();
    const protection = credential?.protection ?? "system";

    return {
      connectionId,
      hasCredential: Boolean(credential),
      encryptionAvailable: safeStorage.isEncryptionAvailable(),
      protection: credential ? protection : "none",
      vaultLocked: protection === "master" && vaultStatus.locked,
      updatedAt: credential?.updatedAt
    };
  }

  save(request: SaveCredentialRequest): CredentialStatus {
    this.assertEncryptionAvailable();
    const vault = this.readVault();
    if (vault && !this.masterKey) {
      throw new Error("Credential vault is locked.");
    }

    const entries = this.read().filter((entry) => entry.connectionId !== request.connectionId);
    const updatedAt = new Date().toISOString();
    const serializedSecret = JSON.stringify(request.secret);
    const encryptedSecret = safeStorage.encryptString(vault ? this.wrapWithMasterKey(serializedSecret) : serializedSecret);

    entries.push({
      id: request.connectionId,
      connectionId: request.connectionId,
      encryptedSecretBase64: encryptedSecret.toString("base64"),
      protection: vault ? "master" : "system",
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
    const serializedSecret =
      credential.protection === "master" ? this.unwrapWithMasterKey(decrypted) : this.readLegacyOrPlainSecret(decrypted);
    return JSON.parse(serializedSecret) as CredentialSecret;
  }

  getVaultStatus(): CredentialVaultStatus {
    const vault = this.readVault();
    return {
      mode: vault ? "master" : "system",
      locked: Boolean(vault && !this.masterKey),
      encryptionAvailable: safeStorage.isEncryptionAvailable(),
      updatedAt: vault?.updatedAt
    };
  }

  enableVault(request: EnableCredentialVaultRequest): CredentialVaultStatus {
    this.assertEncryptionAvailable();
    const masterPassword = request.masterPassword.trim();
    if (masterPassword.length < 8) {
      throw new Error("Master password must be at least 8 characters.");
    }

    if (this.readVault()) {
      this.unlockVault({ masterPassword });
      return this.getVaultStatus();
    }

    const salt = crypto.randomBytes(16);
    const key = this.deriveMasterKey(masterPassword, salt);
    const verifier = this.encryptWithKey(key, MASTER_VERIFIER_TEXT);
    const vault: MasterVaultEnvelope = {
      version: 1,
      kdf: "pbkdf2-sha256",
      iterations: MASTER_KDF_ITERATIONS,
      saltBase64: salt.toString("base64"),
      verifierIvBase64: verifier.ivBase64,
      verifierTagBase64: verifier.tagBase64,
      verifierCiphertextBase64: verifier.ciphertextBase64,
      updatedAt: new Date().toISOString()
    };

    this.writeVault(vault);
    this.masterKey = key;
    this.rewrapCredentials("system", "master");
    return this.getVaultStatus();
  }

  unlockVault(request: UnlockCredentialVaultRequest): CredentialVaultStatus {
    this.assertEncryptionAvailable();
    const vault = this.readVault();
    if (!vault) {
      return this.getVaultStatus();
    }

    const key = this.deriveMasterKey(request.masterPassword, Buffer.from(vault.saltBase64, "base64"), vault.iterations);
    try {
      const verifier = this.decryptWithKey(key, {
        ivBase64: vault.verifierIvBase64,
        tagBase64: vault.verifierTagBase64,
        ciphertextBase64: vault.verifierCiphertextBase64
      });
      if (verifier !== MASTER_VERIFIER_TEXT) {
        throw new Error("Invalid master password.");
      }
    } catch {
      throw new Error("Invalid master password.");
    }

    this.masterKey = key;
    return this.getVaultStatus();
  }

  disableVault(request: DisableCredentialVaultRequest): CredentialVaultStatus {
    const vault = this.readVault();
    if (!vault) {
      return this.getVaultStatus();
    }

    if (!this.masterKey) {
      if (!request.masterPassword) {
        throw new Error("Master password is required to disable the vault.");
      }
      this.unlockVault({ masterPassword: request.masterPassword });
    }

    this.rewrapCredentials("master", "system");
    this.masterKey = null;
    if (fs.existsSync(this.vaultPath)) {
      fs.rmSync(this.vaultPath);
    }
    return this.getVaultStatus();
  }

  lockVault(): CredentialVaultStatus {
    if (this.readVault()) {
      this.masterKey = null;
    }

    return this.getVaultStatus();
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

  private readVault(): MasterVaultEnvelope | null {
    if (!fs.existsSync(this.vaultPath)) {
      return null;
    }

    try {
      const parsed = JSON.parse(fs.readFileSync(this.vaultPath, "utf8")) as MasterVaultEnvelope;
      return parsed.version === 1 && parsed.kdf === "pbkdf2-sha256" ? parsed : null;
    } catch {
      return null;
    }
  }

  private writeVault(vault: MasterVaultEnvelope) {
    fs.mkdirSync(path.dirname(this.vaultPath), { recursive: true });
    fs.writeFileSync(this.vaultPath, JSON.stringify(vault, null, 2));
  }

  private deriveMasterKey(password: string, salt: Buffer, iterations = MASTER_KDF_ITERATIONS) {
    return crypto.pbkdf2Sync(password, salt, iterations, 32, "sha256");
  }

  private encryptWithKey(key: Buffer, plainText: string) {
    const iv = crypto.randomBytes(12);
    const cipher = crypto.createCipheriv("aes-256-gcm", key, iv);
    const ciphertext = Buffer.concat([cipher.update(plainText, "utf8"), cipher.final()]);
    const tag = cipher.getAuthTag();
    return {
      ivBase64: iv.toString("base64"),
      tagBase64: tag.toString("base64"),
      ciphertextBase64: ciphertext.toString("base64")
    };
  }

  private decryptWithKey(
    key: Buffer,
    payload: { ivBase64: string; tagBase64: string; ciphertextBase64: string }
  ) {
    const decipher = crypto.createDecipheriv("aes-256-gcm", key, Buffer.from(payload.ivBase64, "base64"));
    decipher.setAuthTag(Buffer.from(payload.tagBase64, "base64"));
    const decrypted = Buffer.concat([
      decipher.update(Buffer.from(payload.ciphertextBase64, "base64")),
      decipher.final()
    ]);
    return decrypted.toString("utf8");
  }

  private wrapWithMasterKey(plainText: string) {
    if (!this.masterKey) {
      throw new Error("Credential vault is locked.");
    }

    const salt = crypto.randomBytes(16);
    const key = crypto.createHmac("sha256", this.masterKey).update(salt).digest().subarray(0, 32);
    const encrypted = this.encryptWithKey(key, plainText);
    const wrapped: WrappedMasterSecret = {
      version: 1,
      saltBase64: salt.toString("base64"),
      ...encrypted
    };
    return JSON.stringify(wrapped);
  }

  private unwrapWithMasterKey(serializedWrappedSecret: string) {
    if (!this.masterKey) {
      throw new Error("Credential vault is locked.");
    }

    const wrapped = JSON.parse(serializedWrappedSecret) as WrappedMasterSecret;
    const salt = Buffer.from(wrapped.saltBase64, "base64");
    const key = crypto.createHmac("sha256", this.masterKey).update(salt).digest().subarray(0, 32);
    return this.decryptWithKey(key, wrapped);
  }

  private readLegacyOrPlainSecret(decrypted: string) {
    try {
      const parsed = JSON.parse(decrypted) as Partial<WrappedMasterSecret>;
      if (parsed.version === 1 && parsed.saltBase64 && parsed.ivBase64 && parsed.tagBase64 && parsed.ciphertextBase64) {
        return this.unwrapWithMasterKey(decrypted);
      }
    } catch {
      return decrypted;
    }

    return decrypted;
  }

  private rewrapCredentials(fromProtection: "system" | "master", toProtection: "system" | "master") {
    const entries = this.read();
    const nextEntries = entries.map((entry) => {
      const currentProtection = entry.protection ?? "system";
      if (currentProtection !== fromProtection) {
        return entry;
      }

      const plainText = safeStorage.decryptString(Buffer.from(entry.encryptedSecretBase64, "base64"));
      const serializedSecret = fromProtection === "master" ? this.unwrapWithMasterKey(plainText) : plainText;
      const nextPayload = toProtection === "master" ? this.wrapWithMasterKey(serializedSecret) : serializedSecret;
      return {
        ...entry,
        protection: toProtection,
        encryptedSecretBase64: safeStorage.encryptString(nextPayload).toString("base64"),
        updatedAt: new Date().toISOString()
      };
    });

    this.write(nextEntries);
  }
}
