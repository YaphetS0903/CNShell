import { beforeEach, describe, expect, it, vi } from "vitest";
import { useAppStore } from "./app-store";
import type { TerminalSession } from "../types";
import { defaultSettings } from "../types";
import { api } from "../lib/api";

describe("settings save ordering", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAppStore.setState({ settings: defaultSettings });
  });

  it.each([true, false])(
    "preserves newer settings after an older request fails (latest succeeds: %s)",
    async (latestSucceeds) => {
      let rejectOlder!: (reason: unknown) => void;
      const older = new Promise<typeof defaultSettings>((_, reject) => {
        rejectOlder = reject;
      });
      const save = vi.spyOn(api, "saveSettings").mockReturnValueOnce(older);
      if (latestSucceeds)
        save.mockResolvedValueOnce({ ...defaultSettings, theme: "light" });
      else save.mockRejectedValueOnce(new Error("latest failed"));
      const first = useAppStore
        .getState()
        .saveSettings({ ...defaultSettings, theme: "dark" })
        .catch(() => undefined);
      const second = useAppStore
        .getState()
        .saveSettings({ ...defaultSettings, theme: "light" })
        .catch(() => undefined);
      await Promise.resolve();
      expect(save).toHaveBeenCalledTimes(1);
      expect(useAppStore.getState().settings.theme).toBe("light");
      rejectOlder(new Error("older failed"));
      await Promise.all([first, second]);
      expect(save).toHaveBeenCalledTimes(2);
      expect(useAppStore.getState().settings.theme).toBe(
        latestSucceeds ? "light" : defaultSettings.theme,
      );
    },
  );

  it("rolls back to the last successful save instead of a failed optimistic state", async () => {
    const save = vi
      .spyOn(api, "saveSettings")
      .mockResolvedValueOnce({ ...defaultSettings, theme: "dark" })
      .mockRejectedValueOnce(new Error("write failed"));
    const first = useAppStore
      .getState()
      .saveSettings({ ...defaultSettings, theme: "dark" });
    const second = useAppStore
      .getState()
      .saveSettings({ ...defaultSettings, theme: "light" })
      .catch(() => undefined);
    await Promise.all([first, second]);
    expect(save.mock.calls.map(([settings]) => settings.theme)).toEqual([
      "dark",
      "light",
    ]);
    expect(useAppStore.getState().settings.theme).toBe("dark");
  });
});

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
  beforeEach(() =>
    useAppStore.setState({ sessions: [], activeSessionId: null }),
  );

  it("applies a status event that arrives before the session is added", () => {
    useAppStore
      .getState()
      .updateSession("fast-failure", {
        status: "failed",
        lastError: "connection refused",
      });
    useAppStore.getState().addSession(session("fast-failure"));

    expect(useAppStore.getState().sessions[0]).toMatchObject({
      status: "failed",
      lastError: "connection refused",
    });
  });

  it("updates sessions that are already present", () => {
    useAppStore.getState().addSession(session("connected"));
    useAppStore
      .getState()
      .updateSession("connected", { status: "closed", lastError: null });

    expect(useAppStore.getState().sessions[0].status).toBe("closed");
  });

  it("bounds status events for sessions that never appear", () => {
    for (let index = 0; index < 140; index += 1) {
      useAppStore
        .getState()
        .updateSession(`missing-${index}`, { status: "closed" });
    }
    useAppStore.getState().addSession(session("missing-0"));
    useAppStore.getState().addSession(session("missing-139"));

    expect(useAppStore.getState().sessions[0].status).toBe("online");
    expect(useAppStore.getState().sessions[1].status).toBe("closed");
  });
});
