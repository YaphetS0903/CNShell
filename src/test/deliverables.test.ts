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
  "docs/EXTERNAL_ACCEPTANCE.md",
  "docs/THIRD_PARTY_NOTICES.md",
  "src-tauri/resources/licenses/serialport-MPL-2.0.txt",
  "src-tauri/resources/kermit/licenses/G-Kermit-GPL-2.0.txt",
  "src-tauri/resources/kermit/THIRD_PARTY_NOTICES.md",
  ".github/workflows/ci.yml",
  ".github/workflows/release.yml",
  "scripts/external-acceptance-preflight.sh",
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

  it("pins and verifies the bundled G-Kermit binary, GPL license, and source", () => {
    const build = readFileSync(resolve("scripts/build-kermit-sidecar.sh"), "utf8");
    const release = readFileSync(resolve("scripts/release.sh"), "utf8");

    expect(build).toContain("19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd");
    expect(build).toContain("G-Kermit-GPL-2.0.txt");
    expect(build).toContain("source/$ARCHIVE");
    expect(release).toContain("G-Kermit helper 不是 arm64 + x86_64 universal binary");
    expect(release).toContain("G-Kermit 对应源码缺失");
  });

  it("keeps Mosh resize automation and external acceptance boundaries consistent", () => {
    const acceptance = readFileSync(resolve("docs/ACCEPTANCE.md"), "utf8");
    const plan = readFileSync(resolve("docs/NEXT_DEVELOPMENT_PLAN.md"), "utf8");

    expect(acceptance).toContain("ResizeObserver");
    expect(acceptance).not.toContain("自动化屏幕 resize 捕获仍待");
    expect(plan).toContain("前端回归测试已覆盖 `ResizeObserver`");
  });
});
