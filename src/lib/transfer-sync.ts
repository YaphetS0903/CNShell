import { useEffect } from "react";
import { api } from "./api";
import { useAppStore } from "../store/app-store";
import type { TransferTask } from "../types";
import { errorMessage } from "./format";

export const hasActiveTransfers = () =>
  useAppStore
    .getState()
    .transfers.some((task) =>
      ["queued", "running", "paused"].includes(task.status),
    );

export function startTransferSync() {
  let disposed = false;
  let unlisten: (() => void) | undefined;
  let retry: ReturnType<typeof setTimeout> | undefined;
  let retryDelay = 1_000;
  let initialized = false;
  const received = new Map<string, TransferTask>();
  const initial = new Map(
    useAppStore.getState().transfers.map((task) => [task.id, task]),
  );

  const synchronize = async () => {
    try {
      // Register first so the initial database snapshot cannot hide live events.
      if (!unlisten) {
        const stop = await api.onTransfer((task) => {
          if (disposed) return;
          if (!initialized) received.set(task.id, task);
          useAppStore.getState().upsertTransfer(task);
        });
        if (disposed) {
          stop();
          return;
        }
        unlisten = stop;
      }
      const tasks = await api.listTransfers();
      if (disposed) return;
      const latest = new Map(tasks.map((task) => [task.id, task]));
      for (const task of useAppStore.getState().transfers) {
        if (received.has(task.id) || task !== initial.get(task.id))
          latest.set(task.id, task);
      }
      useAppStore.getState().setTransfers([...latest.values()]);
      initialized = true;
      received.clear();
    } catch (reason) {
      if (disposed) return;
      useAppStore
        .getState()
        .setError(`传输状态同步失败，正在重试：${errorMessage(reason)}`);
      retry = setTimeout(() => void synchronize(), retryDelay);
      retryDelay = Math.min(30_000, retryDelay * 2);
    }
  };
  void synchronize();
  return () => {
    disposed = true;
    clearTimeout(retry);
    unlisten?.();
  };
}

export function useTransferSync() {
  useEffect(startTransferSync, []);
}
