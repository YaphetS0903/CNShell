import fs from "node:fs";
import path from "node:path";

export interface AuditLogEntry {
  id: string;
  at: string;
  action: string;
  status: "ok" | "error";
  target?: string;
  details?: unknown;
  error?: string;
}

const SECRET_KEYS = new Set([
  "password",
  "privateKey",
  "passphrase",
  "masterPassword",
  "secret",
  "encryptedSecretBase64",
  "keyBase64",
  "encryptedSnapshotBase64",
  "content",
  "data"
]);

function sanitizeKey(key: string) {
  return key.replace(/[^a-zA-Z0-9._-]/g, "_");
}

export function redactSecrets(value: unknown): unknown {
  if (Array.isArray(value)) {
    return value.slice(0, 20).map((item) => redactSecrets(item));
  }

  if (!value || typeof value !== "object") {
    return typeof value === "string" && value.length > 240 ? `${value.slice(0, 240)}...` : value;
  }

  const output: Record<string, unknown> = {};
  for (const [key, nestedValue] of Object.entries(value)) {
    output[key] = SECRET_KEYS.has(key) ? "[REDACTED]" : redactSecrets(nestedValue);
  }

  return output;
}

export class AuditLogStore {
  private readonly filePath: string;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "audit", "audit.jsonl");
  }

  record(entry: Omit<AuditLogEntry, "id" | "at">) {
    const payload: AuditLogEntry = {
      id: `audit-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      at: new Date().toISOString(),
      ...entry,
      details: redactSecrets(entry.details)
    };

    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    fs.appendFileSync(this.filePath, `${JSON.stringify(payload)}\n`);
  }

  read(query = "", limit = 300) {
    if (!fs.existsSync(this.filePath)) {
      return [];
    }

    const normalizedQuery = query.trim().toLowerCase();
    return fs
      .readFileSync(this.filePath, "utf8")
      .split(/\r?\n/)
      .filter(Boolean)
      .filter((line) => (normalizedQuery ? line.toLowerCase().includes(normalizedQuery) : true))
      .slice(-Math.max(1, Math.min(limit, 1000)));
  }
}

export function summarizeTerminalWrite(id: string, data: string) {
  return {
    id,
    bytes: Buffer.byteLength(data, "utf8"),
    lines: data.split(/\r?\n/).length
  };
}

export function summarizeWorkspace(snapshot: { connections?: unknown[]; sessions?: unknown[] }) {
  return {
    connections: snapshot.connections?.length ?? 0,
    sessions: snapshot.sessions?.length ?? 0
  };
}

export function sanitizeAuditTarget(target: string) {
  return sanitizeKey(target).slice(0, 160);
}
