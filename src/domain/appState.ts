import type { AppSnapshot, ConnectionProfile } from "./models";
import {
  connectionProfiles,
  keyMappingProfiles,
  quickCommands,
  remoteFiles,
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
    serverMetrics
  };
}

export function hydrateAppSnapshot(snapshot: AppSnapshot): AppSnapshot {
  const fallback = createInitialAppSnapshot();
  return {
    ...fallback,
    ...snapshot,
    keyMappingProfiles: snapshot.keyMappingProfiles ?? fallback.keyMappingProfiles,
    scriptRecordings: snapshot.scriptRecordings ?? fallback.scriptRecordings
  };
}

export function groupConnections(connections: ConnectionProfile[]) {
  return connections.reduce<Record<string, ConnectionProfile[]>>((groups, connection) => {
    groups[connection.group] = [...(groups[connection.group] ?? []), connection];
    return groups;
  }, {});
}
