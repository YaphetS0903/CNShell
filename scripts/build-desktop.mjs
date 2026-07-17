#!/usr/bin/env node

import { spawnSync } from "node:child_process";

const run = (command, args) => {
  const result = spawnSync(command, args, { stdio: "inherit", env: process.env });
  if (result.error) throw result.error;
  if (result.status !== 0) process.exit(result.status ?? 1);
};

for (const sidecar of ["freerdp", "mosh", "kermit"]) {
  run(process.execPath, ["scripts/build-sidecar.mjs", sidecar]);
}
run(process.platform === "win32" ? "npm.cmd" : "npm", ["run", "build"]);
