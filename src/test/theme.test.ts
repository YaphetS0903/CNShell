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

  it("defines readable editor colors for the light theme", () => {
    const styles = readFileSync(resolve("src/styles.css"), "utf8");

    expect(styles).toContain("--editor-bg:#fff");
    expect(styles).toContain("--editor-text:#162236");
    expect(contrastRatio("#ffffff", "#162236")).toBeGreaterThanOrEqual(4.5);
  });

  it("uses the active terminal palette for terminal chrome", () => {
    const styles = readFileSync(
      resolve("src/features/terminal/TerminalWorkspace.css"),
      "utf8",
    );

    expect(styles).toContain(
      "background: var(--terminal-tab-background, var(--surface-3));",
    );
    expect(styles).toContain(
      "color: var(--terminal-tab-foreground, var(--text));",
    );
    expect(styles).not.toContain("background: #07101ddd");
  });
});

function contrastRatio(background: string, foreground: string) {
  const luminance = (color: string) => {
    const channels = color
      .slice(1)
      .match(/.{2}/g)!
      .map((channel) => Number.parseInt(channel, 16) / 255)
      .map((channel) =>
        channel <= 0.04045
          ? channel / 12.92
          : ((channel + 0.055) / 1.055) ** 2.4,
      );
    return channels[0] * 0.2126 + channels[1] * 0.7152 + channels[2] * 0.0722;
  };
  const values = [luminance(background), luminance(foreground)].sort(
    (left, right) => right - left,
  );
  return (values[0] + 0.05) / (values[1] + 0.05);
}
