import type { ConnectionProfile, QuickCommand, RemoteFileEntry, ServerMetric, SessionTab } from "./models";

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
    lastConnectedAt: "2026-06-18T09:12:00+08:00"
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
