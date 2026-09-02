import { beforeEach, describe, expect, it } from "vitest";
import { clampPanelFontSize, panelFontSizeStorageKeys, readPanelFontSize } from "./panel-font-size";

describe("panel font size", () => {
  beforeEach(() => localStorage.clear());

  it("clamps values to the accessible panel range", () => {
    expect(clampPanelFontSize(8)).toBe(10);
    expect(clampPanelFontSize(12.6)).toBe(13);
    expect(clampPanelFontSize(99)).toBe(16);
    expect(clampPanelFontSize(Number.NaN, 14)).toBe(14);
  });

  it("restores a valid per-panel preference and ignores invalid values", () => {
    expect(readPanelFontSize(panelFontSizeStorageKeys.monitor)).toBe(11);
    localStorage.setItem(panelFontSizeStorageKeys.monitor, "15");
    expect(readPanelFontSize(panelFontSizeStorageKeys.monitor)).toBe(15);
    localStorage.setItem(panelFontSizeStorageKeys.monitor, "not-a-size");
    expect(readPanelFontSize(panelFontSizeStorageKeys.monitor, 13)).toBe(13);
  });
});
