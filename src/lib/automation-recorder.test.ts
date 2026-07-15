import { describe, expect, it, vi } from "vitest";
import {
  compileRecordedActions,
  isSensitiveCommand,
  listenRecordableAction,
  publishRecordableCommand,
} from "./automation-recorder";

describe("automation recorder", () => {
  it("records only structured command-panel actions", () => {
    const listener = vi.fn();
    const stop = listenRecordableAction(listener);
    expect(publishRecordableCommand("server", "uname -a")).toBe(true);
    expect(listener).toHaveBeenCalledWith(expect.objectContaining({ connectionId: "server", command: "uname -a", source: "commandPanel" }));
    stop();
  });

  it("drops sensitive commands before they enter a recording", () => {
    const listener = vi.fn();
    const stop = listenRecordableAction(listener);
    expect(isSensitiveCommand("curl --password=secret https://example.test")).toBe(true);
    expect(publishRecordableCommand("server", "curl --password=secret https://example.test")).toBe(false);
    expect(listener).not.toHaveBeenCalled();
    stop();
  });

  it("compiles a connection-scoped recording into restricted steps", () => {
    const plan = compileRecordedActions("发布检查", "server", [
      { kind: "command", connectionId: "server", command: "uname -a", recordedAt: "now", source: "commandPanel" },
      { kind: "command", connectionId: "other", command: "whoami", recordedAt: "now", source: "commandPanel" },
    ]);
    expect(plan?.steps).toHaveLength(1);
    expect(plan?.steps[0].command).toBe("uname -a");
  });
});
