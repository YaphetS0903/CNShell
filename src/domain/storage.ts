import type { AppSnapshot } from "./models";

const SNAPSHOT_KEY = "cnshell.workspace.snapshot.v1";

export interface WorkspaceStorage {
  loadSnapshot(): AppSnapshot | null;
  saveSnapshot(snapshot: AppSnapshot): void;
}

export function createLocalWorkspaceStorage(): WorkspaceStorage {
  return {
    loadSnapshot() {
      const storedValue = window.localStorage.getItem(SNAPSHOT_KEY);
      if (!storedValue) {
        return null;
      }

      try {
        return JSON.parse(storedValue) as AppSnapshot;
      } catch {
        window.localStorage.removeItem(SNAPSHOT_KEY);
        return null;
      }
    },
    saveSnapshot(snapshot) {
      window.localStorage.setItem(SNAPSHOT_KEY, JSON.stringify(snapshot));
    }
  };
}
