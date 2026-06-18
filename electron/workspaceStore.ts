import fs from "node:fs";
import path from "node:path";
import type { AppSnapshot } from "../src/domain/models.js";

export class WorkspaceStore {
  private readonly filePath: string;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "workspace.json");
  }

  load(): AppSnapshot | null {
    if (!fs.existsSync(this.filePath)) {
      return null;
    }

    try {
      const parsed = JSON.parse(fs.readFileSync(this.filePath, "utf8")) as AppSnapshot;
      return this.isSnapshot(parsed) ? parsed : null;
    } catch {
      return null;
    }
  }

  save(snapshot: AppSnapshot) {
    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    fs.writeFileSync(this.filePath, JSON.stringify(snapshot, null, 2));
  }

  private isSnapshot(value: AppSnapshot) {
    return (
      Array.isArray(value?.connections) &&
      Array.isArray(value?.sessions) &&
      Array.isArray(value?.quickCommands) &&
      Array.isArray(value?.remoteFiles) &&
      Array.isArray(value?.serverMetrics)
    );
  }
}
