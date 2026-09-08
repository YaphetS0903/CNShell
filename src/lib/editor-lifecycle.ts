export interface RemoteEditorTarget {
  sessionId: string;
  connectionId: string;
  path: string;
}

interface EditorGuard {
  canLeave: () => boolean;
  hasPendingWork: () => boolean;
}

let activeGuard: EditorGuard | undefined;

export function registerEditorGuard(guard: EditorGuard) {
  activeGuard = guard;
  return () => {
    if (activeGuard === guard) activeGuard = undefined;
  };
}

export const canLeaveEditor = () => activeGuard?.canLeave() ?? true;
export const editorHasPendingWork = () =>
  activeGuard?.hasPendingWork() ?? false;
