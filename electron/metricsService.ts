import { Client } from "ssh2";
import type { CredentialStore } from "./credentialStore.js";
import type { KnownHostsStore } from "./knownHostsStore.js";
import { connectSshClient } from "./sshConnectionConfig.js";
import type {
  CollectMetricsRequest,
  CollectMetricsResult,
  KillProcessRequest,
  KillProcessResult,
  ListProcessesRequest,
  ListProcessesResult,
  SshSessionConfig
} from "../src/shared/ipc.js";
import type { FileSystemUsage, NetworkSample, RemoteProcess, ServerMetric, SystemInfo } from "../src/domain/models.js";

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
  const values = parseKeyValueOutput(output);

  return [
    { label: "CPU", value: Math.round(values.CPU || 0), unit: "%", trend: "flat" },
    { label: "Memory", value: Math.round(values.MEM || 0), unit: "%", trend: "flat" },
    { label: "Disk", value: Math.round(values.DISK || 0), unit: "%", trend: "flat" },
    { label: "Ping", value: Math.round(values.PING || 0), unit: "ms", trend: "flat" }
  ];
}

function parseKeyValueOutput(output: string) {
  return Object.fromEntries(
    output
      .split("\n")
      .map((line) => line.trim().split("="))
      .filter((parts) => parts.length === 2)
      .map(([key, value]) => [key, Number(value)])
  );
}

function parseTextFields(output: string) {
  return Object.fromEntries(
    output
      .split("\n")
      .map((line) => {
        const separatorIndex = line.indexOf("=");
        return separatorIndex === -1 ? null : [line.slice(0, separatorIndex), line.slice(separatorIndex + 1)];
      })
      .filter((parts): parts is string[] => Boolean(parts))
  );
}

function parseFilesystems(raw: string | undefined): FileSystemUsage[] {
  if (!raw) {
    return [];
  }

  return raw
    .split("|")
    .map((item) => {
      const [path = "", used = "", total = "", percent = "0"] = item.split(",");
      return {
        path,
        used,
        total,
        percent: Number(percent.replace("%", "")) || 0
      };
    })
    .filter((item) => item.path)
    .slice(0, 16);
}

function parseNetworkSamples(raw: string | undefined): NetworkSample[] {
  if (!raw) {
    return [];
  }

  const now = new Date().toLocaleTimeString();
  return raw
    .split("|")
    .map((item, index) => {
      const [inbound = "0", outbound = "0"] = item.split(",");
      return {
        at: index === 0 ? now : "",
        inboundKb: Number(inbound) || 0,
        outboundKb: Number(outbound) || 0
      };
    })
    .filter((item) => item.inboundKb > 0 || item.outboundKb > 0)
    .slice(-24);
}

function parseSystemInfo(output: string): SystemInfo {
  const fields = parseTextFields(output);
  return {
    os: fields.OS || "",
    kernel: fields.KERNEL || "",
    kernelVersion: fields.KERNEL_VERSION || "",
    architecture: fields.ARCH || "",
    hostname: fields.HOSTNAME || "",
    cpuModel: fields.CPU_MODEL || "",
    cpuCores: Number(fields.CPU_CORES) || 0,
    uptimeDays: Number(fields.UPTIME_DAYS) || 0,
    loadAverage: fields.LOAD_AVG || "",
    memoryUsed: fields.MEM_USED || "0",
    memoryTotal: fields.MEM_TOTAL || "0",
    swapUsed: fields.SWAP_USED || "0",
    swapTotal: fields.SWAP_TOTAL || "0",
    networkInterface: fields.NET_IFACE || "",
    filesystems: parseFilesystems(fields.FILESYSTEMS),
    networkSamples: parseNetworkSamples(fields.NETWORK_SAMPLES)
  };
}

function parseProcesses(output: string): RemoteProcess[] {
  return output
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(1)
    .map((line) => {
      const [pid = "0", ppid = "0", cpu = "0", memory = "0", command = "", ...args] = line.split(/\s+/);
      return {
        pid: Number(pid),
        ppid: Number(ppid),
        cpu: Number(cpu),
        memory: Number(memory),
        command,
        args: args.join(" ")
      };
    })
    .filter((process) => process.pid > 0)
    .slice(0, 80);
}

