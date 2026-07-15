import { describe, expect, it, vi } from "vitest";
import type { TerminalSession } from "../types";
import { createWorkspaceSnapshot, saveBeforeWindowClose, saveWorkspaceIfChanged } from "./workspace-persistence";

const terminal = (id: string, sessionType: TerminalSession["sessionType"]): TerminalSession => ({
  id,
  connectionId: `${id}-connection`,
  sessionType,
  title: id,
  status: "online",
  startedAt: "now",
  lastError: null,
});

describe("workspace persistence", () => {
  it("stores restorable SSH sessions without persisting RDP sessions", () => {
    const snapshot = createWorkspaceSnapshot(
      [terminal("ssh", "terminal"), terminal("mosh", "mosh"), terminal("rdp", "rdp")],
      "ssh",
      new Map([["ssh", "/srv/app"]]),
      {
        terminalLayout: null,
        bottomOpen: true,
        bottomHeight: 260,
        connectionsOpen: true,
        monitorOpen: false,
        connectionWidth: 260,
        monitorWidth: 232,
      },
    );

    expect(snapshot.sessions).toEqual([{ id: "ssh", connectionId: "ssh-connection", cwd: "/srv/app" }, { id: "mosh", connectionId: "mosh-connection", cwd: null }]);
  });

  it("waits for the save before destroying the window", async () => {
    const calls: string[] = [];
    await saveBeforeWindowClose(
      async () => { calls.push("save"); },
      async () => { calls.push("destroy"); },
      vi.fn(),
    );
    expect(calls).toEqual(["save", "destroy"]);
  });

  it("still destroys the window when saving fails", async () => {
    const onSaveError = vi.fn();
    const destroy = vi.fn().mockResolvedValue(undefined);
    await saveBeforeWindowClose(
      async () => { throw new Error("disk full"); },
      destroy,
      onSaveError,
    );
    expect(onSaveError).toHaveBeenCalledOnce();
    expect(destroy).toHaveBeenCalledOnce();
  });

  it("skips unchanged automatic saves", async () => {
    const snapshot = createWorkspaceSnapshot([], null, new Map(), {
      terminalLayout: null,
      bottomOpen: true,
      bottomHeight: 260,
      connectionsOpen: true,
      monitorOpen: true,
      connectionWidth: 260,
      monitorWidth: 232,
    });
    const save = vi.fn().mockResolvedValue(undefined);
    const serialized = await saveWorkspaceIfChanged(snapshot, null, save);
    await saveWorkspaceIfChanged(snapshot, serialized, save);

    expect(save).toHaveBeenCalledOnce();
  });
});
