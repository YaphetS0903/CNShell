import { Client } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { buildSshConnectConfig } from "./sshConnectionConfig.js";
import type { CollectMetricsRequest, CollectMetricsResult } from "../src/shared/ipc.js";
import type { ServerMetric } from "../src/domain/models.js";

function execCommand(client: Client, command: string): Promise<string> {
  return new Promise((resolve, reject) => {
    client.exec(command, (error, stream) => {
      if (error) {
        reject(error);
        return;
      }

      let stdout = "";
      let stderr = "";

      stream.on("data", (data: Buffer) => {
        stdout += data.toString("utf8");
      });

      stream.stderr.on("data", (data: Buffer) => {
        stderr += data.toString("utf8");
      });

      stream.on("close", (code: number) => {
        if (code !== 0 && stderr) {
          reject(new Error(stderr.trim()));
          return;
        }

        resolve(stdout);
      });
    });
  });
}

function parseMetrics(output: string): ServerMetric[] {
  const values = Object.fromEntries(
    output
      .split("\n")
      .map((line) => line.trim().split("="))
      .filter((parts) => parts.length === 2)
      .map(([key, value]) => [key, Number(value)])
  );

  return [
    { label: "CPU", value: Math.round(values.CPU || 0), unit: "%", trend: "flat" },
    { label: "Memory", value: Math.round(values.MEM || 0), unit: "%", trend: "flat" },
    { label: "Disk", value: Math.round(values.DISK || 0), unit: "%", trend: "flat" },
    { label: "Ping", value: Math.round(values.PING || 0), unit: "ms", trend: "flat" }
  ];
}

const METRICS_COMMAND = [
  "read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat",
  "total=$((user+nice+system+idle+iowait+irq+softirq+steal))",
  "busy=$((total-idle-iowait))",
  "cpu_pct=0",
  "if [ \"$total\" -gt 0 ]; then cpu_pct=$((busy*100/total)); fi",
  "mem_pct=$(free | awk '/Mem:/ { if ($2 > 0) printf(\"%.0f\", ($3/$2)*100); else print 0 }')",
  "disk_pct=$(df -P / | awk 'NR==2 { gsub(/%/, \"\", $5); print $5 }')",
  "ping_ms=$( (ping -c 1 -W 1 127.0.0.1 2>/dev/null || true) | awk -F'time=' '/time=/ { split($2,a,\" \"); printf(\"%.0f\", a[1]) } END { if (NR==0) print 0 }')",
  "echo CPU=${cpu_pct:-0}",
  "echo MEM=${mem_pct:-0}",
  "echo DISK=${disk_pct:-0}",
  "echo PING=${ping_ms:-0}"
].join("; ");

export class MetricsService {
  constructor(
    private readonly knownHostsStore: KnownHostsStore | null,
    private readonly credentialStore: CredentialStore | null
  ) {}

  collect(request: CollectMetricsRequest): Promise<CollectMetricsResult> {
    const client = new Client();

    return new Promise((resolve, reject) => {
      client
        .on("ready", () => {
          execCommand(client, METRICS_COMMAND)
            .then((output) => {
              client.end();
              resolve({ metrics: parseMetrics(output) });
            })
            .catch((error: Error) => {
              client.end();
              reject(error);
            });
        })
        .on("error", reject)
        .connect(
          buildSshConnectConfig({
            ssh: request.ssh,
            credentialStore: this.credentialStore,
            knownHostsStore: this.knownHostsStore,
            onHostKeyVerification: (event) => {
              reject(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
            }
          })
        );
    });
  }
}
