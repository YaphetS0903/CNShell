const esbuild = require("esbuild");

esbuild.buildSync({
  entryPoints: ["electron/preload.ts"],
  outfile: "dist-electron/electron/preload.cjs",
  bundle: true,
  platform: "node",
  target: "node22",
  format: "cjs",
  external: ["electron"],
  sourcemap: false,
  logLevel: "info"
});
