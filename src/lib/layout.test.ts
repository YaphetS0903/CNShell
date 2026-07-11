import { describe, expect, it } from "vitest";
import { clampPanelSize, resizeFromKeyboard } from "./layout";

describe("resizable workspace panels", () => {
  it("clamps pointer sizes and maps accessible arrow keys", () => {
    expect(clampPanelSize(99, 180, 420)).toBe(180);
    expect(clampPanelSize(999, 180, 420)).toBe(420);
    expect(resizeFromKeyboard(260, "ArrowRight", "vertical")).toBe(276);
    expect(resizeFromKeyboard(260, "ArrowUp", "horizontal")).toBe(244);
  });
});
