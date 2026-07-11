import { describe, expect, it } from "vitest";
import { createTerminalInputQueue } from "./terminal-input";

describe("terminal input queue", () => {
  it("preserves xterm input order across asynchronous IPC calls", async () => {
    const delivered: string[] = [];
    const enqueue = createTerminalInputQueue(async (data) => {
      await new Promise((resolve) => setTimeout(resolve, data === "first" ? 10 : 0));
      delivered.push(data);
    });

    await Promise.all([enqueue("first"), enqueue("second"), enqueue("third")]);

    expect(delivered).toEqual(["first", "second", "third"]);
  });

  it("continues after a failed IPC write", async () => {
    const delivered: string[] = [];
    const enqueue = createTerminalInputQueue(async (data) => {
      if (data === "failed") throw new Error("offline");
      delivered.push(data);
    });

    await expect(enqueue("failed")).rejects.toThrow("offline");
    await enqueue("recovered");

    expect(delivered).toEqual(["recovered"]);
  });
});
