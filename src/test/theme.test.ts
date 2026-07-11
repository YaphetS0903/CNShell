import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("system theme styles", () => {
  it("uses light semantic colors only for an automatic light theme", () => {
    const styles = readFileSync(resolve("src/styles.css"), "utf8");

    expect(styles).toContain("@media(prefers-color-scheme:light){:root:not([data-theme]){");
    expect(styles).toContain("--bg:#edf2f8");
    expect(styles).toContain("color-scheme:light");
  });
});
