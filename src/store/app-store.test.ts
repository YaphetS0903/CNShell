import { beforeEach, describe, expect, it } from "vitest";
import { useAppStore } from "./app-store";
import type { TerminalSession } from "../types";

const session = (id: string): TerminalSession => ({
  id,
  connectionId: "connection",
  sessionType: "rdp",
  title: "Windows",
  status: "online",
  startedAt: "now",
  lastError: null,
});

describe("session status ordering", () => {
  beforeEach(() => useAppStore.setState({ sessions: [], activeSessionId: null }));

  it("applies a status event that arrives before the session is added", () => {
    useAppStore.getState().updateSession("fast-failure", { status: "failed", lastError: "connection refused" });
    useAppStore.getState().addSession(session("fast-failure"));

    expect(useAppStore.getState().sessions[0]).toMatchObject({
      status: "failed",
      lastError: "connection refused",
    });
  });

  it("updates sessions that are already present", () => {
    useAppStore.getState().addSession(session("connected"));
    useAppStore.getState().updateSession("connected", { status: "closed", lastError: null });

    expect(useAppStore.getState().sessions[0].status).toBe("closed");
  });

  it("bounds status events for sessions that never appear", () => {
    for (let index = 0; index < 140; index += 1) {
      useAppStore.getState().updateSession(`missing-${index}`, { status: "closed" });
    }
    useAppStore.getState().addSession(session("missing-0"));
    useAppStore.getState().addSession(session("missing-139"));

    expect(useAppStore.getState().sessions[0].status).toBe("online");
    expect(useAppStore.getState().sessions[1].status).toBe("closed");
  });
});
