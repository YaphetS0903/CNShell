import { getCurrentWindow } from "@tauri-apps/api/window";
import { api } from "./api";
import { canLeaveEditor, editorHasPendingWork } from "./editor-lifecycle";
import { hasActiveTransfers } from "./transfer-sync";
import { useAppStore } from "../store/app-store";
import { saveBeforeWindowClose } from "./workspace-persistence";
import { errorMessage } from "./format";

export function registerWindowCloseProtection(
  saveWorkspace: () => Promise<void>,
  onError: (message: string) => void,
) {
  let closing = false;
  let disposed = false;
  let closeUnlisten: (() => void) | undefined;
  let exitUnlisten: (() => void) | undefined;
  const beforeUnload = (event: BeforeUnloadEvent) => {
    if (closing || (!editorHasPendingWork() && !hasActiveTransfers())) return;
    event.preventDefault();
    event.returnValue = "";
  };
  window.addEventListener("beforeunload", beforeUnload);
  if (api.isDesktop()) {
    const requestClose = async () => {
      if (closing || disposed) return;
      const activeTransfers = useAppStore
        .getState()
        .transfers.filter((task) =>
          ["queued", "running", "paused"].includes(task.status),
        );
      if (
        activeTransfers.length &&
        !confirm(
          `仍有 ${activeTransfers.length} 个传输任务未完成。关闭 CNshell 会中断这些任务，确定继续吗？`,
        )
      )
        return;
      if (!canLeaveEditor()) return;
      closing = true;
      try {
        await saveBeforeWindowClose(
          saveWorkspace,
          () => api.exitApplication(),
          (reason) => console.error("工作区状态保存失败", reason),
        );
      } catch (reason) {
        closing = false;
        onError(`关闭窗口失败：${errorMessage(reason)}`);
      }
    };
    void getCurrentWindow()
      .onCloseRequested((event) => {
        event.preventDefault();
        void requestClose();
      })
      .then((stop) => {
        if (disposed) stop();
        else closeUnlisten = stop;
      })
      .catch((reason) => {
        if (!disposed) onError(`注册窗口关闭保护失败：${errorMessage(reason)}`);
      });
    void api
      .onApplicationExitRequested(() => void requestClose())
      .then((stop) => {
        if (disposed) stop();
        else exitUnlisten = stop;
      })
      .catch((reason) => {
        if (!disposed) onError(`注册应用退出保护失败：${errorMessage(reason)}`);
      });
  }
  return () => {
    disposed = true;
    window.removeEventListener("beforeunload", beforeUnload);
    closeUnlisten?.();
    exitUnlisten?.();
  };
}
