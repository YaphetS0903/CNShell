#!/usr/bin/env node

import { existsSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";

const [archiveArgument, signatureArgument, urlArgument, outputArgument] = process.argv.slice(2);

if (!archiveArgument || !signatureArgument || !urlArgument || !outputArgument) {
  console.error("Usage: generate-updater-manifest.mjs <archive> <signature> <https-url> <output>");
  process.exit(1);
}

const archive = resolve(archiveArgument);
const signaturePath = resolve(signatureArgument);
const output = resolve(outputArgument);

if (signaturePath !== `${archive}.sig`) {
  throw new Error("Updater signature filename must match the signed archive");
}

for (const [label, path] of [["archive", archive], ["signature", signaturePath]]) {
  if (!existsSync(path) || !statSync(path).isFile() || statSync(path).size === 0) {
    throw new Error(`Updater ${label} is missing or empty: ${path}`);
  }
}

const downloadUrl = new URL(urlArgument);
if (downloadUrl.protocol !== "https:" || downloadUrl.username || downloadUrl.password || downloadUrl.hash) {
  throw new Error("Updater download URL must be HTTPS without credentials or a fragment");
}
if (basename(downloadUrl.pathname) !== basename(archive)) {
  throw new Error("Updater download URL filename must match the signed archive");
}

const packageJson = JSON.parse(readFileSync(resolve("package.json"), "utf8"));
const version = packageJson.version;
if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error("package.json contains an invalid release version");
}

const signature = readFileSync(signaturePath, "utf8").trim();
if (!signature || signature.length > 16_384 || signature.includes("\0")) {
  throw new Error("Updater signature is invalid or exceeds 16 KiB");
}

const changelog = readFileSync(resolve("CHANGELOG.md"), "utf8");
const releaseHeader = `## ${version}`;
const releaseStart = changelog.indexOf(releaseHeader);
if (releaseStart < 0) throw new Error(`CHANGELOG.md has no section for ${version}`);
const notesStart = changelog.indexOf("\n", releaseStart) + 1;
const nextRelease = changelog.indexOf("\n## ", notesStart);
const notes = changelog.slice(notesStart, nextRelease < 0 ? undefined : nextRelease).trim();
if (!notes || Buffer.byteLength(notes, "utf8") > 65_536) {
  throw new Error("Release notes are empty or exceed 64 KiB");
}

const pubDate = process.env.CNSHELL_RELEASE_PUB_DATE ?? new Date().toISOString();
if (!Number.isFinite(Date.parse(pubDate))) throw new Error("CNSHELL_RELEASE_PUB_DATE is not RFC 3339");

const platform = { url: downloadUrl.toString(), signature };
const manifest = {
  version,
  notes,
  pub_date: new Date(pubDate).toISOString(),
  platforms: {
    "darwin-aarch64": platform,
    "darwin-x86_64": platform,
  },
};

const temporary = resolve(dirname(output), `.${basename(output)}.${process.pid}.tmp`);
writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644, flag: "wx" });
renameSync(temporary, output);
