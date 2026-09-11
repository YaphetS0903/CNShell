import { act, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import { api } from "./api";
import {
  hasActiveTransfers,
  startTransferSync,
  useTransferSync,
} from "./transfer-sync";
import { useAppStore } from "../store/app-store";
import { TransferQueue } from "../features/files/TransferQueue";
import type { TransferTask } from "../types";

const task = (
  id: string,
  status: TransferTask["status"] = "running",
  bytes = 0,
): TransferTask => ({
  id,
  status,
  transferredBytes: bytes,
  totalBytes: 1_000,
  sessionId: "ssh",
  direction: "download",
  source: `/${id}.txt`,
  destination: `/local/${id}.txt`,
  conflictPolicy: "overwrite",
  error: null,
  createdAt: "now",
});
const tick = async () => {
  await Promise.resolve();
  await Promise.resolve();
};
let receive: (task: TransferTask) => void;
const stop = vi.fn();
const cleanups: (() => void)[] = [];

beforeEach(() => {
  vi.restoreAllMocks();
  stop.mockReset();
  useAppStore.setState({ transfers: [], transferMetrics: {}, error: null });
  vi.spyOn(api, "onTransfer").mockImplementation(async (handler) => {
    receive = handler;
    return stop;
  });
  vi.spyOn(api, "listTransfers").mockResolvedValue([]);
});
afterEach(() => {
  cleanups.splice(0).forEach((cleanup) => cleanup());
  vi.useRealTimers();
});

it("subscribes before loading history and retains events and enqueue results newer than the snapshot", async () => {
  let finish!: (tasks: TransferTask[]) => void;
  vi.mocked(api.listTransfers).mockImplementation(() => {
    expect(api.onTransfer).toHaveBeenCalledOnce();
    return new Promise((resolve) => {
      finish = resolve;
    });
  });
  cleanups.push(startTransferSync());
  await tick();
  receive(task("first", "completed", 1_000));
  useAppStore.getState().addTransfer(task("second", "queued"));
  finish([task("first", "queued"), task("old", "completed", 1_000)]);
  await tick();
  expect(
    useAppStore.getState().transfers.map(({ id, status }) => [id, status]),
  ).toEqual([
    ["first", "completed"],
    ["old", "completed"],
    ["second", "queued"],
  ]);
  expect(hasActiveTransfers()).toBe(true);
  receive(task("second", "cancelled"));
  expect(hasActiveTransfers()).toBe(false);
});

it("continues updating status and speed while the panel is hidden without re-subscribing on reopen", async () => {
  function Harness({ visible }: { visible: boolean }) {
    useTransferSync();
    return visible ? <TransferQueue /> : null;
  }
  let now = 0;
  vi.spyOn(performance, "now").mockImplementation(() => now);
  const view = render(<Harness visible={false} />);
  await act(tick);
  act(() => receive(task("file", "running", 100)));
  now = 1_000;
  act(() => receive(task("file", "running", 300)));
  view.rerender(<Harness visible />);
  expect(screen.getByText(/200 B\/s/)).toBeInTheDocument();
  view.rerender(<Harness visible={false} />);
  act(() => receive(task("file", "completed", 1_000)));
  expect(hasActiveTransfers()).toBe(false);
  view.rerender(<Harness visible />);
  expect(screen.getByText(/1000 B \/ 1000 B · 已完成/)).toBeInTheDocument();
  expect(api.onTransfer).toHaveBeenCalledOnce();
  expect(api.listTransfers).toHaveBeenCalledOnce();
  view.unmount();
  expect(stop).toHaveBeenCalledOnce();
});

it("keeps task order stable and ignores a queued response arriving after completion", async () => {
  cleanups.push(startTransferSync());
  await tick();
  receive(task("first"));
  receive(task("second"));
  receive(task("first", "completed", 1_000));
  useAppStore.getState().addTransfer(task("first", "queued"));
  expect(
    useAppStore.getState().transfers.map(({ id, status }) => [id, status]),
  ).toEqual([
    ["first", "completed"],
    ["second", "running"],
  ]);
});

it("ignores stale events and snapshots after unmount", async () => {
  let finish!: (tasks: TransferTask[]) => void;
  vi.mocked(api.listTransfers).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const cleanup = startTransferSync();
  await tick();
  cleanup();
  finish([task("old")]);
  receive(task("late"));
  await tick();
  expect(useAppStore.getState().transfers).toEqual([]);
  expect(stop).toHaveBeenCalledOnce();
});

it("unsubscribes a delayed listener registration after unmount without fetching history", async () => {
  let finish!: (stop: () => void) => void;
  vi.mocked(api.onTransfer).mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  const cleanup = startTransferSync();
  cleanup();
  finish(stop);
  await tick();
  expect(stop).toHaveBeenCalledOnce();
  expect(api.listTransfers).not.toHaveBeenCalled();
});

it.each(["subscription", "snapshot"])(
  "recovers from a failed %s without duplicating active listeners",
  async (failure) => {
    vi.useFakeTimers();
    if (failure === "subscription")
      vi.mocked(api.onTransfer).mockRejectedValueOnce(new Error("offline"));
    else
      vi.mocked(api.listTransfers).mockRejectedValueOnce(new Error("offline"));
    cleanups.push(startTransferSync());
    await tick();
    expect(useAppStore.getState().error).toContain("正在重试");
    await vi.advanceTimersByTimeAsync(1_000);
    receive(task("recovered", "completed", 1_000));
    expect(useAppStore.getState().transfers[0].status).toBe("completed");
    expect(api.onTransfer).toHaveBeenCalledTimes(
      failure === "subscription" ? 2 : 1,
    );
    expect(api.listTransfers).toHaveBeenCalledTimes(
      failure === "snapshot" ? 2 : 1,
    );
  },
);
