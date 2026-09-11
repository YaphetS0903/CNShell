import { create } from "zustand";
import { api } from "../lib/api";
import {
  canLeaveEditor,
  type RemoteEditorTarget,
} from "../lib/editor-lifecycle";
import {
  updateTransferMetric,
  type TransferMetric,
} from "../lib/runtime-metrics";
import {
  defaultSettings,
  type AppSettings,
  type ConnectionProfile,
  type MonitorSnapshot,
  type TerminalSession,
  type TransferTask,
} from "../types";

const pendingSessionUpdates = new Map<string, Partial<TerminalSession>>();
const MAX_PENDING_SESSION_UPDATES = 128;

function cachePendingSessionUpdate(
  id: string,
  patch: Partial<TerminalSession>,
) {
  pendingSessionUpdates.set(id, { ...pendingSessionUpdates.get(id), ...patch });
  while (pendingSessionUpdates.size > MAX_PENDING_SESSION_UPDATES) {
    const oldest = pendingSessionUpdates.keys().next().value;
    if (oldest === undefined) break;
    pendingSessionUpdates.delete(oldest);
  }
}

type Panel = "files" | "commands" | "transfers" | "system";

export interface SettingsNavigationTarget {
  category: "basic" | "connection" | "automation" | "team" | "support";
  moduleId?: string;
  basicId?: string;
}

interface AppState {
  connections: ConnectionProfile[];
  sessions: TerminalSession[];
  activeSessionId: string | null;
  activePanel: Panel;
  monitor: MonitorSnapshot | null;
  transfers: TransferTask[];
  transferMetrics: Record<string, TransferMetric>;
  remoteEditor: RemoteEditorTarget | null;
  settings: AppSettings;
  connectionEditorOpen: boolean;
  editingConnection: ConnectionProfile | null;
  settingsOpen: boolean;
  settingsTarget: SettingsNavigationTarget | null;
  helpOpen: boolean;
  loading: boolean;
  error: string | null;
  bootstrap: () => Promise<void>;
  refreshConnections: () => Promise<void>;
  setActiveSession: (id: string) => void;
  addSession: (session: TerminalSession) => void;
  updateSession: (id: string, patch: Partial<TerminalSession>) => void;
  removeSession: (id: string) => void;
  prepareCloseSession: (id: string) => boolean;
  openTextEditor: (target: RemoteEditorTarget) => void;
  closeTextEditor: () => void;
  setPanel: (panel: Panel) => void;
  setMonitor: (snapshot: MonitorSnapshot | null) => void;
  setTransfers: (tasks: TransferTask[]) => void;
  upsertTransfer: (task: TransferTask) => void;
  addTransfer: (task: TransferTask) => void;
  openConnectionEditor: (connection?: ConnectionProfile | null) => void;
  closeConnectionEditor: () => void;
  setSettingsOpen: (
    open: boolean,
    target?: SettingsNavigationTarget | null,
  ) => void;
  setHelpOpen: (open: boolean) => void;
  saveSettings: (settings: AppSettings) => Promise<void>;
  setError: (error: string | null) => void;
}

export const useAppStore = create<AppState>((set, get) => {
  let settingsQueue = Promise.resolve();
  let settingsRevision = 0;
  let pendingSettings = 0;
  let confirmedSettings = defaultSettings;
  return {
    connections: [],
    sessions: [],
    activeSessionId: null,
    activePanel: "files",
    monitor: null,
    transfers: [],
    transferMetrics: {},
    remoteEditor: null,
    settings: defaultSettings,
    connectionEditorOpen: false,
    editingConnection: null,
    settingsOpen: false,
    settingsTarget: null,
    helpOpen: false,
    loading: true,
    error: null,
    bootstrap: async () => {
      set({ loading: true });
      try {
        const [connections, settings] = await Promise.all([
          api.listConnections(),
          api.getSettings(),
        ]);
        set({ connections, settings, loading: false });
      } catch (error) {
        set({ error: String(error), loading: false });
      }
    },
    refreshConnections: async () =>
      set({ connections: await api.listConnections() }),
    setActiveSession: (activeSessionId) => set({ activeSessionId }),
    addSession: (session) =>
      set((state) => {
        const pending = pendingSessionUpdates.get(session.id);
        pendingSessionUpdates.delete(session.id);
        return {
          sessions: [...state.sessions, { ...session, ...pending }],
          activeSessionId: session.id,
        };
      }),
    updateSession: (id, patch) =>
      set((state) => {
        if (!state.sessions.some((session) => session.id === id)) {
          cachePendingSessionUpdate(id, patch);
          return state;
        }
        return {
          sessions: state.sessions.map((session) =>
            session.id === id ? { ...session, ...patch } : session,
          ),
        };
      }),
    prepareCloseSession: (id) => {
      if (get().remoteEditor?.sessionId !== id) return true;
      if (!canLeaveEditor()) return false;
      set({ remoteEditor: null });
      return true;
    },
    openTextEditor: (target) => {
      const current = get().remoteEditor;
      if (
        current?.sessionId === target.sessionId &&
        current.connectionId === target.connectionId &&
        current.path === target.path
      )
        return;
      if (current && !canLeaveEditor()) return;
      set({ remoteEditor: target });
    },
    closeTextEditor: () => set({ remoteEditor: null }),
    removeSession: (id) => {
      if (!get().prepareCloseSession(id)) return;
      set((state) => {
        const sessions = state.sessions.filter((session) => session.id !== id);
        return {
          sessions,
          activeSessionId:
            state.activeSessionId === id
              ? (sessions.at(-1)?.id ?? null)
              : state.activeSessionId,
        };
      });
    },
    setPanel: (activePanel) => set({ activePanel }),
    setMonitor: (monitor) => set({ monitor }),
    setTransfers: (transfers) => set({ transfers }),
    // Enqueue/retry responses can arrive after a newer progress event.
    addTransfer: (task) => {
      if (!get().transfers.some((item) => item.id === task.id))
        get().upsertTransfer(task);
    },
    upsertTransfer: (task) =>
      set((state) => {
        const previous = state.transfers.find((item) => item.id === task.id);
        const metric = updateTransferMetric(
          previous?.status === "running" && task.status === "running"
            ? state.transferMetrics[task.id]
            : undefined,
          task,
          performance.now(),
        );
        return {
          transfers: previous
            ? state.transfers.map((item) => (item.id === task.id ? task : item))
            : [...state.transfers, task],
          transferMetrics: { ...state.transferMetrics, [task.id]: metric },
        };
      }),
    openConnectionEditor: (connection = null) =>
      set({ editingConnection: connection, connectionEditorOpen: true }),
    closeConnectionEditor: () =>
      set({ connectionEditorOpen: false, editingConnection: null }),
    setSettingsOpen: (settingsOpen, settingsTarget = null) =>
      set({ settingsOpen, settingsTarget }),
    setHelpOpen: (helpOpen) => set({ helpOpen }),
    saveSettings: (settings) => {
      if (pendingSettings === 0) confirmedSettings = get().settings;
      pendingSettings += 1;
      const revision = ++settingsRevision;
      set({ settings });
      // Persist in user-action order; only the latest request may roll back the UI.
      const operation = settingsQueue.then(async () => {
        try {
          await api.saveSettings(settings);
          confirmedSettings = settings;
        } catch (error) {
          if (revision === settingsRevision)
            set({ settings: confirmedSettings });
          throw error;
        } finally {
          pendingSettings -= 1;
        }
      });
      settingsQueue = operation.catch(() => undefined);
      return operation;
    },
    setError: (error) => set({ error }),
  };
});
