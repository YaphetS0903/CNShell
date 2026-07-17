#!/usr/bin/env node

import { spawnSync } from "node:child_process";
import { existsSync, statSync } from "node:fs";
import { resolve } from "node:path";

const run = (command, args) => {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
};

if (process.env.CNSHELL_SIDECARS_PREBUILT === "1") {
  if (process.platform !== "win32") {
    throw new Error("CNSHELL_SIDECARS_PREBUILT is restricted to Windows packaging jobs");
  }
  const helper = resolve("src-tauri", "resources", "freerdp", "sdl-freerdp.exe");
  if (!existsSync(helper) || !statSync(helper).isFile() || statSync(helper).size === 0) {
    throw new Error(`Prebuilt Windows FreeRDP helper is missing or empty: ${helper}`);
  }
} else {
  for (const sidecar of ["freerdp", "mosh", "kermit"]) {
    run(process.execPath, ["scripts/build-sidecar.mjs", sidecar]);
  }
}
run(process.platform === "win32" ? "npm.cmd" : "npm", ["run", "build"]);
