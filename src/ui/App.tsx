import { useCallback, useEffect, useMemo, useState } from "react";
import {
  Activity,
  ChevronRight,
  Circle,
  Code2,
  Command,
  FileText,
  Folder,
  HardDrive,
  KeyRound,
  LayoutDashboard,
  Monitor,
  MoreHorizontal,
  Network,
  Plus,
  RefreshCw,
  Search,
  Server,
  Settings,
  ShieldCheck,
  SplitSquareHorizontal,
  TerminalSquare,
  UploadCloud,
  Zap
} from "lucide-react";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { Terminal } from "@xterm/xterm";
import { createInitialAppSnapshot, groupConnections, hydrateAppSnapshot } from "../domain/appState";
import { createLocalWorkspaceStorage } from "../domain/storage";
import type {
  ConnectionProfile,
  JumpHostConfig,
  KeyMappingProfile,
  KeyMappingRule,
  ScriptRecording,
  ScriptRecordingEvent,
  SessionStatus,
  SessionTab,
  TransferJob
} from "../domain/models";
import type { CredentialStatus, HostKeyVerificationEvent, RelayInfo, SshSessionConfig, TunnelInfo, TunnelMode } from "../shared/ipc";
import { terminalTheme } from "./terminalTheme";

const workspaceStorage = createLocalWorkspaceStorage();

interface TriggerEvent {
  id: string;
  sessionId: string;
  severity: "error" | "warning";
  message: string;
  createdAt: string;
}

interface TunnelDraft {
  mode: TunnelMode;
  bindHost: string;
  bindPort: string;
  targetHost: string;
  targetPort: string;
}

interface SafePasteReview {
  text: string;
  reasons: string[];
}

interface MetricHistoryPoint {
  at: string;
  cpu: number;
  memory: number;
  disk: number;
  network: number;
  processes: number;
}

type ZmodemMode = "idle" | "upload" | "download" | "detected";

const tunnelModes: Array<{ value: TunnelMode; label: string }> = [
  { value: "local", label: "Local" },
  { value: "remote", label: "Remote" },
  { value: "dynamic", label: "Dynamic" }
];

const modifierKeys = new Set(["Alt", "Control", "Meta", "Shift"]);

function parsePort(value: string) {
  const port = Number(value);
  return Number.isInteger(port) && port > 0 && port <= 65535 ? port : null;
}

function formatKeyEvent(event: KeyboardEvent) {
  const parts: string[] = [];

  if (event.ctrlKey) {
    parts.push("Ctrl");
  }

  if (event.altKey) {
    parts.push("Alt");
  }

  if (event.shiftKey) {
    parts.push("Shift");
  }

  if (event.metaKey) {
    parts.push("Meta");
  }

  if (!modifierKeys.has(event.key)) {
    const key = event.key.length === 1 ? event.key.toUpperCase() : event.key;
    parts.push(key);
  }

  return parts.join("+");
}

function normalizeSendValue(value: string) {
  return value.replaceAll("\\r", "\r").replaceAll("\\n", "\n").replaceAll("\\t", "\t").replaceAll("\\e", "\x1b");
}

function normalizeRemotePath(path: string) {
  const parts: string[] = [];
  for (const part of path.split("/")) {
    if (!part || part === ".") {
      continue;
    }

    if (part === "..") {
      parts.pop();
      continue;
    }

    parts.push(part);
  }

  return `/${parts.join("/")}`;
}

