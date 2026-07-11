import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

const requiredDeliverables = [
  "CHANGELOG.md",
  "docs/USER_GUIDE.md",
  "docs/SHORTCUTS.md",
  "docs/ARCHITECTURE.md",
  "docs/SECURITY.md",
  "docs/TROUBLESHOOTING.md",
  "docs/INSTALLATION.md",
  "docs/ACCEPTANCE.md",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
];

describe("PLAN deliverables", () => {
  it.each(requiredDeliverables)("includes %s", (path) => {
    expect(existsSync(resolve(path))).toBe(true);
    expect(readFileSync(resolve(path), "utf8").trim().length).toBeGreaterThan(40);
  });

  it("keeps package, Tauri, Cargo, and changelog versions aligned", () => {
    const packageVersion = JSON.parse(readFileSync(resolve("package.json"), "utf8")).version;
    const tauriVersion = JSON.parse(readFileSync(resolve("src-tauri/tauri.conf.json"), "utf8")).version;
    const cargo = readFileSync(resolve("src-tauri/Cargo.toml"), "utf8");
    const changelog = readFileSync(resolve("CHANGELOG.md"), "utf8");

    expect(tauriVersion).toBe(packageVersion);
    expect(cargo).toMatch(new RegExp(`^version = "${packageVersion.replaceAll(".", "\\.")}"$`, "m"));
    expect(changelog).toContain(`## ${packageVersion}`);
  });

  it("documents install, manual upgrade, data-preserving replacement, and complete uninstall", () => {
    const guide = readFileSync(resolve("docs/INSTALLATION.md"), "utf8");

    expect(guide).toContain("## 安装");
    expect(guide).toContain("## 升级");
    expect(guide).toContain("## 卸载");
    expect(guide).toContain("Keychain");
    expect(guide).toContain("Application Support/com.cnshell.desktop");
  });
});
