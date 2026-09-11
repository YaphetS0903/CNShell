import { describe, expect, it } from "vitest";
import { virtualWindow } from "../../lib/runtime-metrics";
import { parseRemoteMode } from "./file-permissions";

describe("remote file virtualization", () => {
  it("renders only a bounded window in a 100,000 item directory", () => {
    const range = virtualWindow(100_000, 26 * 50_000, 520);
    expect(range.start).toBe(49_990);
    expect(range.end - range.start).toBeLessThanOrEqual(40);
    expect(range.top + range.bottom + (range.end - range.start) * 26).toBe(
      100_000 * 26,
    );
  });
  it("accepts only complete octal permission values", () => {
    expect(parseRemoteMode("755")).toBe(0o755);
    expect(parseRemoteMode(" 0644 ")).toBe(0o644);
    expect(parseRemoteMode("755junk")).toBeNull();
    expect(parseRemoteMode("888")).toBeNull();
    expect(parseRemoteMode("")).toBeNull();
  });
});
