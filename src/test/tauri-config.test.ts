import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

interface TauriConfig {
  app?: {
    windows?: Array<{
      title?: string;
      width?: number;
      height?: number;
      minWidth?: number;
      minHeight?: number;
      titleBarStyle?: string;
      hiddenTitle?: boolean;
    }>;
  };
  bundle?: {
    macOS?: {
      hardenedRuntime?: unknown;
      infoPlist?: unknown;
    };
  };
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

  it("locks the Developer ID build to hardened runtime and a privacy plist", () => {
    const config = JSON.parse(readFileSync(resolve("src-tauri/tauri.macos.conf.json"), "utf8")) as TauriConfig;
    const privacyPlist = readFileSync(resolve("src-tauri/Info.plist"), "utf8");

    expect(config.bundle?.macOS?.hardenedRuntime).toBe(true);
    expect(config.bundle?.macOS?.infoPlist).toBe("Info.plist");
    expect(privacyPlist).toContain("NSMicrophoneUsageDescription");
    expect(privacyPlist).toContain("RDP");
  });
});

describe("Tauri window permissions", () => {
  it("allows the close-request handler to destroy the main window", () => {
    const capability = JSON.parse(readFileSync(resolve("src-tauri/capabilities/default.json"), "utf8")) as { permissions: string[] };

    expect(capability.permissions).toContain("core:window:allow-destroy");
  });
});

describe("Windows desktop shell", () => {
  it("keeps the native Windows title and window dimensions explicit", () => {
    const config = JSON.parse(
      readFileSync(resolve("src-tauri/tauri.windows.conf.json"), "utf8"),
    ) as TauriConfig;
    const window = config.app?.windows?.[0];

    expect(window).toMatchObject({
      title: "CNshell",
      width: 1440,
      height: 900,
      minWidth: 900,
      minHeight: 620,
      titleBarStyle: "Visible",
      hiddenTitle: false,
    });
  });

  it("builds the release executable as a GUI process and keeps the menu macOS-only", () => {
    const main = readFileSync(resolve("src-tauri/src/main.rs"), "utf8");
    const lib = readFileSync(resolve("src-tauri/src/lib.rs"), "utf8");

    expect(main).toContain(
      '#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]',
    );
    expect(lib).toContain("#[cfg(target_os = \"macos\")]\nfn build_menu");
    expect(lib).toContain(
      '#[cfg(target_os = "macos")]\n            app.set_menu(build_menu(app)?)?',
    );
  });
});
