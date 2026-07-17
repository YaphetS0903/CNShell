#!/usr/bin/env node

import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

const name = process.argv[2];
if (!new Set(["freerdp", "mosh", "kermit"]).has(name)) {
  console.error("Usage: node scripts/build-sidecar.mjs <freerdp|mosh|kermit>");
  process.exit(2);
}

const windows = process.platform === "win32";
const script = resolve("scripts", `build-${name}-sidecar.${windows ? "ps1" : "sh"}`);
if (!existsSync(script)) {
  console.error(`${name} sidecar 暂不支持 ${process.platform} 构建：${script}`);
  process.exit(1);
}
const executable = windows ? "powershell.exe" : "/bin/zsh";
const args = windows
  ? ["-NoLogo", "-NoProfile", "-NonInteractive", "-ExecutionPolicy", "Bypass", "-File", script]
  : [script];
const result = spawnSync(executable, args, { stdio: "inherit", env: process.env });
if (result.error) throw result.error;
process.exit(result.status ?? 1);
