import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

interface TauriConfig {
  plugins?: {
    updater?: {
      endpoints?: unknown;
      pubkey?: unknown;
    };
  };
}

describe("Tauri updater configuration", () => {
  it("always provides a deserializable base configuration", () => {
    const config = JSON.parse(readFileSync(resolve("src-tauri/tauri.conf.json"), "utf8")) as TauriConfig;

    expect(config.plugins?.updater).toBeDefined();
    expect(config.plugins?.updater?.endpoints).toEqual([]);
    expect(config.plugins?.updater?.pubkey).toBe("");
  });

  it("only grants the updater operations exposed by the explicit settings workflow", () => {
    const capability = JSON.parse(readFileSync(resolve("src-tauri/capabilities/default.json"), "utf8")) as { permissions: string[] };
    expect(capability.permissions).toContain("updater:allow-check");
    expect(capability.permissions).toContain("updater:allow-download-and-install");
    expect(capability.permissions).not.toContain("updater:default");
  });
});

describe("Tauri window permissions", () => {
  it("allows the close-request handler to destroy the main window", () => {
    const capability = JSON.parse(readFileSync(resolve("src-tauri/capabilities/default.json"), "utf8")) as { permissions: string[] };

    expect(capability.permissions).toContain("core:window:allow-destroy");
  });
});
