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
import { Terminal } from "@xterm/xterm";
import { createInitialAppSnapshot, groupConnections } from "../domain/appState";
import { createLocalWorkspaceStorage } from "../domain/storage";
import type { ConnectionProfile, SessionStatus, SessionTab } from "../domain/models";
import type { HostKeyVerificationEvent } from "../shared/ipc";
import { terminalTheme } from "./terminalTheme";

const workspaceStorage = createLocalWorkspaceStorage();

export function App() {
  const [snapshot] = useState(() => workspaceStorage.loadSnapshot() ?? createInitialAppSnapshot());
  const [activeConnectionId, setActiveConnectionId] = useState(snapshot.connections[0].id);
  const [activeTabId, setActiveTabId] = useState(snapshot.sessions[0].id);
  const [appVersion, setAppVersion] = useState("dev");
  const [sshDrafts, setSshDrafts] = useState<Record<string, { password: string; privateKey: string; passphrase: string }>>({});
  const [sessionStartTokens, setSessionStartTokens] = useState<Record<string, number>>({});
  const [hostKeyPrompts, setHostKeyPrompts] = useState<Record<string, HostKeyVerificationEvent>>({});
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

  useEffect(() => {
    void window.cnshell?.getVersion().then(setAppVersion);
  }, []);

  useEffect(() => {
    workspaceStorage.saveSnapshot(snapshot);
  }, [snapshot]);

  useEffect(() => {
    return window.cnshell?.terminal.onHostKeyVerification((event) => {
      setHostKeyPrompts((current) => ({
        ...current,
        [event.id]: event
      }));
      setSessionStatus(event.id, "error");
    });
  }, [setSessionStatus]);

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
        <TopBar activeConnection={activeConnection} status={activeTab.status} version={appVersion} />
        <TabStrip tabs={sessionTabsWithStatus} activeTabId={activeTabId} onSelect={setActiveTabId} />
        <section className="workspace-grid">
          <TerminalPane
            activeConnection={activeConnection}
            activeTab={activeTab}
            sshDraft={activeSshDraft}
            startToken={sessionStartTokens[activeTab.id] ?? 0}
            onStatusChange={setSessionStatus}
          />
          <aside className="ops-panel" aria-label="Operations panels">
            {activeConnection.protocol === "ssh" ? (
              <SshCredentialPanel
                authMethod={activeConnection.authMethod}
                draft={activeSshDraft}
                hostKeyPrompt={hostKeyPrompts[activeTab.id]}
                onChange={updateActiveSshDraft}
                onConnect={startActiveSession}
                onTrustHost={trustActiveHost}
              />
            ) : null}
            <FilePanel remoteFiles={snapshot.remoteFiles} />
            <MetricsPanel metrics={snapshot.serverMetrics} />
            <QuickCommandPanel quickCommands={snapshot.quickCommands} />
          </aside>
        </section>
      </section>
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
  version
}: {
  activeConnection: ConnectionProfile;
  status: SessionStatus;
  version: string;
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
        <button type="button" aria-label="Open command palette">
          <Command size={17} aria-hidden="true" />
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
  onSelect
}: {
  tabs: SessionTab[];
  activeTabId: string;
  onSelect: (tabId: string) => void;
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
      <button type="button" className="new-tab" aria-label="Open new session tab">
        <Plus size={16} aria-hidden="true" />
      </button>
    </div>
  );
}

function TerminalPane({
  activeConnection,
  activeTab,
  sshDraft,
  startToken,
  onStatusChange
}: {
  activeConnection: ConnectionProfile;
  activeTab: SessionTab;
  sshDraft: { password: string; privateKey: string; passphrase: string };
  startToken: number;
  onStatusChange: (sessionId: string, status: SessionStatus) => void;
}) {
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
    terminal.loadAddon(fitAddon);
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
                  host: activeConnection.host,
                  port: activeConnection.port,
                  username: activeConnection.username,
                  password: sshDraft.password || undefined,
                  privateKey: sshDraft.privateKey || undefined,
                  passphrase: sshDraft.passphrase || undefined
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
      terminal.dispose();
    };
  }, [
    activeConnection,
    activeTab,
    onStatusChange,
    sshDraft.password,
    sshDraft.passphrase,
    sshDraft.privateKey,
    startToken
  ]);

  return (
    <section className="terminal-workbench" aria-label="Terminal workbench">
      <div className="terminal-toolbar">
        <div className="breadcrumb">
          <HardDrive size={16} aria-hidden="true" />
          <span>{activeTab.cwd}</span>
        </div>
        <div className="terminal-tools">
          <button type="button">
            <SplitSquareHorizontal size={16} aria-hidden="true" />
            Split
          </button>
          <button type="button">
            <UploadCloud size={16} aria-hidden="true" />
            ZMODEM
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
        <input placeholder="Draft a command before sending to one or many sessions" />
        <button type="button">Send</button>
      </div>
    </section>
  );
}

function SshCredentialPanel({
  authMethod,
  draft,
  hostKeyPrompt,
  onChange,
  onConnect,
  onTrustHost
}: {
  authMethod: ConnectionProfile["authMethod"];
  draft: { password: string; privateKey: string; passphrase: string };
  hostKeyPrompt?: HostKeyVerificationEvent;
  onChange: (field: "password" | "privateKey" | "passphrase", value: string) => void;
  onConnect: () => void;
  onTrustHost: () => void;
}) {
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
      </div>
    </section>
  );
}

function FilePanel({ remoteFiles }: { remoteFiles: ReturnType<typeof createInitialAppSnapshot>["remoteFiles"] }) {
  return (
    <section className="panel-section file-panel" aria-label="Remote files">
      <div className="panel-heading">
        <div>
          <FileText size={16} aria-hidden="true" />
          <h2>SFTP</h2>
        </div>
        <button type="button" aria-label="Upload file">
          <UploadCloud size={16} aria-hidden="true" />
        </button>
      </div>
      <div className="path-row">/var/www/cnshell</div>
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
    </section>
  );
}

function MetricsPanel({ metrics }: { metrics: ReturnType<typeof createInitialAppSnapshot>["serverMetrics"] }) {
  return (
    <section className="panel-section" aria-label="Server metrics">
      <div className="panel-heading">
        <div>
          <Activity size={16} aria-hidden="true" />
          <h2>Monitor</h2>
        </div>
        <span className="poll-rate">5s</span>
      </div>
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
  quickCommands
}: {
  quickCommands: ReturnType<typeof createInitialAppSnapshot>["quickCommands"];
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
          <button type="button" key={command.id} className="quick-command">
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
