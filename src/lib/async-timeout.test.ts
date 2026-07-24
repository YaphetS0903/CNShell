import { afterEach, describe, expect, it, vi } from "vitest";
import { withTimeout } from "./async-timeout";

describe("withTimeout", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("returns a completed request and clears its deadline", async () => {
    vi.useFakeTimers();
    await expect(withTimeout(Promise.resolve("ready"), 25_000, "超时")).resolves.toBe("ready");
    expect(vi.getTimerCount()).toBe(0);
  });

  it("rejects a request that never settles instead of waiting forever", async () => {
    vi.useFakeTimers();
    const result = withTimeout(new Promise<never>(() => {}), 25_000, "目录读取超时，请重试");
    const rejection = expect(result).rejects.toThrow("目录读取超时，请重试");

    await vi.advanceTimersByTimeAsync(25_000);

    await rejection;
  });
});
