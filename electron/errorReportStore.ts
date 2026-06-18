import fs from "node:fs";
import path from "node:path";
import type { RendererErrorReportRequest } from "../src/shared/ipc.js";

export interface ErrorReportEntry {
  id: string;
  at: string;
  source: "main" | "renderer";
  message: string;
  stack?: string;
  componentStack?: string;
}

function truncate(value: string | undefined, maxLength: number) {
  if (!value) {
    return undefined;
  }

  return value.length > maxLength ? `${value.slice(0, maxLength)}...` : value;
}

export class ErrorReportStore {
  private readonly filePath: string;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "errors", "errors.jsonl");
  }

  record(source: "main" | "renderer", error: Error | RendererErrorReportRequest) {
    const entry: ErrorReportEntry = {
      id: `error-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      at: new Date().toISOString(),
      source,
      message: truncate(error.message, 1000) ?? "Unknown error",
      stack: truncate(error.stack, 8000),
      componentStack: "componentStack" in error ? truncate(error.componentStack, 8000) : undefined
    };

    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    fs.appendFileSync(this.filePath, `${JSON.stringify(entry)}\n`);
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
