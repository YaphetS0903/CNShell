import { api } from "./api";
import type { BackgroundTask } from "../types";

export async function waitForTask(task: BackgroundTask): Promise<unknown> {
  if (task.status === "completed") return task.result;
  if (task.status === "failed") throw new Error(task.error ?? "后台任务失败");
  if (task.status === "cancelled") throw new DOMException("后台任务已取消", "AbortError");
  return new Promise((resolve, reject) => {
    let closed = false;
    let unlisten: () => void = () => undefined;
    const finish = (snapshot: BackgroundTask) => {
      if (closed || snapshot.id !== task.id || !["completed", "failed", "cancelled"].includes(snapshot.status)) return;
      closed = true;
      unlisten();
      if (snapshot.status === "completed") resolve(snapshot.result);
      else if (snapshot.status === "cancelled") reject(new DOMException("后台任务已取消", "AbortError"));
      else reject(new Error(snapshot.error ?? "后台任务失败"));
    };
    void api.onBackgroundTask(finish).then((stop) => {
      if (closed) stop();
      else {
        unlisten = stop;
        void api.getTask(task.id).then(finish).catch(reject);
      }
    }).catch(reject);
  });
}
