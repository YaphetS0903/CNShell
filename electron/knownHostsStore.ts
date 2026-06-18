import crypto from "node:crypto";
import fs from "node:fs";
import path from "node:path";

export type HostKeyTrustStatus = "trusted" | "unknown" | "changed";

export interface KnownHostEntry {
  host: string;
  port: number;
  fingerprint: string;
  keyBase64: string;
  trustedAt: string;
  lastSeenAt: string;
}

export interface HostKeyVerificationResult {
  status: HostKeyTrustStatus;
  host: string;
  port: number;
  fingerprint: string;
  expectedFingerprint?: string;
}

export function fingerprintHostKey(key: Buffer) {
  const digest = crypto.createHash("sha256").update(key).digest("base64");
  return `SHA256:${digest}`;
}

export class KnownHostsStore {
  private readonly filePath: string;

  constructor(userDataPath: string) {
    this.filePath = path.join(userDataPath, "known_hosts.json");
  }

  verifyHostKey(host: string, port: number, key: Buffer): HostKeyVerificationResult {
    const fingerprint = fingerprintHostKey(key);
    const entries = this.read();
    const knownHost = entries.find((entry) => entry.host === host && entry.port === port);

    if (!knownHost) {
      return {
        status: "unknown",
        host,
        port,
        fingerprint
      };
    }

    if (knownHost.keyBase64 !== key.toString("base64")) {
      return {
        status: "changed",
        host,
        port,
        fingerprint,
        expectedFingerprint: knownHost.fingerprint
      };
    }

    knownHost.lastSeenAt = new Date().toISOString();
    this.write(entries);

    return {
      status: "trusted",
      host,
      port,
      fingerprint
    };
  }

  trustHost(host: string, port: number, fingerprint: string, keyBase64: string) {
    const entries = this.read().filter((entry) => entry.host !== host || entry.port !== port);
    const now = new Date().toISOString();

    entries.push({
      host,
      port,
      fingerprint,
      keyBase64,
      trustedAt: now,
      lastSeenAt: now
    });

    this.write(entries);
  }

  private find(host: string, port: number) {
    return this.read().find((entry) => entry.host === host && entry.port === port);
  }

  private read(): KnownHostEntry[] {
    if (!fs.existsSync(this.filePath)) {
      return [];
    }

    try {
      const parsed = JSON.parse(fs.readFileSync(this.filePath, "utf8")) as KnownHostEntry[];
      return Array.isArray(parsed) ? parsed : [];
    } catch {
      return [];
    }
  }

  private write(entries: KnownHostEntry[]) {
    fs.mkdirSync(path.dirname(this.filePath), { recursive: true });
    fs.writeFileSync(this.filePath, JSON.stringify(entries, null, 2));
  }
}
