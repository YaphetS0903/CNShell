import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "./api";
import { registerEditorGuard } from "./editor-lifecycle";
import { registerWindowCloseProtection } from "./window-close-protection";
import { useAppStore } from "../store/app-store";
import type { TransferTask } from "../types";

const native = vi.hoisted(() => ({
  onCloseRequested: vi.fn(),
}));
vi.mock("@tauri-apps/api/window", () => ({ getCurrentWindow: () => native }));
let close: (event: { preventDefault: () => void }) => Promise<void>;
let requestApplicationExit: () => void;
const cleanups: (() => void)[] = [];

beforeEach(() => {
  vi.restoreAllMocks();
  vi.spyOn(api, "isDesktop").mockReturnValue(true);
  vi.spyOn(window, "confirm").mockReturnValue(false);
  native.onCloseRequested.mockImplementation(async (handler) => {
    close = handler;
    return () => undefined;
  });
  vi.spyOn(api, "exitApplication").mockResolvedValue(undefined);
  vi.spyOn(api, "onApplicationExitRequested").mockImplementation(
    async (handler) => {
      requestApplicationExit = handler;
      return () => undefined;
    },
  );
  useAppStore.setState({ transfers: [] });
});
afterEach(() => cleanups.splice(0).forEach((cleanup) => cleanup()));

it("keeps the native window open when the editor guard blocks closing, even without transfers", async () => {
  let allow = false;
  cleanups.push(
    registerEditorGuard({ canLeave: () => allow, hasPendingWork: () => true }),
  );
  const save = vi.fn().mockResolvedValue(undefined);
  cleanups.push(registerWindowCloseProtection(save, vi.fn()));
  const event = { preventDefault: vi.fn() };
  await close(event);
  expect(event.preventDefault).toHaveBeenCalled();
  expect(save).not.toHaveBeenCalled();
  expect(api.exitApplication).not.toHaveBeenCalled();
  allow = true;
  await close(event);
  expect(save).toHaveBeenCalledOnce();
  expect(api.exitApplication).toHaveBeenCalledOnce();
});

it("warns about hidden active transfers and stops warning once their completion arrives", async () => {
  const running = { id: "background", status: "paused" } as TransferTask;
  useAppStore.setState({ transfers: [running] });
  cleanups.push(
    registerWindowCloseProtection(
      vi.fn().mockResolvedValue(undefined),
      vi.fn(),
    ),
  );
  await close({ preventDefault: vi.fn() });
  expect(window.confirm).toHaveBeenCalledWith(
    expect.stringContaining("1 个传输任务"),
  );
  expect(api.exitApplication).not.toHaveBeenCalled();
  useAppStore.setState({ transfers: [{ ...running, status: "completed" }] });
  vi.mocked(window.confirm).mockClear();
  await close({ preventDefault: vi.fn() });
  expect(window.confirm).not.toHaveBeenCalled();
  expect(api.exitApplication).toHaveBeenCalledOnce();
});

it("uses the browser unload guard for unsaved edits and releases it after cleanup", () => {
  vi.mocked(api.isDesktop).mockReturnValue(false);
  const release = registerEditorGuard({
    canLeave: () => false,
    hasPendingWork: () => true,
  });
  cleanups.push(release);
  const cleanup = registerWindowCloseProtection(vi.fn(), vi.fn());
  const blocked = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(blocked);
  expect(blocked.defaultPrevented).toBe(true);
  cleanup();
  const allowed = new Event("beforeunload", { cancelable: true });
  window.dispatchEvent(allowed);
  expect(allowed.defaultPrevented).toBe(false);
});

it("serializes repeated native close requests while workspace persistence is pending", async () => {
  let finish!: () => void;
  const save = vi.fn().mockReturnValue(
    new Promise<void>((resolve) => {
      finish = resolve;
    }),
  );
  cleanups.push(registerWindowCloseProtection(save, vi.fn()));
  const first = close({ preventDefault: vi.fn() });
  await close({ preventDefault: vi.fn() });
  expect(save).toHaveBeenCalledOnce();
  expect(api.exitApplication).not.toHaveBeenCalled();
  finish();
  await first;
  expect(api.exitApplication).toHaveBeenCalledOnce();
});

it("routes application menu and operating-system exit requests through the editor guard", async () => {
  const canLeave = vi.fn(() => false);
  cleanups.push(
    registerEditorGuard({ canLeave, hasPendingWork: () => true }),
  );
  cleanups.push(registerWindowCloseProtection(vi.fn(), vi.fn()));
  requestApplicationExit();
  await Promise.resolve();
  expect(api.exitApplication).not.toHaveBeenCalled();
  expect(canLeave).toHaveBeenCalledOnce();
});
