import { describe, expect, it } from "vitest";
import { nativeDropIsInsideElement } from "./native-file-drop";

const dropZone = {
  getBoundingClientRect: () => ({
    left: 40,
    top: 120,
    right: 440,
    bottom: 420,
  }),
};

describe("native file drop hit testing", () => {
  it("accepts native coordinates already expressed as CSS points", () => {
    expect(nativeDropIsInsideElement({ x: 160, y: 240 }, dropZone, 2)).toBe(true);
  });

  it("accepts physical Windows coordinates on a scaled display", () => {
    expect(nativeDropIsInsideElement({ x: 320, y: 480 }, dropZone, 2)).toBe(true);
  });

  it("rejects drops outside the remote file browser", () => {
    expect(nativeDropIsInsideElement({ x: 20, y: 40 }, dropZone, 2)).toBe(false);
  });
});
