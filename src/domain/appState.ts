import type { AppSnapshot, ConnectionProfile } from "./models";
import { connectionProfiles, quickCommands, remoteFiles, serverMetrics, sessionTabs } from "./seed";

export function createInitialAppSnapshot(): AppSnapshot {
  return {
    connections: connectionProfiles,
    sessions: sessionTabs,
    quickCommands,
    remoteFiles,
    serverMetrics
  };
}

export function groupConnections(connections: ConnectionProfile[]) {
  return connections.reduce<Record<string, ConnectionProfile[]>>((groups, connection) => {
    groups[connection.group] = [...(groups[connection.group] ?? []), connection];
    return groups;
  }, {});
}
