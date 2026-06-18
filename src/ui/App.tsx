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
import { createInitialAppSnapshot, groupConnections } from "../domain/appState";
import { createLocalWorkspaceStorage } from "../domain/storage";
import type { ConnectionProfile, SessionStatus, SessionTab, TransferJob } from "../domain/models";
import type { CredentialStatus, HostKeyVerificationEvent } from "../shared/ipc";
import { terminalTheme } from "./terminalTheme";

const workspaceStorage = createLocalWorkspaceStorage();

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
  const [remoteFileEntries, setRemoteFileEntries] = useState(snapshot.remoteFiles);
  const [remotePath, setRemotePath] = useState("/var/www/cnshell");
  const [sftpStatus, setSftpStatus] = useState<"idle" | "loading" | "error">("idle");
  const [sftpError, setSftpError] = useState("");
  const [liveMetrics, setLiveMetrics] = useState(snapshot.serverMetrics);
  const [metricsStatus, setMetricsStatus] = useState<"idle" | "loading" | "error">("idle");
  const [metricsError, setMetricsError] = useState("");
  const [transferLocalPath, setTransferLocalPath] = useState("");
  const [transferRemotePath, setTransferRemotePath] = useState("");
  const [transferJobs, setTransferJobs] = useState<TransferJob[]>([]);
  const [isCommandPaletteOpen, setIsCommandPaletteOpen] = useState(false);
  const [commandQuery, setCommandQuery] = useState("");
  const [isSyncInputEnabled, setIsSyncInputEnabled] = useState(false);
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
        ssh: {
          connectionId: activeConnection.id,
          host: activeConnection.host,
          port: activeConnection.port,
          username: activeConnection.username,
          password: activeSshDraft.password || undefined,
          privateKey: activeSshDraft.privateKey || undefined,
          passphrase: activeSshDraft.passphrase || undefined,
          useSavedCredential: Boolean(activeCredentialStatus?.hasCredential)
        }
      })
      .then((listing) => {
        setRemotePath(listing.path);
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
        ssh: {
          connectionId: activeConnection.id,
          host: activeConnection.host,
          port: activeConnection.port,
          username: activeConnection.username,
          password: activeSshDraft.password || undefined,
          privateKey: activeSshDraft.privateKey || undefined,
          passphrase: activeSshDraft.passphrase || undefined,
          useSavedCredential: Boolean(activeCredentialStatus?.hasCredential)
        }
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

  const refreshMetrics = () => {
    if (activeConnection.protocol !== "ssh") {
      setLiveMetrics(snapshot.serverMetrics);
      return;
    }

    setMetricsStatus("loading");
    setMetricsError("");

    void window.cnshell?.metrics
      .collect({
        ssh: {
          connectionId: activeConnection.id,
          host: activeConnection.host,
          port: activeConnection.port,
          username: activeConnection.username,
          password: activeSshDraft.password || undefined,
          privateKey: activeSshDraft.privateKey || undefined,
          passphrase: activeSshDraft.passphrase || undefined,
          useSavedCredential: Boolean(activeCredentialStatus?.hasCredential)
        }
      })
      .then((result) => {
        setLiveMetrics(result.metrics);
        setMetricsStatus("idle");
      })
      .catch((error: Error) => {
        setMetricsError(error.message);
        setMetricsStatus("error");
      });
  };

  const executeCommand = (command: string) => {
    const targetSessionIds = isSyncInputEnabled
      ? sessionTabsWithStatus.filter((session) => session.status !== "error").map((session) => session.id)
      : [activeTab.id];

    for (const sessionId of targetSessionIds) {
      void window.cnshell?.terminal.write(sessionId, `${command}\r`);
    }

    setIsCommandPaletteOpen(false);
    setCommandQuery("");
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
        setSnapshot(storedSnapshot);
        setRemoteFileEntries(storedSnapshot.remoteFiles);
        setLiveMetrics(storedSnapshot.serverMetrics);
        setActiveConnectionId(storedSnapshot.connections[0]?.id ?? "");
        setActiveTabId(storedSnapshot.sessions[0]?.id ?? "");
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
          onOpenCommandPalette={() => setIsCommandPaletteOpen(true)}
          onToggleSyncInput={() => setIsSyncInputEnabled((current) => !current)}
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
            startToken={sessionStartTokens[activeTab.id] ?? 0}
            onStatusChange={setSessionStatus}
            onReconnect={startActiveSession}
            onDispatchCommand={executeCommand}
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
            <FilePanel
              remoteFiles={remoteFileEntries}
              path={remotePath}
              status={sftpStatus}
              error={sftpError}
              localPath={transferLocalPath}
              transferRemotePath={transferRemotePath}
              transferJobs={transferJobs}
              onPathChange={setRemotePath}
              onLocalPathChange={setTransferLocalPath}
              onTransferRemotePathChange={setTransferRemotePath}
              onRefresh={refreshRemoteFiles}
              onTransfer={startTransfer}
            />
            <MetricsPanel metrics={liveMetrics} status={metricsStatus} error={metricsError} onRefresh={refreshMetrics} />
            <QuickCommandPanel quickCommands={snapshot.quickCommands} onExecute={executeCommand} />
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
  onOpenCommandPalette,
  onToggleSyncInput
}: {
  activeConnection: ConnectionProfile;
  status: SessionStatus;
  version: string;
  isSyncInputEnabled: boolean;
  onOpenCommandPalette: () => void;
  onToggleSyncInput: () => void;
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
  startToken,
  onStatusChange,
  onReconnect,
  onDispatchCommand
}: {
  activeConnection: ConnectionProfile;
  activeTab: SessionTab;
  sshDraft: { password: string; privateKey: string; passphrase: string };
  useSavedCredential: boolean;
  startToken: number;
  onStatusChange: (sessionId: string, status: SessionStatus) => void;
  onReconnect: () => void;
  onDispatchCommand: (command: string) => void;
}) {
  const [composeValue, setComposeValue] = useState("");
  const [terminalSearch, setTerminalSearch] = useState("");
  const [searchAddon, setSearchAddon] = useState<SearchAddon | null>(null);

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
    terminal.writeln("\x1b[1;32mCNshell terminal session starting\x1b[0m");
    terminal.writeln(`Profile: ${activeConnection.username}@${host}`);
    terminal.writeln("");

    const sessionId = activeTab.id;
    const removeDataListener = window.cnshell?.terminal.onData(({ id, data }) => {
      if (id === sessionId) {
        terminal.write(data);
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
      void window.cnshell?.terminal.write(sessionId, data);
    });

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
              ? {
                  connectionId: activeConnection.id,
                  host: activeConnection.host,
                  port: activeConnection.port,
                  username: activeConnection.username,
                  password: sshDraft.password || undefined,
                  privateKey: sshDraft.privateKey || undefined,
                  passphrase: sshDraft.passphrase || undefined,
                  useSavedCredential
                }
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
      startTerminalSession();
    }

    const resizeObserver = new ResizeObserver(resizeSession);
    resizeObserver.observe(terminalHost);

    return () => {
      resizeObserver.disconnect();
      dataDisposable.dispose();
      removeDataListener?.();
      removeExitListener?.();
      removeErrorListener?.();
      void window.cnshell?.terminal.stop(sessionId);
      onStatusChange(sessionId, "disconnected");
      setSearchAddon(null);
      terminal.dispose();
    };
  }, [
    activeConnection,
    activeTab,
    onStatusChange,
    sshDraft.password,
    sshDraft.passphrase,
    sshDraft.privateKey,
    useSavedCredential,
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
          <button type="button">
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
  onTransfer
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
}) {
  return (
    <section className="panel-section file-panel" aria-label="Remote files">
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>SFTP</h2>
        </div>
        <button type="button" aria-label="Refresh remote files" onClick={onRefresh}>
          <RefreshCw size={16} aria-hidden="true" />
        </button>
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
          <button type="button" key={file.id} className="file-row">
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

function MetricsPanel({
  metrics,
  status,
  error,
  onRefresh
}: {
  metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"];
  status: "idle" | "loading" | "error";
  error: string;
  onRefresh: () => void;
}) {
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
    </section>
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
