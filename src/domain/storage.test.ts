import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createInitialAppSnapshot } from "./appState";
import { createLocalWorkspaceStorage } from "./storage";
import type { CNshellApi } from "../shared/ipc";

describe("createLocalWorkspaceStorage", () => {
  beforeEach(() => {
    window.localStorage.clear();
    window.cnshell = undefined;
  });

  afterEach(() => {
    window.localStorage.clear();
    window.cnshell = undefined;
    vi.restoreAllMocks();
  });

  it("saves and loads snapshots from localStorage when Electron API is absent", async () => {
    const storage = createLocalWorkspaceStorage();
    const snapshot = createInitialAppSnapshot();

    await expect(storage.saveSnapshot(snapshot)).resolves.toBe(true);
    await expect(storage.loadSnapshot()).resolves.toEqual(snapshot);
  });

  it("removes invalid localStorage snapshots", async () => {
    window.localStorage.setItem("cnshell.workspace.snapshot.v1", "{bad json");

    const storage = createLocalWorkspaceStorage();

    await expect(storage.loadSnapshot()).resolves.toBeNull();
    expect(window.localStorage.getItem("cnshell.workspace.snapshot.v1")).toBeNull();
  });

  it("delegates load and save to the Electron workspace API when available", async () => {
    const snapshot = createInitialAppSnapshot();
    const load = vi.fn().mockResolvedValue(snapshot);
    const save = vi.fn().mockResolvedValue(true);
    window.cnshell = {
      workspace: { load, save }
    } as unknown as CNshellApi;

    const storage = createLocalWorkspaceStorage();

    await expect(storage.loadSnapshot()).resolves.toEqual(snapshot);
    await expect(storage.saveSnapshot(snapshot)).resolves.toBe(true);
    expect(load).toHaveBeenCalledOnce();
    expect(save).toHaveBeenCalledWith(snapshot);
  });
});
