import { describe, expect, it } from "vitest";
import { createInitialAppSnapshot, groupConnections, hydrateAppSnapshot } from "./appState";
import type { AppSnapshot } from "./models";

describe("appState", () => {
  it("creates a complete initial workspace snapshot", () => {
    const snapshot = createInitialAppSnapshot();

    expect(snapshot.connections.length).toBeGreaterThan(0);
    expect(snapshot.sessions.length).toBeGreaterThan(0);
    expect(snapshot.quickCommands.length).toBeGreaterThan(0);
    expect(snapshot.keyMappingProfiles.length).toBeGreaterThan(0);
    expect(snapshot.remoteFiles.length).toBeGreaterThan(0);
    expect(snapshot.serverMetrics.length).toBeGreaterThan(0);
    expect(snapshot.systemInfo.filesystems.length).toBeGreaterThan(0);
    expect(snapshot.remoteProcesses).toEqual([]);
  });

  it("hydrates missing modern collections from defaults", () => {
    const fallback = createInitialAppSnapshot();
    const legacySnapshot = {
      connections: fallback.connections.slice(0, 1),
      sessions: fallback.sessions.slice(0, 1),
      quickCommands: fallback.quickCommands,
      remoteFiles: fallback.remoteFiles,
      serverMetrics: fallback.serverMetrics
    } as AppSnapshot;

    const hydrated = hydrateAppSnapshot(legacySnapshot);

    expect(hydrated.connections).toHaveLength(1);
    expect(hydrated.keyMappingProfiles).toEqual(fallback.keyMappingProfiles);
    expect(hydrated.scriptRecordings).toEqual(fallback.scriptRecordings);
    expect(hydrated.systemInfo).toEqual(fallback.systemInfo);
    expect(hydrated.remoteProcesses).toEqual(fallback.remoteProcesses);
  });

  it("restores a home session when persisted sessions are empty", () => {
    const fallback = createInitialAppSnapshot();
    const hydrated = hydrateAppSnapshot({
      ...fallback,
      sessions: []
    });

    expect(hydrated.sessions).toHaveLength(1);
    expect(hydrated.sessions[0]).toEqual(expect.objectContaining({
      connectionId: hydrated.connections[0].id,
      status: "disconnected"
    }));
  });

  it("recovers from empty or mismatched persisted workspace state", () => {
    const fallback = createInitialAppSnapshot();
    const brokenSnapshot = {
      ...fallback,
      connections: [],
      sessions: [
        {
          id: "tab-missing",
          connectionId: "missing-connection",
          title: "missing",
          cwd: "/",
          status: "disconnected",
          startedAt: new Date().toISOString()
        }
      ]
    } as AppSnapshot;

    const hydrated = hydrateAppSnapshot(brokenSnapshot);

    expect(hydrated.connections.length).toBeGreaterThan(0);
    expect(hydrated.sessions.every((session) => hydrated.connections.some((connection) => connection.id === session.connectionId))).toBe(true);
  });

  it("groups connections by their declared group", () => {
    const snapshot = createInitialAppSnapshot();
    const grouped = groupConnections(snapshot.connections);

    expect(Object.keys(grouped)).toContain("Production");
    expect(grouped.Production.some((connection) => connection.id === "prod-web-01")).toBe(true);
  });
});
