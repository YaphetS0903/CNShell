import type { AppSnapshot, ConnectionProfile } from "./models";
import {
  connectionProfiles,
  keyMappingProfiles,
  quickCommands,
  remoteFiles,
  remoteProcesses,
  scriptRecordings,
  serverMetrics,
  sessionTabs
} from "./seed";

export function createInitialAppSnapshot(): AppSnapshot {
  return {
    connections: connectionProfiles,
    sessions: sessionTabs,
    quickCommands,
    keyMappingProfiles,
    scriptRecordings,
    remoteFiles,
    serverMetrics,
    remoteProcesses
  };
}

export function hydrateAppSnapshot(snapshot: AppSnapshot): AppSnapshot {
  const fallback = createInitialAppSnapshot();
  const connections = snapshot.connections?.length ? snapshot.connections : fallback.connections;
  const connectionIds = new Set(connections.map((connection) => connection.id));
  const sessions = (snapshot.sessions ?? fallback.sessions).filter((session) => connectionIds.has(session.connectionId));

  return {
    ...fallback,
    ...snapshot,
    connections,
    sessions,
    quickCommands: snapshot.quickCommands?.length ? snapshot.quickCommands : fallback.quickCommands,
    remoteFiles: snapshot.remoteFiles ?? fallback.remoteFiles,
    serverMetrics: snapshot.serverMetrics ?? fallback.serverMetrics,
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
