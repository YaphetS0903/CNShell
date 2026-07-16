import { execFileSync, spawnSync } from "node:child_process";
import {
  chmodSync,
  mkdtempSync,
  readFileSync,
  rmSync,
  statSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { afterEach, describe, expect, it } from "vitest";

const describeOnMac = process.platform === "darwin" ? describe : describe.skip;
const script = resolve("scripts/external-acceptance-preflight.sh");
const temporaryDirectories: string[] = [];

afterEach(() => {
  for (const directory of temporaryDirectories.splice(0)) {
    rmSync(directory, { recursive: true, force: true });
  }
});

function secretEnvironment(overrides: NodeJS.ProcessEnv = {}) {
  return {
    ...process.env,
    APPLE_API_ISSUER: "secret-issuer-sentinel",
    APPLE_API_KEY: "secret-key-id-sentinel",
    APPLE_SIGNING_IDENTITY: "Developer ID Application: secret-identity-sentinel",
    TAURI_SIGNING_PRIVATE_KEY: "secret-updater-key-sentinel",
    UPDATER_DOWNLOAD_BASE_URL: "https://private-update.example.test/secret-path",
    CNSHELL_ACCEPTANCE_RDP_WINDOWS_10: "secret-rdp-host-10",
    CNSHELL_ACCEPTANCE_RDP_WINDOWS_11: "secret-rdp-host-11",
    CNSHELL_ACCEPTANCE_RDP_WINDOWS_SERVER: "secret-rdp-host-server",
    CNSHELL_ACCEPTANCE_MOSH_TARGET: "secret-mosh-host",
    CNSHELL_ACCEPTANCE_WEBDAV_URL: "https://secret-webdav.example.test/private",
    CNSHELL_ACCEPTANCE_SECOND_DEVICE: "secret-device-name",
    CNSHELL_ACCEPTANCE_RELAY_URL: "https://secret-relay.example.test",
    CNSHELL_RELAY_AGE_RECIPIENT: "age1secret-recipient-sentinel",
    CNSHELL_ACCEPTANCE_RELAY_BACKUP_TARGET: "secret-backup-target",
    ...overrides,
  };
}

const secretSentinels = [
  "secret-issuer-sentinel",
  "secret-key-id-sentinel",
  "Developer ID Application: secret-identity-sentinel",
  "secret-updater-key-sentinel",
  "https://private-update.example.test/secret-path",
  "secret-rdp-host-10",
  "secret-rdp-host-11",
  "secret-rdp-host-server",
  "secret-mosh-host",
  "https://secret-webdav.example.test/private",
  "secret-device-name",
  "https://secret-relay.example.test",
  "age1secret-recipient-sentinel",
  "secret-backup-target",
];

describeOnMac("external acceptance preflight", () => {
  it("reports every external gate without exposing configured values", () => {
    const directory = mkdtempSync(join(tmpdir(), "cnshell-preflight-key-"));
    temporaryDirectories.push(directory);
    const apiKeyPath = join(directory, "secret-auth-key.p8");
    writeFileSync(apiKeyPath, "private-key-sentinel", { mode: 0o600 });
    chmodSync(apiKeyPath, 0o600);
    const output = execFileSync("/bin/zsh", [script], {
      cwd: resolve("."),
      env: secretEnvironment({ APPLE_API_KEY_PATH: apiKeyPath }),
      encoding: "utf8",
    });

    for (const heading of [
      "Developer ID",
      "XQuartz / X11",
      "FIDO2 身份",
      "实体串口",
      "Windows RDP 矩阵",
      "Mosh 网络切换",
      "WebDAV 双设备",
      "生产 Relay",
    ]) {
      expect(output).toContain(heading);
    }
    for (const secret of secretSentinels) {
      expect(output).not.toContain(secret);
    }
    expect(output).not.toContain(apiKeyPath);
    expect(output).toMatch(/\| Apple 公证凭据 \| READY \|/);
    expect(output).toMatch(/\| 签名更新服务 \| READY \|/);
    expect(output).toMatch(/\| Windows RDP 矩阵 \| READY \|/);
    expect(output).toMatch(/\| WebDAV 双设备 \| READY \|/);
    expect(output).toMatch(/\| 生产 Relay \| READY \|/);
    expect(output).toContain("READY 只表示前置条件已检测到");
  });

  it("atomically writes a private Markdown report", () => {
    const directory = mkdtempSync(join(tmpdir(), "cnshell-preflight-"));
    temporaryDirectories.push(directory);
    const report = join(directory, "report.md");

    execFileSync("/bin/zsh", [script, "--output", report], {
      cwd: resolve("."),
      env: secretEnvironment(),
    });

    expect(statSync(report).mode & 0o777).toBe(0o600);
    const contents = readFileSync(report, "utf8");
    expect(contents).toContain("# CNshell 外部验收预检");
    for (const secret of secretSentinels) {
      expect(contents).not.toContain(secret);
    }
  });

  it("returns a distinct status when required prerequisites are missing", () => {
    const result = spawnSync("/bin/zsh", [script, "--require-ready"], {
      cwd: resolve("."),
      env: secretEnvironment(),
      encoding: "utf8",
    });

    expect(result.status).toBe(2);
    expect(result.stdout).toContain("MISSING");
  });

  it("refuses to replace a symbolic-link report target", () => {
    const directory = mkdtempSync(join(tmpdir(), "cnshell-preflight-link-"));
    temporaryDirectories.push(directory);
    const target = join(directory, "target.md");
    const link = join(directory, "report.md");
    writeFileSync(target, "unchanged", { mode: 0o600 });
    symlinkSync(target, link);

    const result = spawnSync("/bin/zsh", [script, "--output", link], {
      cwd: resolve("."),
      env: secretEnvironment(),
      encoding: "utf8",
    });

    expect(result.status).toBe(73);
    expect(readFileSync(target, "utf8")).toBe("unchanged");
  });
});