function inferDirectoryFromCommand(command: string, currentPath: string) {
  const trimmed = command.trim();
  const match = /^cd(?:\s+(.+))?$/.exec(trimmed);
  if (!match) {
    return null;
  }

  const rawPath = (match[1] ?? "~").trim().replace(/^['"]|['"]$/g, "");
  if (!rawPath || rawPath === "~") {
    return "/";
  }

  if (rawPath === "-") {
    return null;
  }

  if (rawPath.startsWith("/")) {
    return normalizeRemotePath(rawPath);
  }

  return normalizeRemotePath(`${currentPath.replace(/\/$/, "")}/${rawPath}`);
}

function getActiveKeyRules(profiles: KeyMappingProfile[]) {
  return profiles.flatMap((profile) => (profile.enabled ? profile.rules.filter((rule) => rule.enabled) : []));
}

function inspectPastedText(text: string) {
  const reasons: string[] = [];
  const trimmed = text.trim();
  const lines = trimmed.split(/\r?\n/).filter(Boolean);

  if (lines.length > 1) {
    reasons.push(`${lines.length} lines`);
  }

  if (/[;&|`$()]/.test(trimmed)) {
    reasons.push("shell chaining or expansion");
  }

  if (/\b(rm\s+-[^\n]*[rf]|mkfs|dd\s+if=|chmod\s+-R\s+777|chown\s+-R|shutdown|reboot|:(){:|sudo\s+rm)\b/i.test(trimmed)) {
    reasons.push("high-risk command");
  }

  return reasons;
}

function shouldReviewPaste(text: string) {
  return inspectPastedText(text).length > 0;
}

function metricValue(metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"], label: string) {
  return metrics.find((metric) => metric.label === label)?.value ?? 0;
}

function describeTunnel(tunnel: TunnelInfo) {
  const bind = `${tunnel.bindHost}:${tunnel.bindPort}`;

  if (tunnel.mode === "dynamic") {
    return `${bind} SOCKS5`;
  }

  return `${bind} -> ${tunnel.targetHost ?? "?"}:${tunnel.targetPort ?? "?"}`;
}

function createSshConfig(
  connection: ConnectionProfile,
  draft: { password: string; privateKey: string; passphrase: string },
  useSavedCredential: boolean
): SshSessionConfig {
  return {
    connectionId: connection.id,
    host: connection.host,
    port: connection.port,
    username: connection.username,
    password: draft.password || undefined,
    privateKey: draft.privateKey || undefined,
    passphrase: draft.passphrase || undefined,
    useSavedCredential,
    gateways: connection.gateways
  };
}

function applyHighlightRules(data: string) {
  return data
    .split(/(\r?\n)/)
    .map((part) => {
      if (/(\r?\n)/.test(part)) {
        return part;
      }

      if (/\b(error|failed|failure|fatal|denied)\b/i.test(part)) {
        return `\x1b[31m${part}\x1b[0m`;
      }

      if (/\b(warn|warning|retry|slow)\b/i.test(part)) {
        return `\x1b[33m${part}\x1b[0m`;
      }

      if (/\b(success|succeeded|ok|ready|done)\b/i.test(part)) {
        return `\x1b[32m${part}\x1b[0m`;
      }

      return part;
    })
    .join("");
}

function detectTriggerEvents(sessionId: string, data: string): TriggerEvent[] {
  return data
    .split(/\r?\n/)
    .filter((line) => /\b(error|failed|failure|fatal|denied|warning)\b/i.test(line))
    .slice(-3)
    .map((line) => ({
      id: `${sessionId}-${Date.now()}-${Math.random().toString(36).slice(2)}`,
      sessionId,
      severity: /\b(warn|warning)\b/i.test(line) ? "warning" : "error",
      message: line.trim().slice(0, 220),
      createdAt: new Date().toLocaleTimeString()
    }));
}

function detectZmodemMode(data: string): ZmodemMode {
  if (/rz\s+(waiting|ready)|\*\*B0|ZRQINIT/i.test(data) || data.includes("\x18B0")) {
    return "upload";
  }

  if (/sz\s+(sending|ready)|ZFILE|\*\*B1/i.test(data) || data.includes("\x18B1")) {
    return "download";
  }

  if (/zmodem/i.test(data)) {
    return "detected";
  }

  return "idle";
}

export function App() {
  const [snapshot, setSnapshot] = useState(() => createInitialAppSnapshot());
  const [isWorkspaceReady, setIsWorkspaceReady] = useState(false);
  const [activeConnectionId, setActiveConnectionId] = useState(snapshot.connections[0].id);
  const [activeTabId, setActiveTabId] = useState(snapshot.sessions[0].id);
  const [appVersion, setAppVersion] = useState("dev");
  const [sshDrafts, setSshDrafts] = useState<Record<string, { password: string; privateKey: string; passphrase: string }>>({});
  const [sessionStartTokens, setSessionStartTokens] = useState<Record<string, number>>({});
  const [hostKeyPrompts, setHostKeyPrompts] = useState<Record<string, HostKeyVerificationEvent>>({});
  const [credentialStatuses, setCredentialStatuses] = useState<Record<string, CredentialStatus>>({});
  const [credentialErrors, setCredentialErrors] = useState<Record<string, string>>({});
  const [rdpStatus, setRdpStatus] = useState<"idle" | "launching" | "error">("idle");
  const [rdpError, setRdpError] = useState("");
  const [remoteFileEntries, setRemoteFileEntries] = useState(snapshot.remoteFiles);
  const [remotePath, setRemotePath] = useState("/var/www/cnshell");
  const [sftpStatus, setSftpStatus] = useState<"idle" | "loading" | "error">("idle");
  const [sftpError, setSftpError] = useState("");
  const [liveMetrics, setLiveMetrics] = useState(snapshot.serverMetrics);
  const [metricsStatus, setMetricsStatus] = useState<"idle" | "loading" | "error">("idle");
  const [metricsError, setMetricsError] = useState("");
  const [metricHistory, setMetricHistory] = useState<MetricHistoryPoint[]>([]);
  const [remoteProcesses, setRemoteProcesses] = useState(snapshot.remoteProcesses);
  const [processStatus, setProcessStatus] = useState<"idle" | "loading" | "error">("idle");
  const [processError, setProcessError] = useState("");
  const [transferLocalPath, setTransferLocalPath] = useState("");
  const [transferRemotePath, setTransferRemotePath] = useState("");
  const [transferJobs, setTransferJobs] = useState<TransferJob[]>([]);
  const [zmodemMode, setZmodemMode] = useState<ZmodemMode>("idle");
  const [zmodemMessage, setZmodemMessage] = useState("No ZMODEM session detected");
  const [editorPath, setEditorPath] = useState("");
  const [editorContent, setEditorContent] = useState("");
  const [editorStatus, setEditorStatus] = useState<"idle" | "loading" | "saving" | "error" | "saved">("idle");
  const [editorError, setEditorError] = useState("");
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [isSyncInputEnabled, setIsSyncInputEnabled] = useState(false);
  const [isHighlightEnabled, setIsHighlightEnabled] = useState(true);
  const [triggerEvents, setTriggerEvents] = useState<TriggerEvent[]>([]);
  const [tunnelDraft, setTunnelDraft] = useState<TunnelDraft>({
    mode: "local",
    bindHost: "127.0.0.1",
    bindPort: "8080",
    targetHost: "127.0.0.1",
    targetPort: "80"
  });
  const [tunnels, setTunnels] = useState<TunnelInfo[]>([]);
  const [relayDraft, setRelayDraft] = useState({
    relayHost: "0.0.0.0",
    relayPort: "18080",
    targetHost: "127.0.0.1",
    targetPort: "8080"
  });
  const [relays, setRelays] = useState<RelayInfo[]>([]);
  const [isRecordingScript, setIsRecordingScript] = useState(false);
  const [recordingStartedAt, setRecordingStartedAt] = useState<number | null>(null);
  const [recordingLastInputAt, setRecordingLastInputAt] = useState<number | null>(null);
  const [recordingEvents, setRecordingEvents] = useState<ScriptRecordingEvent[]>([]);
  const [logQuery, setLogQuery] = useState("");
  const [logLines, setLogLines] = useState<string[]>([]);
  const [logStatus, setLogStatus] = useState<"idle" | "loading" | "error">("idle");
  const [cloudSyncStatus, setCloudSyncStatus] = useState("Ready");
  const [sessionStatuses, setSessionStatuses] = useState<Record<string, SessionStatus>>(() =>
    Object.fromEntries(snapshot.sessions.map((session) => [session.id, session.status]))
  );

  const groupedConnections = useMemo(() => groupConnections(snapshot.connections), [snapshot.connections]);

  const activeConnection = useMemo(
    () => snapshot.connections.find((connection) => connection.id === activeConnectionId) ?? snapshot.connections[0],
    [activeConnectionId, snapshot.connections]
  );

  const activeTab = useMemo(
    () => {
      const tab = snapshot.sessions.find((session) => session.id === activeTabId) ?? snapshot.sessions[0];
      return {
        ...tab,
        status: sessionStatuses[tab.id] ?? tab.status
      };
    },
    [activeTabId, sessionStatuses, snapshot.sessions]
  );

  const sessionTabsWithStatus = useMemo(
    () =>
      snapshot.sessions.map((session) => ({
        ...session,
        status: sessionStatuses[session.id] ?? session.status
      })),
    [sessionStatuses, snapshot.sessions]
  );

  const setSessionStatus = useCallback((sessionId: string, status: SessionStatus) => {
    setSessionStatuses((current) => ({
      ...current,
      [sessionId]: status
    }));
  }, []);

  const activeSshDraft = useMemo(
    () => sshDrafts[activeConnection.id] ?? { password: "", privateKey: "", passphrase: "" },
    [activeConnection.id, sshDrafts]
  );

  const activeCredentialStatus = credentialStatuses[activeConnection.id];

  const updateActiveSshDraft = (field: "password" | "privateKey" | "passphrase", value: string) => {
    setSshDrafts((current) => ({
      ...current,
      [activeConnection.id]: {
        ...(current[activeConnection.id] ?? { password: "", privateKey: "", passphrase: "" }),
        [field]: value
      }
    }));
  };

  const updateActiveGateways = (gateways: JumpHostConfig[]) => {
    setSnapshot((current) => ({
      ...current,
      connections: current.connections.map((connection) =>
        connection.id === activeConnection.id ? { ...connection, gateways } : connection
      )
    }));
  };

  const updateKeyMappingProfiles = (profiles: KeyMappingProfile[]) => {
    setSnapshot((current) => ({
      ...current,
      keyMappingProfiles: profiles
    }));
  };

  const syncRemotePath = (path: string) => {
    const normalizedPath = normalizeRemotePath(path || "/");
    setRemotePath(normalizedPath);
    setSnapshot((current) => ({
      ...current,
      sessions: current.sessions.map((session) =>
        session.id === activeTab.id ? { ...session, cwd: normalizedPath } : session
      )
    }));
  };

  const appendRecordingInput = (input: string) => {
    if (!isRecordingScript || !input) {
      return;
    }

    const now = Date.now();
    const delayMs = recordingLastInputAt ? Math.min(now - recordingLastInputAt, 5000) : 0;
    setRecordingLastInputAt(now);
    setRecordingEvents((current) => [
      ...current,
      {
        id: `script-event-${now}-${current.length}`,
        input,
        delayMs
      }
    ]);
  };

  const sendTerminalInput = (sessionId: string, input: string, options: { record?: boolean } = {}) => {
    if (options.record !== false) {
      appendRecordingInput(input);
    }

    if (sessionId === activeTab.id && input.endsWith("\r")) {
      const nextPath = inferDirectoryFromCommand(input.replace(/\r$/, ""), remotePath);
      if (nextPath) {
        syncRemotePath(nextPath);
      }
    }

    void window.cnshell?.terminal.write(sessionId, input);
  };

  const startScriptRecording = () => {
    const now = Date.now();
    setIsRecordingScript(true);
    setRecordingStartedAt(now);
    setRecordingLastInputAt(now);
    setRecordingEvents([]);
  };

  const stopScriptRecording = () => {
    if (recordingEvents.length > 0) {
      const createdAt = new Date(recordingStartedAt ?? Date.now()).toISOString();
      const recording: ScriptRecording = {
        id: `script-${Date.now()}`,
        name: `Recording ${new Date().toLocaleTimeString()}`,
        createdAt,
        events: recordingEvents
      };

      setSnapshot((current) => ({
        ...current,
        scriptRecordings: [recording, ...current.scriptRecordings].slice(0, 12)
      }));
    }

    setIsRecordingScript(false);
    setRecordingStartedAt(null);
    setRecordingLastInputAt(null);
    setRecordingEvents([]);
  };

  const replayScriptRecording = (recording: ScriptRecording) => {
    let delay = 0;
    for (const event of recording.events) {
      delay += Math.min(event.delayMs, 3000);
      window.setTimeout(() => {
        sendTerminalInput(activeTab.id, event.input, { record: false });
      }, delay);
    }
  };

  const refreshSessionLog = useCallback(() => {
    setLogStatus("loading");
    void window.cnshell?.logs
      .readSession({
        sessionId: activeTab.id,
        query: logQuery,
        limit: 300
      })
      .then((result) => {
        setLogLines(result.lines);
        setLogStatus("idle");
      })
      .catch(() => {
        setLogLines([]);
        setLogStatus("error");
      });
  }, [activeTab.id, logQuery]);

  const exportCloudSyncSettings = () => {
    setCloudSyncStatus("Exporting encrypted settings");
    void window.cnshell?.cloudSync
      .exportSettings({ snapshot })
      .then((result) => {
        setCloudSyncStatus(result.ok ? `Exported ${result.path ?? ""}` : "Export canceled");
      })
      .catch((error: Error) => {
        setCloudSyncStatus(error.message);
      });
  };

  const importCloudSyncSettings = () => {
    setCloudSyncStatus("Importing encrypted settings");
    void window.cnshell?.cloudSync
      .importSettings()
      .then((result) => {
        if (!result.ok || !result.importedSnapshot) {
          setCloudSyncStatus("Import canceled");
          return;
        }

        const importedSnapshot = hydrateAppSnapshot(result.importedSnapshot);
        setSnapshot(importedSnapshot);
        setRemoteFileEntries(importedSnapshot.remoteFiles);
        setLiveMetrics(importedSnapshot.serverMetrics);
        setRemoteProcesses(importedSnapshot.remoteProcesses);
        setActiveConnectionId(importedSnapshot.connections[0]?.id ?? "");
        setActiveTabId(importedSnapshot.sessions[0]?.id ?? "");
        setCloudSyncStatus(`Imported ${result.path ?? ""}`);
      })
      .catch((error: Error) => {
        setCloudSyncStatus(error.message);
      });
  };

  const startActiveSession = () => {
    setSessionStartTokens((current) => ({
      ...current,
      [activeTab.id]: (current[activeTab.id] ?? 0) + 1
    }));
  };

  const trustActiveHost = () => {
    const prompt = hostKeyPrompts[activeTab.id];
    if (!prompt || prompt.status === "changed") {
      return;
    }

    void window.cnshell?.terminal.trustHost(prompt).then(() => {
      setHostKeyPrompts((current) => {
        const next = { ...current };
        delete next[activeTab.id];
        return next;
      });
      startActiveSession();
    });
  };

  const refreshCredentialStatus = useCallback((connectionId: string) => {
    void window.cnshell?.credentials.status(connectionId).then((status) => {
      setCredentialStatuses((current) => ({
        ...current,
        [connectionId]: status
      }));
    });
  }, []);

  const saveActiveCredential = () => {
    void window.cnshell?.credentials
      .save({
        connectionId: activeConnection.id,
        secret: {
          password: activeSshDraft.password || undefined,
          privateKey: activeSshDraft.privateKey || undefined,
          passphrase: activeSshDraft.passphrase || undefined
        }
      })
      .then((status) => {
        setCredentialStatuses((current) => ({
          ...current,
          [activeConnection.id]: status
        }));
        setCredentialErrors((current) => ({
          ...current,
          [activeConnection.id]: ""
        }));
        setSshDrafts((current) => ({
          ...current,
          [activeConnection.id]: { password: "", privateKey: "", passphrase: "" }
        }));
      })
      .catch((error: Error) => {
        setCredentialErrors((current) => ({
          ...current,
          [activeConnection.id]: error.message
        }));
      });
  };

  const deleteActiveCredential = () => {
    void window.cnshell?.credentials.delete(activeConnection.id).then((status) => {
      setCredentialStatuses((current) => ({
        ...current,
        [activeConnection.id]: status
      }));
      setCredentialErrors((current) => ({
        ...current,
        [activeConnection.id]: ""
      }));
    });
  };

  const openActiveRdp = () => {
    if (activeConnection.protocol !== "rdp") {
      return;
    }

    setRdpStatus("launching");
    setRdpError("");

    void window.cnshell?.rdp
      .open({
        host: activeConnection.host,
        port: activeConnection.port || 3389,
        username: activeConnection.username
      })
      .then(() => {
        setRdpStatus("idle");
      })
      .catch((error: Error) => {
        setRdpStatus("error");
        setRdpError(error.message);
      });
  };

  const refreshRemoteFiles = () => {
    if (activeConnection.protocol !== "ssh") {
      setRemoteFileEntries(snapshot.remoteFiles);
      return;
    }

    setSftpStatus("loading");
    setSftpError("");

    void window.cnshell?.sftp
      .listDirectory({
        path: remotePath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((listing) => {
        syncRemotePath(listing.path);
        setRemoteFileEntries(listing.entries);
        setSftpStatus("idle");
      })
      .catch((error: Error) => {
        setSftpError(error.message);
        setSftpStatus("error");
      });
  };

  const startTransfer = (direction: "upload" | "download") => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const localPath = transferLocalPath.trim();
    const remoteTransferPath = transferRemotePath.trim();
    if (!localPath || !remoteTransferPath) {
      return;
    }

    const jobId = `${direction}-${Date.now()}`;
    const job: TransferJob = {
      id: jobId,
      direction,
      localPath,
      remotePath: remoteTransferPath,
      status: "running"
    };

    setTransferJobs((current) => [job, ...current].slice(0, 8));

    void window.cnshell?.sftp
      .transferFile({
        direction,
        localPath,
        remotePath: remoteTransferPath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => {
        setTransferJobs((current) =>
          current.map((item) => (item.id === jobId ? { ...item, status: "completed", message: "Done" } : item))
        );
        refreshRemoteFiles();
      })
      .catch((error: Error) => {
        setTransferJobs((current) =>
          current.map((item) => (item.id === jobId ? { ...item, status: "error", message: error.message } : item))
        );
      });
  };

  const openRemoteFile = (remoteFilePath: string) => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    setEditorPath(remoteFilePath);
    setEditorStatus("loading");
    setEditorError("");

    void window.cnshell?.sftp
      .readFile({
        remotePath: remoteFilePath,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setEditorPath(result.remotePath);
        setEditorContent(result.content);
        setEditorStatus("idle");
      })
      .catch((error: Error) => {
        setEditorError(error.message);
        setEditorStatus("error");
      });
  };

  const saveRemoteFile = () => {
    if (activeConnection.protocol !== "ssh" || !editorPath) {
      return;
    }

    setEditorStatus("saving");
    setEditorError("");

    void window.cnshell?.sftp
      .writeFile({
        remotePath: editorPath,
        content: editorContent,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => {
        setEditorStatus("saved");
        refreshRemoteFiles();
      })
      .catch((error: Error) => {
        setEditorError(error.message);
        setEditorStatus("error");
      });
  };

  const appendMetricHistory = (
    metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"],
    processCount = remoteProcesses.length
  ) => {
    const now = new Date();
    setMetricHistory((current) =>
      [
        ...current,
        {
          at: now.toLocaleTimeString(),
          cpu: metricValue(metrics, "CPU"),
          memory: metricValue(metrics, "Memory"),
          disk: metricValue(metrics, "Disk"),
          network: metricValue(metrics, "Ping"),
          processes: processCount
        }
      ].slice(-20)
    );
  };

  const refreshMetrics = () => {
    if (activeConnection.protocol !== "ssh") {
      setLiveMetrics(snapshot.serverMetrics);
      return;
    }

    setMetricsStatus("loading");
    setMetricsError("");

    void window.cnshell?.metrics
      .collect({
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setLiveMetrics(result.metrics);
        appendMetricHistory(result.metrics);
        setMetricsStatus("idle");
      })
      .catch((error: Error) => {
        setMetricsError(error.message);
        setMetricsStatus("error");
      });
  };

  const refreshProcesses = () => {
    if (activeConnection.protocol !== "ssh") {
      setRemoteProcesses(snapshot.remoteProcesses);
      return;
    }

    setProcessStatus("loading");
    setProcessError("");

    void window.cnshell?.metrics
      .listProcesses({
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((result) => {
        setRemoteProcesses(result.processes);
        appendMetricHistory(liveMetrics, result.processes.length);
        setProcessStatus("idle");
      })
      .catch((error: Error) => {
        setProcessError(error.message);
        setProcessStatus("error");
      });
  };

  const killProcess = (pid: number) => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    setProcessStatus("loading");
    setProcessError("");

    void window.cnshell?.metrics
      .killProcess({
        pid,
        signal: "TERM",
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then(() => refreshProcesses())
      .catch((error: Error) => {
        setProcessError(error.message);
        setProcessStatus("error");
      });
  };

  const executeCommand = (command: string) => {
    const targetSessionIds = isSyncInputEnabled
      ? sessionTabsWithStatus.filter((session) => session.status !== "error").map((session) => session.id)
      : [activeTab.id];

    for (const sessionId of targetSessionIds) {
      sendTerminalInput(sessionId, `${command}\r`);
    }

    setIsCommandPaletteOpen(false);
    setCommandQuery("");
  };

  const addTriggerEvents = useCallback((events: TriggerEvent[]) => {
    if (events.length === 0) {
      return;
    }

    setTriggerEvents((current) => [...events, ...current].slice(0, 8));
  }, []);

  const handleZmodemDetected = useCallback((mode: ZmodemMode) => {
    if (mode === "idle") {
      return;
    }

    setZmodemMode(mode);
    setZmodemMessage(
      mode === "upload"
        ? "Remote rz is waiting. Use the ZMODEM panel to upload."
        : mode === "download"
          ? "Remote sz transfer detected. Use the ZMODEM panel to download."
          : "ZMODEM activity detected."
    );
  }, []);

  const startTunnel = () => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const tunnelId = `tunnel-${Date.now()}`;
    const bindPort = parsePort(tunnelDraft.bindPort);
    const parsedTargetPort = tunnelDraft.mode === "dynamic" ? null : parsePort(tunnelDraft.targetPort);
    const bindHost = tunnelDraft.bindHost.trim();
    const targetHost = tunnelDraft.targetHost.trim();
    if (!bindPort || !bindHost || (tunnelDraft.mode !== "dynamic" && (!parsedTargetPort || !targetHost))) {
      return;
    }
    const targetPort = tunnelDraft.mode === "dynamic" ? undefined : parsedTargetPort ?? undefined;

    const startingTunnel: TunnelInfo = {
      id: tunnelId,
      mode: tunnelDraft.mode,
      bindHost,
      bindPort,
      targetHost: tunnelDraft.mode === "dynamic" ? undefined : targetHost,
      targetPort,
      status: "starting"
    };
    setTunnels((current) => [startingTunnel, ...current].slice(0, 6));

    void window.cnshell?.tunnels
      .start({
        id: tunnelId,
        mode: tunnelDraft.mode,
        bindHost,
        bindPort,
        targetHost: tunnelDraft.mode === "dynamic" ? undefined : targetHost,
        targetPort,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((info) => {
        setTunnels((current) => current.map((tunnel) => (tunnel.id === tunnelId ? info : tunnel)));
      })
      .catch((error: Error) => {
        setTunnels((current) =>
          current.map((tunnel) =>
            tunnel.id === tunnelId ? { ...tunnel, status: "error", message: error.message } : tunnel
          )
        );
      });
  };

  const stopTunnel = (id: string) => {
    void window.cnshell?.tunnels.stop(id).then(() => {
      setTunnels((current) =>
        current.map((tunnel) => (tunnel.id === id ? { ...tunnel, status: "stopped" } : tunnel))
      );
    });
  };

  const startRelay = () => {
    if (activeConnection.protocol !== "ssh") {
      return;
    }

    const relayPort = parsePort(relayDraft.relayPort);
    const targetPort = parsePort(relayDraft.targetPort);
    const relayHost = relayDraft.relayHost.trim();
    const targetHost = relayDraft.targetHost.trim();
    if (!relayPort || !targetPort || !relayHost || !targetHost) {
      return;
    }

    const relayId = `relay-${Date.now()}`;
    const startingRelay: RelayInfo = {
      id: relayId,
      relayHost,
      relayPort,
      targetHost,
      targetPort,
      status: "starting"
    };
    setRelays((current) => [startingRelay, ...current].slice(0, 5));

    void window.cnshell?.relay
      .start({
        id: relayId,
        relayHost,
        relayPort,
        targetHost,
        targetPort,
        ssh: createSshConfig(activeConnection, activeSshDraft, Boolean(activeCredentialStatus?.hasCredential))
      })
      .then((info) => {
        setRelays((current) => current.map((relay) => (relay.id === relayId ? info : relay)));
      })
      .catch((error: Error) => {
        setRelays((current) =>
          current.map((relay) => (relay.id === relayId ? { ...relay, status: "error", message: error.message } : relay))
        );
      });
  };

  const stopRelay = (id: string) => {
    void window.cnshell?.relay.stop(id).then(() => {
      setRelays((current) => current.map((relay) => (relay.id === id ? { ...relay, status: "stopped" } : relay)));
    });
  };

  const createSessionForActiveConnection = () => {
    const sessionId = `tab-${activeConnection.id}-${Date.now()}`;
    const nextSession: SessionTab = {
      id: sessionId,
      connectionId: activeConnection.id,
      title: activeConnection.name,
      cwd: activeConnection.protocol === "local" ? "~" : "/",
      status: "disconnected",
      startedAt: new Date().toISOString()
    };

    setSnapshot((current) => ({
      ...current,
      sessions: [...current.sessions, nextSession]
    }));
    setSessionStatuses((current) => ({
      ...current,
      [sessionId]: "disconnected"
    }));
    setActiveTabId(sessionId);
  };

  useEffect(() => {
    void window.cnshell?.getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    void workspaceStorage.loadSnapshot().then((storedSnapshot) => {
      if (storedSnapshot) {
        const hydratedSnapshot = hydrateAppSnapshot(storedSnapshot);
        setSnapshot(hydratedSnapshot);
        setRemoteFileEntries(hydratedSnapshot.remoteFiles);
        setLiveMetrics(hydratedSnapshot.serverMetrics);
        setRemoteProcesses(hydratedSnapshot.remoteProcesses);
        setActiveConnectionId(hydratedSnapshot.connections[0]?.id ?? "");
        setActiveTabId(hydratedSnapshot.sessions[0]?.id ?? "");
      }

      setIsWorkspaceReady(true);
    });
  }, []);

  useEffect(() => {
    if (isWorkspaceReady) {
      void workspaceStorage.saveSnapshot(snapshot);
    }
  }, [isWorkspaceReady, snapshot]);

  useEffect(() => {
    return window.cnshell?.terminal.onHostKeyVerification((event) => {
      setHostKeyPrompts((current) => ({
        ...current,
        [event.id]: event
      }));
      setSessionStatus(event.id, "error");
    });
  }, [setSessionStatus]);

  useEffect(() => {
    if (activeConnection.protocol === "ssh") {
      refreshCredentialStatus(activeConnection.id);
    }
  }, [activeConnection.id, activeConnection.protocol, refreshCredentialStatus]);

  useEffect(() => {
    refreshSessionLog();
  }, [refreshSessionLog]);

  useEffect(() => {
    setRemotePath(activeTab.cwd || "/");
  }, [activeTab.cwd, activeTab.id]);

  if (!isWorkspaceReady) {
    return (
      <main className="app-shell loading-shell">
        <section className="workspace-loading" aria-live="polite">
          <div className="brand-mark" aria-hidden="true">
            CN
          </div>
          <strong>Loading CNshell workspace</strong>
          <span>Preparing connections and sessions</span>
        </section>
      </main>
    );
  }

  return (
    <main className="app-shell">
      <ConnectionSidebar
        groupedConnections={groupedConnections}
        activeConnectionId={activeConnectionId}
        onSelect={(connectionId) => {
          setActiveConnectionId(connectionId);
          const nextTab = snapshot.sessions.find((tab) => tab.connectionId === connectionId);
          if (nextTab) {
            setActiveTabId(nextTab.id);
          }
        }}
      />
      <section className="workspace" aria-label="CNshell workspace">
        <TopBar
          activeConnection={activeConnection}
          status={activeTab.status}
          version={appVersion}
          isSyncInputEnabled={isSyncInputEnabled}
          isHighlightEnabled={isHighlightEnabled}
          onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
          onToggleSyncInput={() => setIsSyncInputEnabled((current) => !current)}
          onToggleHighlight={() => setIsHighlightEnabled((current) => !current)}
        />
        <TabStrip
          tabs={sessionTabsWithStatus}
          activeTabId={activeTabId}
          onSelect={setActiveTabId}
          onCreate={createSessionForActiveConnection}
        />
        <section className="workspace-grid">
          <TerminalPane
            activeConnection={activeConnection}
            activeTab={activeTab}
            sshDraft={activeSshDraft}
            useSavedCredential={Boolean(activeCredentialStatus?.hasCredential)}
            keyMappingProfiles={snapshot.keyMappingProfiles}
            startToken={sessionStartTokens[activeTab.id] ?? 0}
            isHighlightEnabled={isHighlightEnabled}
            zmodemMode={zmodemMode}
            onStatusChange={setSessionStatus}
            onReconnect={startActiveSession}
            onDispatchCommand={executeCommand}
            onTerminalInput={sendTerminalInput}
            onTriggerEvents={addTriggerEvents}
            onZmodemDetected={handleZmodemDetected}
          />
          <aside className="ops-panel" aria-label="Operations panels">
            {activeConnection.protocol === "ssh" ? (
              <SshCredentialPanel
                authMethod={activeConnection.authMethod}
                draft={activeSshDraft}
                credentialStatus={activeCredentialStatus}
                credentialError={credentialErrors[activeConnection.id]}
                hostKeyPrompt={hostKeyPrompts[activeTab.id]}
                onChange={updateActiveSshDraft}
                onConnect={startActiveSession}
                onSaveCredential={saveActiveCredential}
                onDeleteCredential={deleteActiveCredential}
                onTrustHost={trustActiveHost}
              />
            ) : null}
            {activeConnection.protocol === "rdp" ? (
              <RdpPanel connection={activeConnection} status={rdpStatus} error={rdpError} onOpen={openActiveRdp} />
            ) : null}
            {activeConnection.protocol === "ssh" ? (
              <JumpHostPanel gateways={activeConnection.gateways ?? []} onChange={updateActiveGateways} />
            ) : null}
            <FilePanel
              remoteFiles={remoteFileEntries}
              path={remotePath}
              status={sftpStatus}
              error={sftpError}
              localPath={transferLocalPath}
              transferRemotePath={transferRemotePath}
              transferJobs={transferJobs}
              onPathChange={syncRemotePath}
              onLocalPathChange={setTransferLocalPath}
              onTransferRemotePathChange={setTransferRemotePath}
              onRefresh={refreshRemoteFiles}
              onTransfer={startTransfer}
              onOpenFile={openRemoteFile}
            />
            <ZmodemPanel
              mode={zmodemMode}
              message={zmodemMessage}
              localPath={transferLocalPath}
              remotePath={transferRemotePath}
              onLocalPathChange={setTransferLocalPath}
              onRemotePathChange={setTransferRemotePath}
              onUpload={() => {
                setZmodemMode("upload");
                setZmodemMessage("Uploading through ZMODEM-compatible transfer flow");
                startTransfer("upload");
              }}
              onDownload={() => {
                setZmodemMode("download");
                setZmodemMessage("Downloading through ZMODEM-compatible transfer flow");
                startTransfer("download");
              }}
            />
            <RemoteEditorPanel
              path={editorPath}
              content={editorContent}
              status={editorStatus}
              error={editorError}
              onContentChange={setEditorContent}
              onSave={saveRemoteFile}
            />
            <MetricsPanel
              metrics={liveMetrics}
              history={metricHistory}
              status={metricsStatus}
              error={metricsError}
              onRefresh={refreshMetrics}
            />
            <ProcessPanel
              processes={remoteProcesses}
              status={processStatus}
              error={processError}
              onRefresh={refreshProcesses}
              onKill={killProcess}
            />
            <TunnelPanel
              draft={tunnelDraft}
              tunnels={tunnels}
              onDraftChange={setTunnelDraft}
              onStart={startTunnel}
              onStop={stopTunnel}
            />
            <RelayPanel
              draft={relayDraft}
              relays={relays}
              onDraftChange={setRelayDraft}
              onStart={startRelay}
              onStop={stopRelay}
            />
            <KeyMappingPanel profiles={snapshot.keyMappingProfiles} onChange={updateKeyMappingProfiles} />
            <ScriptRecorderPanel
              isRecording={isRecordingScript}
              eventCount={recordingEvents.length}
              recordings={snapshot.scriptRecordings}
              onStart={startScriptRecording}
              onStop={stopScriptRecording}
              onReplay={replayScriptRecording}
            />
            <LogViewerPanel
              query={logQuery}
              lines={logLines}
              status={logStatus}
              onQueryChange={setLogQuery}
              onRefresh={refreshSessionLog}
            />
            <QuickCommandPanel quickCommands={snapshot.quickCommands} onExecute={executeCommand} />
            <TriggerPanel events={triggerEvents} />
            <CloudSyncPanel
              status={cloudSyncStatus}
              onExport={exportCloudSyncSettings}
              onImport={importCloudSyncSettings}
            />
          </aside>
        </section>
      </section>
      {isCommandPaletteOpen ? (
        <CommandPalette
          commands={snapshot.quickCommands}
          query={commandQuery}
          onQueryChange={setCommandQuery}
          onExecute={executeCommand}
          onClose={() => setIsCommandPaletteOpen(false)}
        />
      ) : null}
    </main>
  );
}

interface ConnectionSidebarProps {
  groupedConnections: Record<string, ConnectionProfile[]>;
  activeConnectionId: string;
  onSelect: (connectionId: string) => void;
}

function ConnectionSidebar({ groupedConnections, activeConnectionId, onSelect }: ConnectionSidebarProps) {
  return (
    <aside className="sidebar" aria-label="Connection manager">
      <div className="brand-row">
        <div className="brand-mark" aria-hidden="true">
          CN
        </div>
        <div>
          <h1>CNshell</h1>
          <p>SSH Operations Console</p>
        </div>
      </div>

      <label className="search-box">
        <Search size={17} aria-hidden="true" />
        <span className="sr-only">Search connections</span>
        <input placeholder="Search hosts, tags, groups" />
      </label>

      <div className="sidebar-actions" aria-label="Connection actions">
        <button type="button">
          <Plus size={16} aria-hidden="true" />
          New
        </button>
        <button type="button" aria-label="Connection settings">
          <Settings size={16} aria-hidden="true" />
        </button>
      </div>

      <nav className="connection-tree">
        {Object.entries(groupedConnections).map(([group, connections]) => (
          <section key={group} className="connection-group" aria-label={`${group} group`}>
            <button type="button" className="group-title">
              <ChevronRight size={15} aria-hidden="true" />
              {group}
              <span>{connections.length}</span>
            </button>
            {connections.map((connection) => (
              <button
                type="button"
                key={connection.id}
                className={`connection-item ${connection.id === activeConnectionId ? "active" : ""}`}
                onClick={() => onSelect(connection.id)}
              >
                <span className="connection-color" style={{ background: connection.color }} aria-hidden="true" />
                <span className="connection-copy">
                  <strong>{connection.name}</strong>
                  <small>
                    {connection.username}@{connection.host}
                  </small>
                </span>
                <ProtocolIcon protocol={connection.protocol} />
              </button>
            ))}
          </section>
        ))}
      </nav>
    </aside>
  );
}

function ProtocolIcon({ protocol }: { protocol: ConnectionProfile["protocol"] }) {
  if (protocol === "rdp") {
    return <Monitor size={15} aria-label="RDP" />;
  }

  if (protocol === "local") {
    return <TerminalSquare size={15} aria-label="Local shell" />;
  }

  return <Server size={15} aria-label="SSH" />;
}

function TopBar({
  activeConnection,
  status,
  version,
  isSyncInputEnabled,
  isHighlightEnabled,
  onOpenCommandPalette,
  onToggleSyncInput,
  onToggleHighlight
}: {
  activeConnection: ConnectionProfile;
  status: SessionStatus;
  version: string;
  isSyncInputEnabled: boolean;
  isHighlightEnabled: boolean;
  onOpenCommandPalette: () => void;
  onToggleSyncInput: () => void;
  onToggleHighlight: () => void;
}) {
  return (
    <header className="topbar">
      <div className="host-summary">
        <span className={`status-pill ${status}`}>
          <Circle size={9} fill="currentColor" aria-hidden="true" />
          {status}
        </span>
        <div>
          <strong>{activeConnection.name}</strong>
          <span>
            {activeConnection.protocol.toUpperCase()} / {activeConnection.host}:{activeConnection.port || "local"}
          </span>
        </div>
      </div>
      <div className="topbar-actions">
        <button type="button" aria-label="Open command palette" onClick={onOpenCommandPalette}>
          <Command size={17} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={isSyncInputEnabled ? "active" : ""}
          aria-label="Toggle synchronized input"
          aria-pressed={isSyncInputEnabled}
          onClick={onToggleSyncInput}
        >
          <SplitSquareHorizontal size={17} aria-hidden="true" />
        </button>
        <button
          type="button"
          className={isHighlightEnabled ? "active" : ""}
          aria-label="Toggle highlight rules"
          aria-pressed={isHighlightEnabled}
          onClick={onToggleHighlight}
        >
          <Zap size={17} aria-hidden="true" />
        </button>
        <button type="button" aria-label="Open tunneling manager">
          <Network size={17} aria-hidden="true" />
        </button>
        <button type="button" aria-label="Open credential vault">
          <KeyRound size={17} aria-hidden="true" />
        </button>
        <span className="version-label">v{version}</span>
      </div>
    </header>
  );
}

function TabStrip({
  tabs,
  activeTabId,
  onSelect,
  onCreate
}: {
  tabs: SessionTab[];
  activeTabId: string;
  onSelect: (tabId: string) => void;
  onCreate: () => void;
}) {
  return (
    <div className="tab-strip" role="tablist" aria-label="Session tabs">
      {tabs.map((tab) => (
        <button
          type="button"
          role="tab"
          aria-selected={tab.id === activeTabId}
          key={tab.id}
          className={`session-tab ${tab.id === activeTabId ? "active" : ""}`}
          onClick={() => onSelect(tab.id)}
        >
          <TerminalSquare size={15} aria-hidden="true" />
          <span>{tab.title}</span>
          <small className={tab.status}>{tab.status}</small>
        </button>
      ))}
      <button type="button" className="new-tab" aria-label="Open new session tab" onClick={onCreate}>
        <Plus size={16} aria-hidden="true" />
      </button>
    </div>
  );
}

function TerminalPane({
  activeConnection,
  activeTab,
  sshDraft,
  useSavedCredential,
  keyMappingProfiles,
  startToken,
  isHighlightEnabled,
  zmodemMode,
  onStatusChange,
  onReconnect,
  onDispatchCommand,
  onTerminalInput,
  onTriggerEvents,
  onZmodemDetected
}: {
  activeConnection: ConnectionProfile;
  activeTab: SessionTab;
  sshDraft: { password: string; privateKey: string; passphrase: string };
  useSavedCredential: boolean;
  keyMappingProfiles: KeyMappingProfile[];
  startToken: number;
  isHighlightEnabled: boolean;
  zmodemMode: ZmodemMode;
  onStatusChange: (sessionId: string, status: SessionStatus) => void;
  onReconnect: () => void;
  onDispatchCommand: (command: string) => void;
  onTerminalInput: (sessionId: string, input: string, options?: { record?: boolean }) => void;
  onTriggerEvents: (events: TriggerEvent[]) => void;
  onZmodemDetected: (mode: ZmodemMode) => void;
}) {
  const [composeValue, setComposeValue] = useState("");
  const [terminalSearch, setTerminalSearch] = useState("");
  const [searchAddon, setSearchAddon] = useState<SearchAddon | null>(null);
  const [safePasteReview, setSafePasteReview] = useState<SafePasteReview | null>(null);
  const [safePasteSessionId, setSafePasteSessionId] = useState("");

  useEffect(() => {
    const host = activeConnection.host;
    const terminalHost = document.getElementById("terminal-host");
    if (!terminalHost) {
      return;
    }

    terminalHost.innerHTML = "";
    const terminal = new Terminal({
      cursorBlink: true,
      fontFamily: "'Cascadia Code', 'JetBrains Mono', Consolas, monospace",
      fontSize: 13,
      lineHeight: 1.32,
      theme: terminalTheme
    });

    const fitAddon = new FitAddon();
    const activeSearchAddon = new SearchAddon();
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(activeSearchAddon);
    setSearchAddon(activeSearchAddon);
    terminal.open(terminalHost);
    fitAddon.fit();
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") {
        return true;
      }

      const key = formatKeyEvent(event);
      const rule = getActiveKeyRules(keyMappingProfiles).find((item) => item.key === key);
      if (!rule) {
        return true;
      }

      onTerminalInput(activeTab.id, normalizeSendValue(rule.send));
      return false;
    });
    terminal.writeln("\x1b[1;32mCNshell terminal session starting\x1b[0m");
    terminal.writeln(`Profile: ${activeConnection.username}@${host}`);
    terminal.writeln("");

    const sessionId = activeTab.id;
    const removeDataListener = window.cnshell?.terminal.onData(({ id, data }) => {
      if (id === sessionId) {
        onTriggerEvents(detectTriggerEvents(sessionId, data));
        onZmodemDetected(detectZmodemMode(data));
        terminal.write(isHighlightEnabled ? applyHighlightRules(data) : data);
      }
    });

    const removeExitListener = window.cnshell?.terminal.onExit(({ id, exitCode }) => {
      if (id === sessionId) {
        terminal.writeln("");
        terminal.writeln(`\x1b[33mSession exited with code ${exitCode}.\x1b[0m`);
        onStatusChange(sessionId, "disconnected");
      }
    });

    const dataDisposable = terminal.onData((data) => {
      onTerminalInput(sessionId, data);
    });

    const pasteHandler = (event: ClipboardEvent) => {
      const text = event.clipboardData?.getData("text/plain") ?? "";
      if (!text || !shouldReviewPaste(text)) {
        return;
      }

      event.preventDefault();
      setSafePasteSessionId(sessionId);
      setSafePasteReview({
        text,
        reasons: inspectPastedText(text)
      });
    };
    terminal.textarea?.addEventListener("paste", pasteHandler);

    const resizeSession = () => {
      fitAddon.fit();
      void window.cnshell?.terminal.resize({
        id: sessionId,
        cols: terminal.cols,
        rows: terminal.rows
      });
    };

    const removeErrorListener = window.cnshell?.terminal.onError(({ id, message }) => {
      if (id === sessionId) {
        terminal.writeln("");
        terminal.writeln(`\x1b[31m${message}\x1b[0m`);
        onStatusChange(sessionId, "error");
      }
    });

    const startTerminalSession = () => {
      onStatusChange(sessionId, "connecting");

      void window.cnshell?.terminal
        .start({
          id: sessionId,
          kind: activeConnection.protocol === "ssh" ? "ssh" : "local",
          cols: terminal.cols,
          rows: terminal.rows,
          ssh:
            activeConnection.protocol === "ssh"
              ? createSshConfig(activeConnection, sshDraft, useSavedCredential)
              : undefined
        })
        .then(() => onStatusChange(sessionId, "connected"))
        .catch((error: Error) => {
          terminal.writeln(`\x1b[31m${error.message}\x1b[0m`);
          onStatusChange(sessionId, "error");
        });
    };

    if (activeConnection.protocol === "ssh") {
      terminal.writeln("\x1b[33mSSH profile selected. Enter credentials in the SSH panel, then press Connect.\x1b[0m");
      if (startToken > 0) {
        startTerminalSession();
      } else {
        onStatusChange(sessionId, "disconnected");
      }
    } else {
      if (activeConnection.protocol === "rdp") {
        terminal.writeln("\x1b[33mRDP profile selected. Use the RDP panel to launch Windows Remote Desktop.\x1b[0m");
        onStatusChange(sessionId, "disconnected");
      } else {
        startTerminalSession();
      }
    }

    const resizeObserver = new ResizeObserver(resizeSession);
    resizeObserver.observe(terminalHost);

    return () => {
      resizeObserver.disconnect();
      dataDisposable.dispose();
      removeDataListener?.();
      removeExitListener?.();
      removeErrorListener?.();
      terminal.textarea?.removeEventListener("paste", pasteHandler);
      void window.cnshell?.terminal.stop(sessionId);
      onStatusChange(sessionId, "disconnected");
      setSearchAddon(null);
      terminal.dispose();
    };
  }, [
    activeConnection,
    activeTab,
    onStatusChange,
    sshDraft,
    sshDraft.password,
    sshDraft.passphrase,
    sshDraft.privateKey,
    keyMappingProfiles,
    useSavedCredential,
    isHighlightEnabled,
    onTerminalInput,
    onTriggerEvents,
    onZmodemDetected,
    startToken
  ]);

  const sendComposeValue = () => {
    const command = composeValue.trim();
    if (!command) {
      return;
    }

    onDispatchCommand(command);
    setComposeValue("");
  };

  const findNext = () => {
    if (terminalSearch.trim()) {
      searchAddon?.findNext(terminalSearch);
    }
  };

  const approveSafePaste = () => {
    if (!safePasteReview) {
      return;
    }

    onTerminalInput(safePasteSessionId || activeTab.id, safePasteReview.text);
    setSafePasteReview(null);
    setSafePasteSessionId("");
  };

  const cancelSafePaste = () => {
    setSafePasteReview(null);
    setSafePasteSessionId("");
  };

  return (
    <section className="terminal-workbench" aria-label="Terminal workbench">
      <div className="terminal-toolbar">
        <div className="breadcrumb">
          <HardDrive size={16} aria-hidden="true" />
          <span>{activeTab.cwd}</span>
        </div>
        <div className="terminal-tools">
          <label className="terminal-search">
            <Search size={15} aria-hidden="true" />
            <input
              value={terminalSearch}
              placeholder="Search"
              onChange={(event) => setTerminalSearch(event.target.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  findNext();
                }
              }}
            />
          </label>
          <button type="button" onClick={findNext}>
            Find
          </button>
          <button type="button">
            <SplitSquareHorizontal size={16} aria-hidden="true" />
            Split
          </button>
          <button type="button" className={zmodemMode !== "idle" ? "active" : ""}>
            <UploadCloud size={16} aria-hidden="true" />
            ZMODEM
          </button>
          <button type="button" onClick={onReconnect}>
            <RefreshCw size={16} aria-hidden="true" />
            Reconnect
          </button>
          <button type="button" aria-label="More terminal actions">
            <MoreHorizontal size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
      <div id="terminal-host" className="terminal-host" />
      {safePasteReview ? (
        <div className="safe-paste-review" role="alert">
          <div>
            <strong>Review paste</strong>
            <span>{safePasteReview.reasons.join(" / ")}</span>
          </div>
          <pre>{safePasteReview.text.slice(0, 420)}</pre>
          <button type="button" onClick={approveSafePaste}>
            Paste
          </button>
          <button type="button" onClick={cancelSafePaste}>
            Cancel
          </button>
        </div>
      ) : null}
      <div className="compose-pane">
        <div>
          <Code2 size={16} aria-hidden="true" />
          <span>Compose Pane</span>
        </div>
        <textarea
          value={composeValue}
          placeholder="Draft a command before sending to one or many sessions"
          onChange={(event) => setComposeValue(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && (event.ctrlKey || event.metaKey)) {
              event.preventDefault();
              sendComposeValue();
            }
          }}
        />
        <button type="button" onClick={sendComposeValue}>
          Send
        </button>
      </div>
    </section>
  );
}

function SshCredentialPanel({
  authMethod,
  draft,
  credentialStatus,
  credentialError,
  hostKeyPrompt,
  onChange,
  onConnect,
  onSaveCredential,
  onDeleteCredential,
  onTrustHost
}: {
  authMethod: ConnectionProfile["authMethod"];
  draft: { password: string; privateKey: string; passphrase: string };
  credentialStatus?: CredentialStatus;
  credentialError?: string;
  hostKeyPrompt?: HostKeyVerificationEvent;
  onChange: (field: "password" | "privateKey" | "passphrase", value: string) => void;
  onConnect: () => void;
  onSaveCredential: () => void;
  onDeleteCredential: () => void;
  onTrustHost: () => void;
}) {
  const hasDraftSecret = Boolean(draft.password || draft.privateKey);

  return (
    <section className="panel-section ssh-panel" aria-label="SSH credentials">
      <div className="panel-heading">
        <div>
          <KeyRound size={16} aria-hidden="true" />
          <h2>SSH Login</h2>
        </div>
        <span className="poll-rate">{authMethod}</span>
      </div>
      <div className="ssh-form">
        <div className="credential-status-row">
          <span className={credentialStatus?.hasCredential ? "saved" : ""}>
            {credentialStatus?.hasCredential ? "Saved credential available" : "No saved credential"}
          </span>
          {credentialStatus?.encryptionAvailable === false ? <small>Encryption unavailable</small> : null}
        </div>
        {credentialError ? (
          <div className="credential-error" role="alert">
            {credentialError}
          </div>
        ) : null}
        {hostKeyPrompt ? (
          <div className={`host-key-prompt ${hostKeyPrompt.status}`} role="alert">
            <strong>{hostKeyPrompt.status === "changed" ? "Host key changed" : "Unknown host key"}</strong>
            <span>
              {hostKeyPrompt.host}:{hostKeyPrompt.port}
            </span>
            <code>{hostKeyPrompt.fingerprint}</code>
            {hostKeyPrompt.expectedFingerprint ? <small>Expected {hostKeyPrompt.expectedFingerprint}</small> : null}
            <button type="button" disabled={hostKeyPrompt.status === "changed"} onClick={onTrustHost}>
              <ShieldCheck size={16} aria-hidden="true" />
              Trust and reconnect
            </button>
          </div>
        ) : null}
        <label>
          <span>Password</span>
          <input
            type="password"
            value={draft.password}
            placeholder="Session only"
            onChange={(event) => onChange("password", event.target.value)}
          />
        </label>
        <label>
          <span>Private key</span>
          <textarea
            value={draft.privateKey}
            placeholder="Paste an OpenSSH private key for this session"
            onChange={(event) => onChange("privateKey", event.target.value)}
          />
        </label>
        <label>
          <span>Passphrase</span>
          <input
            type="password"
            value={draft.passphrase}
            placeholder="Optional"
            onChange={(event) => onChange("passphrase", event.target.value)}
          />
        </label>
        <button type="button" onClick={onConnect}>
          <TerminalSquare size={16} aria-hidden="true" />
          Connect
        </button>
        <div className="credential-actions">
          <button
            type="button"
            disabled={!hasDraftSecret || credentialStatus?.encryptionAvailable === false}
            onClick={onSaveCredential}
          >
            <ShieldCheck size={16} aria-hidden="true" />
            Save credential
          </button>
          <button type="button" disabled={!credentialStatus?.hasCredential} onClick={onDeleteCredential}>
            Delete saved
          </button>
        </div>
      </div>
    </section>
  );
}

function RdpPanel({
  connection,
  status,
  error,
  onOpen
}: {
  connection: ConnectionProfile;
  status: "idle" | "launching" | "error";
  error: string;
  onOpen: () => void;
}) {
  return (
    <section className="panel-section rdp-panel" aria-label="RDP connection">
      <div className="panel-heading">
        <div>
          <Monitor size={16} aria-hidden="true" />
          <h2>RDP</h2>
        </div>
        <span className={`rdp-status ${status}`}>{status}</span>
      </div>
      <div className="rdp-body">
        <div className="rdp-target">
          <strong>{connection.host}:{connection.port || 3389}</strong>
          <span>{connection.username}</span>
        </div>
        {error ? (
          <div className="credential-error" role="alert">
            {error}
          </div>
        ) : null}
        <button type="button" onClick={onOpen}>
          <Monitor size={16} aria-hidden="true" />
          Open Remote Desktop
        </button>
      </div>
    </section>
  );
}

function JumpHostPanel({
  gateways,
  onChange
}: {
  gateways: JumpHostConfig[];
  onChange: (gateways: JumpHostConfig[]) => void;
}) {
  const addGateway = () => {
    onChange([
      ...gateways,
      {
        id: `gateway-${Date.now()}`,
        name: `jump-${gateways.length + 1}`,
        host: "127.0.0.1",
        port: 22,
        username: "deploy"
      }
    ]);
  };

  const updateGateway = (id: string, patch: Partial<JumpHostConfig>) => {
    onChange(gateways.map((gateway) => (gateway.id === id ? { ...gateway, ...patch } : gateway)));
  };

  const removeGateway = (id: string) => {
    onChange(gateways.filter((gateway) => gateway.id !== id));
  };

  return (
    <section className="panel-section" aria-label="Jump host proxy">
      <div className="panel-heading">
        <div>
          <SplitSquareHorizontal size={16} aria-hidden="true" />
          <h2>Jump Hosts</h2>
        </div>
        <button type="button" aria-label="Add jump host" onClick={addGateway}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="jump-host-list">
        {gateways.length === 0 ? (
          <div className="trigger-empty">Direct SSH connection</div>
        ) : (
          gateways.map((gateway, index) => (
            <div key={gateway.id} className="jump-host-row">
              <strong>{index + 1}</strong>
              <input
                value={gateway.name}
                placeholder="Name"
                aria-label={`Jump host ${index + 1} name`}
                onChange={(event) => updateGateway(gateway.id, { name: event.target.value })}
              />
              <input
                value={gateway.host}
                placeholder="Host"
                aria-label={`Jump host ${index + 1} host`}
                onChange={(event) => updateGateway(gateway.id, { host: event.target.value })}
              />
              <input
                value={gateway.port}
                placeholder="Port"
                aria-label={`Jump host ${index + 1} port`}
                onChange={(event) => updateGateway(gateway.id, { port: Number(event.target.value) || 22 })}
              />
              <input
                value={gateway.username}
                placeholder="User"
                aria-label={`Jump host ${index + 1} user`}
                onChange={(event) => updateGateway(gateway.id, { username: event.target.value })}
              />
              <button type="button" onClick={() => removeGateway(gateway.id)}>
                Remove
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function FilePanel({
  remoteFiles,
  path,
  status,
  error,
  localPath,
  transferRemotePath,
  transferJobs,
  onPathChange,
  onLocalPathChange,
  onTransferRemotePathChange,
  onRefresh,
  onTransfer,
  onOpenFile
}: {
  remoteFiles: ReturnType<typeof createInitialAppSnapshot>["remoteFiles"];
  path: string;
  status: "idle" | "loading" | "error";
  error: string;
  localPath: string;
  transferRemotePath: string;
  transferJobs: TransferJob[];
  onPathChange: (path: string) => void;
  onLocalPathChange: (path: string) => void;
  onTransferRemotePathChange: (path: string) => void;
  onRefresh: () => void;
  onTransfer: (direction: "upload" | "download") => void;
  onOpenFile: (path: string) => void;
}) {
  return (
    <section className="panel-section file-panel" aria-label="Remote files">
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>SFTP</h2>
        </div>
        <div className="panel-actions">
          <span className="sync-pill">cwd sync</span>
          <button type="button" aria-label="Refresh remote files" onClick={onRefresh}>
            <RefreshCw size={16} aria-hidden="true" />
          </button>
        </div>
      </div>
      <label className="path-row">
        <span className="sr-only">Remote path</span>
        <input value={path} onChange={(event) => onPathChange(event.target.value)} />
      </label>
      {status === "loading" ? <div className="sftp-state">Loading remote directory...</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="transfer-box">
        <label>
          <span>Local path</span>
          <input value={localPath} onChange={(event) => onLocalPathChange(event.target.value)} />
        </label>
        <label>
          <span>Remote path</span>
          <input value={transferRemotePath} onChange={(event) => onTransferRemotePathChange(event.target.value)} />
        </label>
        <div className="transfer-actions">
          <button type="button" onClick={() => onTransfer("upload")}>
            Upload
          </button>
          <button type="button" onClick={() => onTransfer("download")}>
            Download
          </button>
        </div>
      </div>
      <div className="file-list">
        {remoteFiles.map((file) => (
          <button
            type="button"
            key={file.id}
            className="file-row"
            onClick={() => {
              if (file.type === "directory") {
                onPathChange(file.path);
                return;
              }

              onOpenFile(file.path);
            }}
          >
            <Folder size={16} aria-hidden="true" />
            <span>
              <strong>{file.name}</strong>
              <small>
                {file.mode} / {file.modifiedAt}
              </small>
            </span>
            <em>{file.type === "directory" ? "-" : `${Math.max(1, Math.round(file.size / 1024))} KB`}</em>
          </button>
        ))}
      </div>
      {transferJobs.length > 0 ? (
        <div className="transfer-list">
          {transferJobs.map((job) => (
            <div key={job.id} className={`transfer-row ${job.status}`}>
              <strong>{job.direction}</strong>
              <span>{job.direction === "upload" ? job.localPath : job.remotePath}</span>
              <small>{job.message ?? job.status}</small>
            </div>
          ))}
        </div>
      ) : null}
    </section>
  );
}

function ZmodemPanel({
  mode,
  message,
  localPath,
  remotePath,
  onLocalPathChange,
  onRemotePathChange,
  onUpload,
  onDownload
}: {
  mode: ZmodemMode;
  message: string;
  localPath: string;
  remotePath: string;
  onLocalPathChange: (path: string) => void;
  onRemotePathChange: (path: string) => void;
  onUpload: () => void;
  onDownload: () => void;
}) {
  return (
    <section className="panel-section" aria-label="ZMODEM transfer">
      <div className="panel-heading">
        <div>
          <UploadCloud size={16} aria-hidden="true" />
          <h2>ZMODEM</h2>
        </div>
        <span className={`zmodem-pill ${mode}`}>{mode}</span>
      </div>
      <div className="zmodem-panel">
        <div className="zmodem-state">{message}</div>
        <input value={localPath} placeholder="Local file path" onChange={(event) => onLocalPathChange(event.target.value)} />
        <input value={remotePath} placeholder="Remote file path" onChange={(event) => onRemotePathChange(event.target.value)} />
        <div className="zmodem-actions">
          <button type="button" onClick={onUpload}>
            Upload
          </button>
          <button type="button" onClick={onDownload}>
            Download
          </button>
        </div>
      </div>
    </section>
  );
}

function RemoteEditorPanel({
  path,
  content,
  status,
  error,
  onContentChange,
  onSave
}: {
  path: string;
  content: string;
  status: "idle" | "loading" | "saving" | "error" | "saved";
  error: string;
  onContentChange: (content: string) => void;
  onSave: () => void;
}) {
  return (
    <section className="panel-section remote-editor" aria-label="Remote file editor">
      <div className="panel-heading">
        <div>
          <Code2 size={16} aria-hidden="true" />
          <h2>Editor</h2>
        </div>
        <button type="button" disabled={!path || status === "loading" || status === "saving"} onClick={onSave}>
          Save
        </button>
      </div>
      <div className="remote-editor-body">
        <div className={`editor-status ${status}`}>
          <span>{path || "No file selected"}</span>
          <small>{error || status}</small>
        </div>
        <textarea
          value={content}
          placeholder="Select a remote file from SFTP"
          disabled={!path || status === "loading"}
          onChange={(event) => onContentChange(event.target.value)}
        />
      </div>
    </section>
  );
}

function MetricsPanel({
  metrics,
  history,
  status,
  error,
  onRefresh
}: {
  metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"];
  history: MetricHistoryPoint[];
  status: "idle" | "loading" | "error";
  error: string;
  onRefresh: () => void;
}) {
  const chartSeries = [
    { label: "CPU", key: "cpu" as const, unit: "%", max: 100 },
    { label: "Memory", key: "memory" as const, unit: "%", max: 100 },
    { label: "Disk", key: "disk" as const, unit: "%", max: 100 },
    { label: "Network", key: "network" as const, unit: "ms", max: 200 },
    { label: "Processes", key: "processes" as const, unit: "", max: Math.max(20, ...history.map((point) => point.processes)) }
  ];

  return (
    <section className="panel-section" aria-label="Server metrics">
      <div className="panel-heading">
        <div>
          <Activity size={16} aria-hidden="true" />
          <h2>Monitor</h2>
        </div>
        <button type="button" aria-label="Refresh metrics" onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      {status === "loading" ? <div className="sftp-state">Collecting remote metrics...</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="metric-grid">
        {metrics.map((metric) => (
          <article key={metric.label} className="metric-tile">
            <span>{metric.label}</span>
            <strong>
              {metric.value}
              {metric.unit}
            </strong>
            <div className={`metric-bar ${metric.trend}`}>
              <span style={{ width: `${Math.min(metric.value, 100)}%` }} />
            </div>
          </article>
        ))}
      </div>
      <div className="monitor-chart-grid">
        {chartSeries.map((series) => (
          <MetricSparkline
            key={series.label}
            label={series.label}
            unit={series.unit}
            max={series.max}
            values={history.map((point) => point[series.key])}
          />
        ))}
      </div>
    </section>
  );
}

function MetricSparkline({ label, unit, max, values }: { label: string; unit: string; max: number; values: number[] }) {
  const points =
    values.length === 0
      ? ""
      : values
          .map((value, index) => {
            const x = values.length === 1 ? 100 : (index / (values.length - 1)) * 100;
            const y = 34 - Math.min(value / max, 1) * 30;
            return `${x.toFixed(1)},${y.toFixed(1)}`;
          })
          .join(" ");
  const latest = values.at(-1) ?? 0;

  return (
    <article className="monitor-chart">
      <div>
        <span>{label}</span>
        <strong>
          {Math.round(latest)}
          {unit}
        </strong>
      </div>
      <svg viewBox="0 0 100 36" preserveAspectRatio="none" aria-hidden="true">
        <polyline points={points} />
      </svg>
    </article>
  );
}

function QuickCommandPanel({
  quickCommands,
  onExecute
}: {
  quickCommands: ReturnType<typeof createInitialAppSnapshot>["quickCommands"];
  onExecute: (command: string) => void;
}) {
  return (
    <section className="panel-section" aria-label="Quick commands">
      <div className="panel-heading">
        <div>
          <Zap size={16} aria-hidden="true" />
          <h2>Quick Commands</h2>
        </div>
        <button type="button" aria-label="Manage quick commands">
          <LayoutDashboard size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="quick-list">
        {quickCommands.map((command) => (
          <button type="button" key={command.id} className="quick-command" onClick={() => onExecute(command.command)}>
            <span>
              <strong>{command.title}</strong>
              <small>{command.command}</small>
            </span>
            <ShieldCheck size={15} aria-label={`${command.scope} scope`} />
          </button>
        ))}
      </div>
    </section>
  );
}

function TriggerPanel({ events }: { events: TriggerEvent[] }) {
  return (
    <section className="panel-section" aria-label="Trigger events">
      <div className="panel-heading">
        <div>
          <Zap size={16} aria-hidden="true" />
          <h2>Triggers</h2>
        </div>
        <span className="poll-rate">{events.length}</span>
      </div>
      <div className="trigger-list">
        {events.length === 0 ? (
          <div className="trigger-empty">No trigger events</div>
        ) : (
          events.map((event) => (
            <div key={event.id} className={`trigger-row ${event.severity}`}>
              <strong>{event.severity}</strong>
              <span>{event.message}</span>
              <small>{event.createdAt}</small>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function ProcessPanel({
  processes,
  status,
  error,
  onRefresh,
  onKill
}: {
  processes: ReturnType<typeof createInitialAppSnapshot>["remoteProcesses"];
  status: "idle" | "loading" | "error";
  error: string;
  onRefresh: () => void;
  onKill: (pid: number) => void;
}) {
  return (
    <section className="panel-section" aria-label="Process manager">
      <div className="panel-heading">
        <div>
          <Activity size={16} aria-hidden="true" />
          <h2>Processes</h2>
        </div>
        <button type="button" aria-label="Refresh processes" onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      {status === "loading" ? <div className="sftp-state">Loading process list...</div> : null}
      {status === "error" ? (
        <div className="sftp-state error" role="alert">
          {error}
        </div>
      ) : null}
      <div className="process-list">
        {processes.length === 0 ? (
          <div className="trigger-empty">No process data</div>
        ) : (
          processes.map((process) => (
            <div key={process.pid} className="process-row">
              <strong>{process.pid}</strong>
              <span title={process.args || process.command}>{process.command}</span>
              <small>{process.cpu.toFixed(1)}%</small>
              <small>{process.memory.toFixed(1)}%</small>
              <button type="button" onClick={() => onKill(process.pid)}>
                Term
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function TunnelPanel({
  draft,
  tunnels,
  onDraftChange,
  onStart,
  onStop
}: {
  draft: TunnelDraft;
  tunnels: TunnelInfo[];
  onDraftChange: (draft: TunnelDraft) => void;
  onStart: () => void;
  onStop: (id: string) => void;
}) {
  const requiresTarget = draft.mode !== "dynamic";

  return (
    <section className="panel-section" aria-label="SSH tunnels">
      <div className="panel-heading">
        <div>
          <Network size={16} aria-hidden="true" />
          <h2>Tunnels</h2>
        </div>
        <button type="button" aria-label="Start tunnel" onClick={onStart}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="tunnel-mode-switch" role="tablist" aria-label="Tunnel mode">
        {tunnelModes.map((mode) => (
          <button
            key={mode.value}
            type="button"
            aria-pressed={draft.mode === mode.value}
            onClick={() => onDraftChange({ ...draft, mode: mode.value })}
          >
            {mode.label}
          </button>
        ))}
      </div>
      <div className="tunnel-form">
        <input
          value={draft.bindHost}
          placeholder={draft.mode === "remote" ? "Remote bind" : "Local bind"}
          onChange={(event) => onDraftChange({ ...draft, bindHost: event.target.value })}
        />
        <input
          value={draft.bindPort}
          placeholder={draft.mode === "remote" ? "Remote port" : "Local port"}
          onChange={(event) => onDraftChange({ ...draft, bindPort: event.target.value })}
        />
        <input
          value={draft.targetHost}
          placeholder={requiresTarget ? "Target host" : "SOCKS target"}
          disabled={!requiresTarget}
          onChange={(event) => onDraftChange({ ...draft, targetHost: event.target.value })}
        />
        <input
          value={draft.targetPort}
          placeholder="Target port"
          disabled={!requiresTarget}
          onChange={(event) => onDraftChange({ ...draft, targetPort: event.target.value })}
        />
      </div>
      <div className="tunnel-list">
        {tunnels.length === 0 ? (
          <div className="trigger-empty">No active tunnels</div>
        ) : (
          tunnels.map((tunnel) => (
            <div key={tunnel.id} className={`tunnel-row ${tunnel.status}`}>
              <strong>{tunnel.mode}</strong>
              <span>{describeTunnel(tunnel)}</span>
              <small>{tunnel.message ?? tunnel.status}</small>
              <button type="button" onClick={() => onStop(tunnel.id)}>
                Stop
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function RelayPanel({
  draft,
  relays,
  onDraftChange,
  onStart,
  onStop
}: {
  draft: { relayHost: string; relayPort: string; targetHost: string; targetPort: string };
  relays: RelayInfo[];
  onDraftChange: (draft: { relayHost: string; relayPort: string; targetHost: string; targetPort: string }) => void;
  onStart: () => void;
  onStop: (id: string) => void;
}) {
  return (
    <section className="panel-section" aria-label="CN Relay">
      <div className="panel-heading">
        <div>
          <Network size={16} aria-hidden="true" />
          <h2>CN Relay</h2>
        </div>
        <button type="button" aria-label="Start relay" onClick={onStart}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="relay-form">
        <input
          value={draft.relayHost}
          placeholder="Relay bind"
          onChange={(event) => onDraftChange({ ...draft, relayHost: event.target.value })}
        />
        <input
          value={draft.relayPort}
          placeholder="Relay port"
          onChange={(event) => onDraftChange({ ...draft, relayPort: event.target.value })}
        />
        <input
          value={draft.targetHost}
          placeholder="Intranet host"
          onChange={(event) => onDraftChange({ ...draft, targetHost: event.target.value })}
        />
        <input
          value={draft.targetPort}
          placeholder="Target port"
          onChange={(event) => onDraftChange({ ...draft, targetPort: event.target.value })}
        />
      </div>
      <div className="relay-list">
        {relays.length === 0 ? (
          <div className="trigger-empty">No relay tunnels</div>
        ) : (
          relays.map((relay) => (
            <div key={relay.id} className={`relay-row ${relay.status}`}>
              <span>{`${relay.relayHost}:${relay.relayPort} -> ${relay.targetHost}:${relay.targetPort}`}</span>
              <small>{relay.message ?? relay.status}</small>
              <button type="button" onClick={() => onStop(relay.id)}>
                Stop
              </button>
            </div>
          ))
        )}
      </div>
    </section>
  );
}

function KeyMappingPanel({
  profiles,
  onChange
}: {
  profiles: KeyMappingProfile[];
  onChange: (profiles: KeyMappingProfile[]) => void;
}) {
  const activeProfile = profiles[0];

  const updateProfile = (patch: Partial<KeyMappingProfile>) => {
    if (!activeProfile) {
      return;
    }

    onChange(profiles.map((profile) => (profile.id === activeProfile.id ? { ...profile, ...patch } : profile)));
  };

  const updateRule = (ruleId: string, patch: Partial<KeyMappingRule>) => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: activeProfile.rules.map((rule) => (rule.id === ruleId ? { ...rule, ...patch } : rule))
    });
  };

  const addRule = () => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: [
        ...activeProfile.rules,
        {
          id: `key-rule-${Date.now()}`,
          key: "Ctrl+K",
          send: "\\r",
          description: "Custom mapping",
          enabled: true
        }
      ]
    });
  };

  const removeRule = (ruleId: string) => {
    if (!activeProfile) {
      return;
    }

    updateProfile({
      rules: activeProfile.rules.filter((rule) => rule.id !== ruleId)
    });
  };

  return (
    <section className="panel-section" aria-label="Key mapping profiles">
      <div className="panel-heading">
        <div>
          <Command size={16} aria-hidden="true" />
          <h2>Key Map</h2>
        </div>
        <button type="button" aria-label="Add key mapping" onClick={addRule}>
          <Plus size={16} aria-hidden="true" />
        </button>
      </div>
      {activeProfile ? (
        <div className="keymap-panel">
          <label className="keymap-profile-toggle">
            <input
              type="checkbox"
              checked={activeProfile.enabled}
              onChange={(event) => updateProfile({ enabled: event.target.checked })}
            />
            <span>{activeProfile.name}</span>
          </label>
          <div className="keymap-list">
            {activeProfile.rules.map((rule) => (
              <div key={rule.id} className="keymap-row">
                <input
                  value={rule.key}
                  aria-label={`${rule.description} shortcut`}
                  onChange={(event) => updateRule(rule.id, { key: event.target.value })}
                />
                <input
                  value={rule.send}
                  aria-label={`${rule.description} send sequence`}
                  onChange={(event) => updateRule(rule.id, { send: event.target.value })}
                />
                <input
                  value={rule.description}
                  aria-label="Key mapping description"
                  onChange={(event) => updateRule(rule.id, { description: event.target.value })}
                />
                <label className="keymap-enabled">
                  <input
                    type="checkbox"
                    checked={rule.enabled}
                    onChange={(event) => updateRule(rule.id, { enabled: event.target.checked })}
                  />
                </label>
                <button type="button" onClick={() => removeRule(rule.id)}>
                  Remove
                </button>
              </div>
            ))}
          </div>
        </div>
      ) : (
        <div className="trigger-empty">No key mapping profile</div>
      )}
    </section>
  );
}

function ScriptRecorderPanel({
  isRecording,
  eventCount,
  recordings,
  onStart,
  onStop,
  onReplay
}: {
  isRecording: boolean;
  eventCount: number;
  recordings: ScriptRecording[];
  onStart: () => void;
  onStop: () => void;
  onReplay: (recording: ScriptRecording) => void;
}) {
  return (
    <section className="panel-section" aria-label="Script recorder">
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>Scripts</h2>
        </div>
        <span className={`recording-pill ${isRecording ? "active" : ""}`}>{isRecording ? "rec" : "idle"}</span>
      </div>
      <div className="script-recorder">
        <div className="script-actions">
          <button type="button" disabled={isRecording} onClick={onStart}>
            Record
          </button>
          <button type="button" disabled={!isRecording} onClick={onStop}>
            Stop
          </button>
          <span>{eventCount} events</span>
        </div>
        <div className="script-list">
          {recordings.length === 0 ? (
            <div className="trigger-empty">No recorded scripts</div>
          ) : (
            recordings.slice(0, 4).map((recording) => (
              <div key={recording.id} className="script-row">
                <div>
                  <strong>{recording.name}</strong>
                  <small>
                    {recording.events.length} events / {new Date(recording.createdAt).toLocaleDateString()}
                  </small>
                </div>
                <button type="button" onClick={() => onReplay(recording)}>
                  Replay
                </button>
              </div>
            ))
          )}
        </div>
      </div>
    </section>
  );
}

function LogViewerPanel({
  query,
  lines,
  status,
  onQueryChange,
  onRefresh
}: {
  query: string;
  lines: string[];
  status: "idle" | "loading" | "error";
  onQueryChange: (query: string) => void;
  onRefresh: () => void;
}) {
  return (
    <section className="panel-section" aria-label="Log viewer">
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>Logs</h2>
        </div>
        <button type="button" aria-label="Refresh session log" onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="log-viewer">
        <input
          value={query}
          placeholder="Filter log lines"
          onChange={(event) => onQueryChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter") {
              onRefresh();
            }
          }}
        />
        <div className={`log-lines ${status}`}>
          {lines.length === 0 ? (
            <div className="trigger-empty">{status === "loading" ? "Loading logs" : "No matching log lines"}</div>
          ) : (
            lines.map((line, index) => (
              <pre key={`${index}-${line.slice(0, 16)}`}>{line || " "}</pre>
            ))
          )}
        </div>
      </div>
    </section>
  );
}

function CloudSyncPanel({
  status,
  onExport,
  onImport
}: {
  status: string;
  onExport: () => void;
  onImport: () => void;
}) {
  return (
    <section className="panel-section" aria-label="Cloud sync">
      <div className="panel-heading">
        <div>
          <ShieldCheck size={16} aria-hidden="true" />
          <h2>Cloud Sync</h2>
        </div>
      </div>
      <div className="cloud-sync-panel">
        <div className="cloud-sync-state">{status}</div>
        <div className="cloud-sync-actions">
          <button type="button" onClick={onExport}>
            Export
          </button>
          <button type="button" onClick={onImport}>
            Import
          </button>
        </div>
      </div>
    </section>
  );
}

function CommandPalette({
  commands,
  query,
  onQueryChange,
  onExecute,
  onClose
}: {
  commands: ReturnType<typeof createInitialAppSnapshot>["quickCommands"];
  query: string;
  onQueryChange: (query: string) => void;
  onExecute: (command: string) => void;
  onClose: () => void;
}) {
  const filteredCommands = commands.filter((command) => {
    const haystack = `${command.title} ${command.command}`.toLowerCase();
    return haystack.includes(query.toLowerCase());
  });

  return (
    <div className="palette-backdrop" role="presentation" onClick={onClose}>
      <section
        className="command-palette"
        role="dialog"
        aria-label="Command palette"
        onClick={(event) => event.stopPropagation()}
      >
        <label className="palette-search">
          <Search size={17} aria-hidden="true" />
          <input
            autoFocus
            value={query}
            placeholder="Search quick commands"
            onChange={(event) => onQueryChange(event.target.value)}
            onKeyDown={(event) => {
              if (event.key === "Escape") {
                onClose();
              }
            }}
          />
        </label>
        <div className="palette-results">
          {filteredCommands.map((command) => (
            <button type="button" key={command.id} onClick={() => onExecute(command.command)}>
              <strong>{command.title}</strong>
              <small>{command.command}</small>
            </button>
          ))}
        </div>
      </section>
    </div>
  );
}
