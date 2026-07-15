import type { AutomationPlan, AutomationStep } from "../types";

export interface RecordableAutomationAction {
  kind: "command";
  connectionId: string;
  command: string;
  recordedAt: string;
  source: "commandPanel";
}

type Listener = (action: RecordableAutomationAction) => void;
const listeners = new Set<Listener>();

const sensitivePatterns = [
  /(?:password|passwd|token|secret|api[_-]?key|private[_-]?key|authorization|bearer)\s*=/i,
  /(?:--password|--token|--secret)\b/i,
  /\bsshpass\b/i,
  /\bread\s+-s\b/i,
];

export function isSensitiveCommand(command: string): boolean {
  return sensitivePatterns.some((pattern) => pattern.test(command));
}

export function publishRecordableCommand(connectionId: string, command: string): boolean {
  const normalized = command.trim();
  if (!connectionId || !normalized || normalized.length > 16 * 1024 || isSensitiveCommand(normalized)) return false;
  const action: RecordableAutomationAction = {
    kind: "command",
    connectionId,
    command: normalized,
    recordedAt: new Date().toISOString(),
    source: "commandPanel",
  };
  listeners.forEach((listener) => listener(action));
  return true;
}

export function listenRecordableAction(listener: Listener): () => void {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

export function compileRecordedActions(
  name: string,
  connectionId: string,
  actions: RecordableAutomationAction[],
): AutomationPlan | null {
  const steps: AutomationStep[] = actions
    .filter((action) => action.connectionId === connectionId && action.kind === "command")
    .map((action) => ({
      id: crypto.randomUUID(),
      kind: "command",
      command: action.command,
      pattern: null,
      timeoutSeconds: 30,
      action: null,
      direction: null,
      localPath: null,
      remotePath: null,
    }));
  if (!name.trim() || !connectionId || !steps.length || steps.length > 50) return null;
  return { id: crypto.randomUUID(), name: name.trim(), connectionId, steps };
}

