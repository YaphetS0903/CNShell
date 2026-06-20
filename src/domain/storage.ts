import type { AppSnapshot } from "./models";

const SNAPSHOT_KEY = "cnshell.workspace.snapshot.v1";

export interface WorkspaceStorage {
  loadSnapshot(): Promise<AppSnapshot | null>;
  saveSnapshot(snapshot: AppSnapshot): Promise<boolean>;
}

export function createLocalWorkspaceStorage(): WorkspaceStorage {
  return {
    async loadSnapshot() {
      const loadLocalSnapshot = () => {
        try {
          const storedValue = window.localStorage.getItem(SNAPSHOT_KEY);
          return storedValue ? (JSON.parse(storedValue) as AppSnapshot) : null;
        } catch {
          window.localStorage.removeItem(SNAPSHOT_KEY);
          return null;
        }
      };

      if (window.cnshell?.workspace) {
        const desktopSnapshot = await window.cnshell.workspace.load();
        if (desktopSnapshot) {
          return desktopSnapshot;
        }

        const legacySnapshot = loadLocalSnapshot();
        if (legacySnapshot) {
          await window.cnshell.workspace.save(legacySnapshot);
        }
        return legacySnapshot;
      }

      return loadLocalSnapshot();
    },
    saveSnapshot(snapshot) {
      if (window.cnshell?.workspace) {
        window.localStorage.setItem(SNAPSHOT_KEY, JSON.stringify(snapshot));
        return window.cnshell.workspace.save(snapshot);
      }

      window.localStorage.setItem(SNAPSHOT_KEY, JSON.stringify(snapshot));
      return Promise.resolve(true);
    }
  };
}
