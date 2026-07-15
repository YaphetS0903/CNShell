import type { TerminalSession } from "../types";
import type { TerminalLayout } from "../features/terminal/terminal-layout";

export interface WorkspaceSnapshot {
  sessions: { id: string; connectionId: string; cwd: string | null }[];
  activeSessionId: string | null;
  terminalLayout: TerminalLayout | null;
  bottomOpen: boolean;
  bottomHeight: number;
  connectionsOpen: boolean;
  monitorOpen: boolean;
  connectionWidth: number;
  monitorWidth: number;
}

export function createWorkspaceSnapshot(
  sessions: TerminalSession[],
  activeSessionId: string | null,
  cwdBySession: Map<string, string>,
  layout: Omit<WorkspaceSnapshot, "sessions" | "activeSessionId">,
): WorkspaceSnapshot {
  return {
    ...layout,
    sessions: sessions
      .filter((session) => session.sessionType !== "rdp")
      .map((session) => ({
        id: session.id,
        connectionId: session.connectionId,
        cwd: cwdBySession.get(session.id) ?? null,
      })),
    activeSessionId,
  };
}

export async function saveBeforeWindowClose(
  save: () => Promise<void>,
  destroy: () => Promise<void>,
  onSaveError: (error: unknown) => void,
): Promise<void> {
  try {
    await save();
  } catch (error) {
    onSaveError(error);
  }
  await destroy();
}

export async function saveWorkspaceIfChanged(
  snapshot: WorkspaceSnapshot,
  previous: string | null,
  save: (value: WorkspaceSnapshot) => Promise<void>,
): Promise<string> {
  const serialized = JSON.stringify(snapshot);
  if (serialized === previous) return serialized;
  await save(snapshot);
  return serialized;
}
