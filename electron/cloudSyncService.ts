import fs from "node:fs";
import { dialog, safeStorage } from "electron";
import type { AppSnapshot } from "../src/domain/models.js";
import type { CloudSyncResult, ExportCloudSyncRequest } from "../src/shared/ipc.js";

interface CloudSyncPackage {
  version: 1;
  exportedAt: string;
  encryptedSnapshotBase64: string;
}

export class CloudSyncService {
  async exportSettings(request: ExportCloudSyncRequest): Promise<CloudSyncResult> {
    this.assertEncryptionAvailable();

    const result = await dialog.showSaveDialog({
      title: "Export encrypted CNshell settings",
      defaultPath: "cnshell-settings.cnsync",
      filters: [{ name: "CNshell Sync Package", extensions: ["cnsync"] }]
    });

    if (result.canceled || !result.filePath) {
      return { ok: false };
    }

    const encryptedSnapshot = safeStorage.encryptString(JSON.stringify(request.snapshot));
    const payload: CloudSyncPackage = {
      version: 1,
      exportedAt: new Date().toISOString(),
      encryptedSnapshotBase64: encryptedSnapshot.toString("base64")
    };

    fs.writeFileSync(result.filePath, JSON.stringify(payload, null, 2));
    return { ok: true, path: result.filePath };
  }

  async importSettings(): Promise<CloudSyncResult> {
    this.assertEncryptionAvailable();

    const result = await dialog.showOpenDialog({
      title: "Import encrypted CNshell settings",
      properties: ["openFile"],
      filters: [{ name: "CNshell Sync Package", extensions: ["cnsync"] }]
    });

    const filePath = result.filePaths[0];
    if (result.canceled || !filePath) {
      return { ok: false };
    }

    const payload = JSON.parse(fs.readFileSync(filePath, "utf8")) as CloudSyncPackage;
    const decrypted = safeStorage.decryptString(Buffer.from(payload.encryptedSnapshotBase64, "base64"));
    return {
      ok: true,
      path: filePath,
      importedSnapshot: JSON.parse(decrypted) as AppSnapshot
    };
  }

  private assertEncryptionAvailable() {
    if (!safeStorage.isEncryptionAvailable()) {
      throw new Error("System encryption is not available on this machine.");
    }
  }
}
