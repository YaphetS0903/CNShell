import type {
  ConnectionProfile,
  KeyMappingProfile,
  QuickCommand,
  RemoteFileEntry,
  RemoteProcess,
  ScriptRecording,
  ServerMetric,
  SystemInfo,
  SessionTab
} from "./models";

export const connectionProfiles: ConnectionProfile[] = [
  {
    id: "prod-web-01",
    name: "prod-web-01",
    group: "Production",
    protocol: "ssh",
    host: "10.24.18.11",
    port: 22,
    username: "deploy",
    authMethod: "privateKey",
    color: "#2f9e44",
    tags: ["nginx", "api"],
    lastConnectedAt: "2026-06-18T09:12:00+08:00",
    gateways: [
      {
        id: "gw-prod-bastion",
        name: "prod-bastion",
        host: "10.24.0.10",
        port: 22,
        username: "deploy"
      }
    ]
  },
  {
    id: "stage-db-01",
    name: "stage-db-01",
    group: "Staging",
    protocol: "ssh",
    host: "10.31.9.45",
    port: 22,
    username: "postgres",
    authMethod: "agent",
    color: "#d9480f",
    tags: ["postgres", "backup"],
    lastConnectedAt: "2026-06-17T21:38:00+08:00"
  },
  {
    id: "rdp-admin-01",
    name: "rdp-admin-01",
    group: "Windows",
    protocol: "rdp",
    host: "10.24.30.20",
    port: 3389,
    username: "administrator",
    authMethod: "password",
    color: "#7048e8",
    tags: ["windows", "rdp"]
  },
  {
    id: "local-powershell",
    name: "Local PowerShell",
    group: "Local",
    protocol: "local",
    host: "localhost",
    port: 0,
    username: "local",
    authMethod: "agent",
    color: "#1971c2",
    tags: ["windows"]
  }
];

export const sessionTabs: SessionTab[] = [
  {
    id: "tab-prod-web-01",
    connectionId: "prod-web-01",
    title: "prod-web-01",
    cwd: "/var/www/cnshell",
    status: "connected",
    startedAt: "2026-06-18T09:18:00+08:00"
  },
  {
    id: "tab-stage-db-01",
    connectionId: "stage-db-01",
    title: "stage-db-01",
    cwd: "/data/postgresql",
    status: "connecting",
    startedAt: "2026-06-18T09:21:00+08:00"
  }
];

export const quickCommands: QuickCommand[] = [
  { id: "qc-systemctl", title: "Restart service", command: "sudo systemctl restart ${service}", scope: "global" },
  { id: "qc-disk", title: "Disk usage", command: "df -h", scope: "global" },
  { id: "qc-nginx", title: "Nginx errors", command: "sudo tail -n 200 /var/log/nginx/error.log", scope: "group" }
];

export const keyMappingProfiles: KeyMappingProfile[] = [
  {
    id: "keys-default",
    name: "Default Terminal",
    enabled: true,
    rules: [
      { id: "km-clear", key: "Ctrl+L", send: "\u000c", description: "Clear terminal", enabled: true },
      { id: "km-interrupt", key: "Ctrl+C", send: "\u0003", description: "Interrupt process", enabled: true },
      { id: "km-eof", key: "Ctrl+D", send: "\u0004", description: "Send EOF", enabled: true }
    ]
  }
];

export const scriptRecordings: ScriptRecording[] = [];

export const remoteFiles: RemoteFileEntry[] = [
  {
    id: "file-1",
    name: "releases",
    path: "/var/www/cnshell/releases",
    type: "directory",
    size: 0,
    modifiedAt: "2026-06-18 08:49",
    mode: "drwxr-xr-x"
  },
  {
    id: "file-2",
    name: ".env",
    path: "/var/www/cnshell/.env",
    type: "file",
    size: 2688,
    modifiedAt: "2026-06-18 08:51",
    mode: "-rw-------"
  },
  {
    id: "file-3",
    name: "current",
    path: "/var/www/cnshell/current",
    type: "symlink",
    size: 26,
    modifiedAt: "2026-06-18 08:53",
    mode: "lrwxrwxrwx"
  }
];

export const serverMetrics: ServerMetric[] = [
  { label: "CPU", value: 37, unit: "%", trend: "flat" },
  { label: "Memory", value: 68, unit: "%", trend: "up" },
  { label: "Disk", value: 74, unit: "%", trend: "up" },
  { label: "Ping", value: 18, unit: "ms", trend: "down" }
];

export const systemInfo: SystemInfo = {
  os: "Ubuntu 22.04 LTS",
  kernel: "Linux",
  kernelVersion: "5.15.0",
  architecture: "x86_64",
  hostname: "prod-web-01",
  cpuModel: "Intel Xeon",
  cpuCores: 4,
  uptimeDays: 97,
  loadAverage: "0.75, 0.92, 0.91",
  memoryUsed: "6.3G",
  memoryTotal: "23.4G",
  swapUsed: "0",
  swapTotal: "0",
  networkInterface: "enp0s6",
  filesystems: [
    { path: "/dev", used: "11.7G", total: "11.7G", percent: 100 },
    { path: "/run", used: "2.3G", total: "2.3G", percent: 100 },
    { path: "/", used: "59.1G", total: "96.7G", percent: 61 },
    { path: "/dev/shm", used: "9.4G", total: "11.7G", percent: 80 }
  ],
  networkSamples: [
    { at: "23:44:00", inboundKb: 8, outboundKb: 2 },
    { at: "23:44:05", inboundKb: 16, outboundKb: 4 },
    { at: "23:44:10", inboundKb: 31, outboundKb: 7 },
    { at: "23:44:15", inboundKb: 22, outboundKb: 3 }
  ]
};

export const remoteProcesses: RemoteProcess[] = [];
