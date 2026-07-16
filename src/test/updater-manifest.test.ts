import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { describe, expect, it } from "vitest";

describe("static updater manifest", () => {
  it("routes one signed universal archive to both macOS architectures", () => {
    const directory = mkdtempSync(join(tmpdir(), "cnshell-updater-"));
    const archive = join(directory, "CNshell.app.tar.gz");
    const signature = `${archive}.sig`;
    const output = join(directory, "latest.json");
    writeFileSync(archive, "signed archive");
    writeFileSync(signature, "trusted-signature\n");

    execFileSync(
      process.execPath,
      [
        "scripts/generate-updater-manifest.mjs",
        archive,
        signature,
        "https://updates.example/v0.1.1/CNshell.app.tar.gz",
        output,
      ],
      {
        cwd: process.cwd(),
        env: { ...process.env, CNSHELL_RELEASE_PUB_DATE: "2026-07-16T00:00:00Z" },
      },
    );

    const manifest = JSON.parse(readFileSync(output, "utf8")) as {
      version: string;
      notes: string;
      pub_date: string;
      platforms: Record<string, { url: string; signature: string }>;
    };
    expect(manifest.version).toBe("0.1.1");
    expect(manifest.notes).toContain("### 新增");
    expect(manifest.pub_date).toBe("2026-07-16T00:00:00.000Z");
    expect(Object.keys(manifest.platforms).sort()).toEqual([
      "darwin-aarch64",
      "darwin-x86_64",
    ]);
    expect(manifest.platforms["darwin-aarch64"]).toEqual(
      manifest.platforms["darwin-x86_64"],
    );
    expect(manifest.platforms["darwin-aarch64"].signature).toBe(
      "trusted-signature",
    );
  });

  it("rejects an insecure download URL", () => {
    const directory = mkdtempSync(join(tmpdir(), "cnshell-updater-"));
    const archive = join(directory, "CNshell.app.tar.gz");
    const signature = `${archive}.sig`;
    writeFileSync(archive, "signed archive");
    writeFileSync(signature, "trusted-signature");

    expect(() =>
      execFileSync(
        process.execPath,
        [
          "scripts/generate-updater-manifest.mjs",
          archive,
          signature,
          "http://updates.example/v0.1.1/CNshell.app.tar.gz",
          join(directory, "latest.json"),
        ],
        { cwd: process.cwd(), stdio: "ignore" },
      ),
    ).toThrow();
  });
});