const METRICS_COMMAND = [
  "os_name=$( (grep PRETTY_NAME /etc/os-release 2>/dev/null | cut -d= -f2- | tr -d '\"') || uname -s )",
  "kernel_name=$(uname -s 2>/dev/null || echo '')",
  "kernel_version=$(uname -r 2>/dev/null || echo '')",
  "arch_name=$(uname -m 2>/dev/null || echo '')",
  "host_name=$(hostname 2>/dev/null || echo '')",
  "cpu_model=$(awk -F: '/model name|Hardware/ { gsub(/^ /,\"\",$2); print $2; exit }' /proc/cpuinfo 2>/dev/null)",
  "cpu_cores=$(getconf _NPROCESSORS_ONLN 2>/dev/null || nproc 2>/dev/null || echo 0)",
  "uptime_days=$(awk '{ printf(\"%.0f\", $1/86400) }' /proc/uptime 2>/dev/null)",
  "load_avg=$(cut -d' ' -f1-3 /proc/loadavg 2>/dev/null)",
  "read cpu user nice system idle iowait irq softirq steal guest guest_nice < /proc/stat",
  "total=$((user+nice+system+idle+iowait+irq+softirq+steal))",
  "busy=$((total-idle-iowait))",
  "cpu_pct=0",
  "if [ \"$total\" -gt 0 ]; then cpu_pct=$((busy*100/total)); fi",
  "mem_pct=$(free | awk '/Mem:/ { if ($2 > 0) printf(\"%.0f\", ($3/$2)*100); else print 0 }')",
  "mem_used=$(free -h | awk '/Mem:/ {print $3}')",
  "mem_total=$(free -h | awk '/Mem:/ {print $2}')",
  "swap_used=$(free -h | awk '/Swap:/ {print $3}')",
  "swap_total=$(free -h | awk '/Swap:/ {print $2}')",
  "disk_pct=$(df -P / | awk 'NR==2 { gsub(/%/, \"\", $5); print $5 }')",
  "filesystems=$(df -hP | awk 'NR>1 { gsub(/%/, \"\", $5); printf \"%s,%s,%s,%s|\", $6,$3,$2,$5 }')",
  "net_iface=$(ip route get 1.1.1.1 2>/dev/null | awk '{ for(i=1;i<=NF;i++) if($i==\"dev\") { print $(i+1); exit } }')",
  "if [ -z \"$net_iface\" ]; then net_iface=$(ls /sys/class/net 2>/dev/null | grep -v '^lo$' | head -n1); fi",
  "rx1=$(cat /sys/class/net/${net_iface:-lo}/statistics/rx_bytes 2>/dev/null || echo 0)",
  "tx1=$(cat /sys/class/net/${net_iface:-lo}/statistics/tx_bytes 2>/dev/null || echo 0)",
  "sleep 1",
  "rx2=$(cat /sys/class/net/${net_iface:-lo}/statistics/rx_bytes 2>/dev/null || echo 0)",
  "tx2=$(cat /sys/class/net/${net_iface:-lo}/statistics/tx_bytes 2>/dev/null || echo 0)",
  "rx_kb=$(( (rx2-rx1)/1024 ))",
  "tx_kb=$(( (tx2-tx1)/1024 ))",
  "ping_ms=$( (ping -c 1 -W 1 127.0.0.1 2>/dev/null || true) | awk -F'time=' '/time=/ { split($2,a,\" \"); printf(\"%.0f\", a[1]) } END { if (NR==0) print 0 }')",
  "echo OS=${os_name:-}",
  "echo KERNEL=${kernel_name:-}",
  "echo KERNEL_VERSION=${kernel_version:-}",
  "echo ARCH=${arch_name:-}",
  "echo HOSTNAME=${host_name:-}",
  "echo CPU_MODEL=${cpu_model:-}",
  "echo CPU_CORES=${cpu_cores:-0}",
  "echo UPTIME_DAYS=${uptime_days:-0}",
  "echo LOAD_AVG=${load_avg:-}",
  "echo MEM_USED=${mem_used:-0}",
  "echo MEM_TOTAL=${mem_total:-0}",
  "echo SWAP_USED=${swap_used:-0}",
  "echo SWAP_TOTAL=${swap_total:-0}",
  "echo NET_IFACE=${net_iface:-lo}",
  "echo FILESYSTEMS=${filesystems:-}",
  "echo NETWORK_SAMPLES=${rx_kb:-0},${tx_kb:-0}",
  "echo CPU=${cpu_pct:-0}",
  "echo MEM=${mem_pct:-0}",
  "echo DISK=${disk_pct:-0}",
  "echo PING=${ping_ms:-0}"
].join("; ");

const PROCESS_LIST_COMMAND = "ps -eo pid,ppid,pcpu,pmem,comm,args --sort=-pcpu | head -n 81";

export class MetricsService {
  constructor(
    private readonly knownHostsStore: KnownHostsStore | null,
    private readonly credentialStore: CredentialStore | null
  ) {}

  collect(request: CollectMetricsRequest): Promise<CollectMetricsResult> {
    return this.withSshClient(request.ssh, (client) =>
      execCommand(client, METRICS_COMMAND).then((output) => ({
        metrics: parseMetrics(output),
        systemInfo: parseSystemInfo(output)
      }))
    );
  }

  listProcesses(request: ListProcessesRequest): Promise<ListProcessesResult> {
    return this.withSshClient(request.ssh, (client) =>
      execCommand(client, PROCESS_LIST_COMMAND).then((output) => ({ processes: parseProcesses(output) }))
    );
  }

  killProcess(request: KillProcessRequest): Promise<KillProcessResult> {
    const signal = request.signal ?? "TERM";
    return this.withSshClient(request.ssh, (client) =>
      execCommand(client, `kill -s ${signal} ${request.pid}`).then(() => ({ ok: true }))
    );
  }

  private withSshClient<T>(ssh: SshSessionConfig, action: (client: Client) => Promise<T>): Promise<T> {
    const client = new Client();

    return new Promise<T>((resolve, reject) => {
      let gateways: Client[] = [];
      const closeClient = () => {
        client.end();
        for (const gateway of gateways) {
          gateway.end();
        }
      };

      connectSshClient(client, {
        ssh,
        credentialStore: this.credentialStore,
        knownHostsStore: this.knownHostsStore,
        onHostKeyVerification: (event) => {
          reject(new Error(`Host key verification required for ${event.host}:${event.port} (${event.fingerprint}).`));
        }
      })
        .then((connected) => {
          gateways = connected.gateways;
          action(client)
            .then((result) => {
              closeClient();
              resolve(result);
            })
            .catch((error: Error) => {
              closeClient();
              reject(error);
            });
        })
        .catch(reject);
    });
  }
}
