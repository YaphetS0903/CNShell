import { describe, expect, it } from "vitest";
import { primaryShortcutPressed } from "./platform";

describe("platform shortcuts", () => {
  it("uses Command on macOS and Ctrl on Windows", () => {
    expect(primaryShortcutPressed({ metaKey: true, ctrlKey: false }, "macos")).toBe(true);
    expect(primaryShortcutPressed({ metaKey: false, ctrlKey: true }, "macos")).toBe(false);
    expect(primaryShortcutPressed({ metaKey: false, ctrlKey: true }, "windows")).toBe(true);
    expect(primaryShortcutPressed({ metaKey: true, ctrlKey: false }, "windows")).toBe(false);
  });
});
