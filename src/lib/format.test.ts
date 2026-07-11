import { describe, expect, it } from "vitest";
import { errorMessage, formatBytes } from "./format";

describe("formatBytes", () => {
  it("formats byte units without invalid values", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(1024)).toBe("1.0 KB");
    expect(formatBytes(1024 ** 3)).toBe("1.0 GB");
  });
});

describe("errorMessage", () => {
  it("supports structured and string errors", () => {
    expect(errorMessage("失败")).toBe("失败");
    expect(errorMessage({ message: "认证失败" })).toBe("认证失败");
  });
});
