import type { AppSnapshot, ConnectionProfile } from "./models";
import { connectionProfiles, keyMappingProfiles, quickCommands, remoteFiles, serverMetrics, sessionTabs } from "./seed";

export function createInitialAppSnapshot(): AppSnapshot {
  return {
    connections: connectionProfiles,
    sessions: sessionTabs,
    quickCommands,
    keyMappingProfiles,
    remoteFiles,
    serverMetrics
  };
}

export function hydrateAppSnapshot(snapshot: AppSnapshot): AppSnapshot {
  const fallback = createInitialAppSnapshot();
  return {
    ...fallback,
    ...snapshot,
    keyMappingProfiles: snapshot.keyMappingProfiles ?? fallback.keyMappingProfiles
  };
}

export function groupConnections(connections: ConnectionProfile[]) {
  return connections.reduce<Record<string, ConnectionProfile[]>>((groups, connection) => {
    groups[connection.group] = [...(groups[connection.group] ?? []), connection];
    return groups;
  }, {});
}
