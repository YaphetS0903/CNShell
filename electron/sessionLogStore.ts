import fs from "node:fs";
import path from "node:path";

function sanitizeSessionId(sessionId: string) {
  return sessionId.replace(/[^a-zA-Z0-9._-]/g, "_");
}

export class SessionLogStore {
  private readonly logDir: string;

  constructor(userDataPath: string) {
    this.logDir = path.join(userDataPath, "logs");
  }

  append(sessionId: string, data: string) {
    fs.mkdirSync(this.logDir, { recursive: true });
    const filePath = path.join(this.logDir, `${sanitizeSessionId(sessionId)}.log`);
    fs.appendFileSync(filePath, data);
  }
}
