import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("system theme styles", () => {
  it("uses light semantic colors only for an automatic light theme", () => {
    const styles = readFileSync(resolve("src/styles.css"), "utf8");

    expect(styles).toContain(
      "@media(prefers-color-scheme:light){:root:not([data-theme]){",
    );
    expect(styles).toContain("--bg:#edf2f8");
    expect(styles).toContain("color-scheme:light");
  });

  it("defines readable editor colors for the light theme", () => {
    const styles = readFileSync(resolve("src/styles.css"), "utf8");

    expect(styles).toContain("--editor-bg:#fff");
    expect(styles).toContain("--editor-text:#162236");
    expect(contrastRatio("#ffffff", "#162236")).toBeGreaterThanOrEqual(4.5);
  });

  it("keeps faint text above AA contrast on semantic surfaces", () => {
    const styles = readFileSync(resolve("src/styles.css"), "utf8");
    const accessibility = readFileSync(
      resolve("src/accessibility.css"),
      "utf8",
    );
    const darkSurface = cssVariable(rootBlock(styles), "surface-3");
    const darkFaint = cssVariable(rootBlock(accessibility), "faint");
    const lightStyles = themeBlock(styles, "light");
    const lightAccessibility = themeBlock(accessibility, "light");
    const lightFaint = cssVariable(lightAccessibility, "faint");

    expect(contrastRatio(darkSurface, darkFaint)).toBeGreaterThanOrEqual(4.5);
    for (const surface of ["bg", "surface", "surface-2", "surface-3"]) {
      expect(
        contrastRatio(cssVariable(lightStyles, surface), lightFaint),
      ).toBeGreaterThanOrEqual(4.5);
    }
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

function rootBlock(styles: string) {
  return styles.match(/:root\s*\{([^}]*)\}/s)?.[1] ?? "";
}

function themeBlock(styles: string, theme: string) {
  return (
    styles.match(
      new RegExp(
        `:root\\[data-theme=["']${theme}["']\\]\\s*\\{([^}]*)\\}`,
        "s",
      ),
    )?.[1] ?? ""
  );
}

function cssVariable(block: string, name: string) {
  const value = block.match(
    new RegExp(`--${name.replace("-", "\\-")}\\s*:\\s*(#[0-9a-f]{3,8})`, "i"),
  )?.[1];
  if (!value) throw new Error(`Missing CSS variable --${name}`);
  return value.length === 4
    ? `#${value
        .slice(1)
        .split("")
        .map((character) => character.repeat(2))
        .join("")}`
    : value;
}

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
