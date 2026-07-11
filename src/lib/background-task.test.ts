import { describe, expect, it, vi } from "vitest";
import { api } from "./api";
import { waitForTask } from "./background-task";
import type { BackgroundTask } from "../types";

const task = (status: BackgroundTask["status"]): BackgroundTask => ({
  id: "task",
  kind: "test",
  status,
  result: status === "completed" ? "done" : null,
  error: status === "failed" ? "boom" : null,
  createdAt: "now",
});

describe("background task waiting", () => {
  it("uses a snapshot to close the event subscription race", async () => {
    const stop = vi.fn();
    vi.spyOn(api, "onBackgroundTask").mockResolvedValue(stop);
    vi.spyOn(api, "getTask").mockResolvedValue(task("completed"));
    await expect(waitForTask(task("running"))).resolves.toBe("done");
    expect(stop).toHaveBeenCalledOnce();
  });

  it("propagates task failures", async () => {
    await expect(waitForTask(task("failed"))).rejects.toThrow("boom");
  });
});
