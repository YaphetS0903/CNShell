import type { AppSnapshot, ConnectionProfile, SessionTab } from "./models";
import {
  connectionProfiles,
  keyMappingProfiles,
  quickCommands,
  remoteFiles,
  remoteProcesses,
  scriptRecordings,
  serverMetrics,
  systemInfo,
  sessionTabs
} from "./seed";

export function createHomeSessionForConnection(connection: ConnectionProfile): SessionTab {
  return {
    id: `tab-${connection.id}-home`,
    connectionId: connection.id,
    title: connection.name,
    cwd: connection.protocol === "local" ? "~" : "/",
    status: "disconnected",
    startedAt: new Date().toISOString()
  };
}

export function createInitialAppSnapshot(): AppSnapshot {
  return {
    connections: connectionProfiles,
    sessions: sessionTabs,
    quickCommands,
    keyMappingProfiles,
    scriptRecordings,
    remoteFiles,
    serverMetrics,
    systemInfo,
    remoteProcesses
  };
}

export function hydrateAppSnapshot(snapshot: AppSnapshot): AppSnapshot {
  const fallback = createInitialAppSnapshot();
  const connections = snapshot.connections?.length ? snapshot.connections : fallback.connections;
  const connectionIds = new Set(connections.map((connection) => connection.id));
  const persistedSessions = snapshot.sessions === undefined ? fallback.sessions : snapshot.sessions;
  const validSessions = persistedSessions.filter((session) => connectionIds.has(session.connectionId));
  const sessions = validSessions.length ? validSessions : [createHomeSessionForConnection(connections[0])];

  return {
    ...fallback,
    ...snapshot,
    connections,
    sessions,
    quickCommands: snapshot.quickCommands?.length ? snapshot.quickCommands : fallback.quickCommands,
    remoteFiles: snapshot.remoteFiles ?? fallback.remoteFiles,
    serverMetrics: snapshot.serverMetrics ?? fallback.serverMetrics,
    systemInfo: snapshot.systemInfo ?? fallback.systemInfo,
    keyMappingProfiles: snapshot.keyMappingProfiles ?? fallback.keyMappingProfiles,
    scriptRecordings: snapshot.scriptRecordings ?? fallback.scriptRecordings,
    remoteProcesses: snapshot.remoteProcesses ?? fallback.remoteProcesses
  };
}

export function groupConnections(connections: ConnectionProfile[]) {
  return connections.reduce<Record<string, ConnectionProfile[]>>((groups, connection) => {
    groups[connection.group] = [...(groups[connection.group] ?? []), connection];
    return groups;
  }, {});
}
