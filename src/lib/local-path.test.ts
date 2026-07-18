import { describe, expect, it } from "vitest";
import { joinLocalPath, localPathName } from "./local-path";

describe("local path helpers", () => {
  it("extracts names from Windows drive, UNC, and POSIX paths", () => {
    expect(localPathName("C:\\Users\\chen\\日志.txt")).toBe("日志.txt");
    expect(localPathName("\\\\server\\share\\folder")).toBe("folder");
    expect(localPathName("/Users/chen/log.txt")).toBe("log.txt");
    expect(localPathName("C:\\Users\\chen\\folder\\")).toBe("folder");
  });

  it("joins local destinations with the target platform separator", () => {
    expect(joinLocalPath("C:\\Downloads\\", "日志.txt", "windows")).toBe(
      "C:\\Downloads\\日志.txt",
    );
    expect(joinLocalPath("\\\\server\\share\\", "backup", "windows")).toBe(
      "\\\\server\\share\\backup",
    );
    expect(joinLocalPath("/tmp/", "backup", "macos")).toBe("/tmp/backup");
  });
});
