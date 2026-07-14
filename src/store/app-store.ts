import { create } from "zustand";
import { api } from "../lib/api";
import { defaultSettings, type AppSettings, type ConnectionProfile, type MonitorSnapshot, type TerminalSession, type TransferTask } from "../types";

const pendingSessionUpdates = new Map<string, Partial<TerminalSession>>();
const MAX_PENDING_SESSION_UPDATES = 128;

function cachePendingSessionUpdate(id: string, patch: Partial<TerminalSession>) {
  pendingSessionUpdates.set(id, { ...pendingSessionUpdates.get(id), ...patch });
  while (pendingSessionUpdates.size > MAX_PENDING_SESSION_UPDATES) {
    const oldest = pendingSessionUpdates.keys().next().value;
    if (oldest === undefined) break;
    pendingSessionUpdates.delete(oldest);
  }
}

type Panel = "files" | "commands" | "transfers" | "system";

interface AppState {
  connections: ConnectionProfile[];
  sessions: TerminalSession[];
  activeSessionId: string | null;
  activePanel: Panel;
  monitor: MonitorSnapshot | null;
  transfers: TransferTask[];
  settings: AppSettings;
  connectionEditorOpen: boolean;
  editingConnection: ConnectionProfile | null;
  settingsOpen: boolean;
  helpOpen: boolean;
  loading: boolean;
  error: string | null;
  bootstrap: () => Promise<void>;
  refreshConnections: () => Promise<void>;
  setActiveSession: (id: string) => void;
  addSession: (session: TerminalSession) => void;
  updateSession: (id: string, patch: Partial<TerminalSession>) => void;
  removeSession: (id: string) => void;
  setPanel: (panel: Panel) => void;
  setMonitor: (snapshot: MonitorSnapshot | null) => void;
  setTransfers: (tasks: TransferTask[]) => void;
  upsertTransfer: (task: TransferTask) => void;
  openConnectionEditor: (connection?: ConnectionProfile | null) => void;
  closeConnectionEditor: () => void;
  setSettingsOpen: (open: boolean) => void;
  setHelpOpen: (open: boolean) => void;
  saveSettings: (settings: AppSettings) => Promise<void>;
  setError: (error: string | null) => void;
}

export const useAppStore = create<AppState>((set, get) => ({
  connections: [], sessions: [], activeSessionId: null, activePanel: "files", monitor: null,
  transfers: [], settings: defaultSettings, connectionEditorOpen: false, editingConnection: null,
  settingsOpen: false, helpOpen: false, loading: true, error: null,
  bootstrap: async () => {
    set({ loading: true });
    try {
      const [connections, settings, transfers] = await Promise.all([api.listConnections(), api.getSettings(), api.listTransfers()]);
      set({ connections, settings, transfers, loading: false });
    } catch (error) { set({ error: String(error), loading: false }); }
  },
  refreshConnections: async () => set({ connections: await api.listConnections() }),
  setActiveSession: (activeSessionId) => set({ activeSessionId }),
  addSession: (session) => set((state) => {
    const pending = pendingSessionUpdates.get(session.id);
    pendingSessionUpdates.delete(session.id);
    return { sessions: [...state.sessions, { ...session, ...pending }], activeSessionId: session.id };
  }),
  updateSession: (id, patch) => set((state) => {
    if (!state.sessions.some((session) => session.id === id)) {
      cachePendingSessionUpdate(id, patch);
      return state;
    }
    return { sessions: state.sessions.map((session) => session.id === id ? { ...session, ...patch } : session) };
  }),
  removeSession: (id) => set((state) => {
    const sessions = state.sessions.filter((session) => session.id !== id);
    return { sessions, activeSessionId: state.activeSessionId === id ? sessions.at(-1)?.id ?? null : state.activeSessionId };
  }),
  setPanel: (activePanel) => set({ activePanel }),
  setMonitor: (monitor) => set({ monitor }),
  setTransfers: (transfers) => set({ transfers }),
  upsertTransfer: (task) => set((state) => ({ transfers: [...state.transfers.filter((item) => item.id !== task.id), task] })),
  openConnectionEditor: (connection = null) => set({ editingConnection: connection, connectionEditorOpen: true }),
  closeConnectionEditor: () => set({ connectionEditorOpen: false, editingConnection: null }),
  setSettingsOpen: (settingsOpen) => set({ settingsOpen }),
  setHelpOpen: (helpOpen) => set({ helpOpen }),
  saveSettings: async (settings) => { const previous=get().settings;set({settings});try{await api.saveSettings(settings);}catch(error){set({settings:previous});throw error;} },
  setError: (error) => set({ error })
}));
