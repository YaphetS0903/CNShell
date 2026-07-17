#!/usr/bin/env node

import { existsSync, readFileSync, renameSync, statSync, writeFileSync } from "node:fs";
import { basename, dirname, resolve } from "node:path";

const allowedPlatforms = new Set([
  "darwin-aarch64",
  "darwin-x86_64",
  "windows-x86_64",
  "windows-aarch64",
]);

function parseArguments(arguments_) {
  if (arguments_[0] !== "--platform") {
    const [archive, signature, url, output] = arguments_;
    if (!archive || !signature || !url || !output || arguments_.length !== 4) {
      console.error("Usage: generate-updater-manifest.mjs <archive> <signature> <https-url> <output>");
      process.exit(1);
    }
    return {
      output,
      entries: [
        { platform: "darwin-aarch64", archive, signature, url },
        { platform: "darwin-x86_64", archive, signature, url },
      ],
    };
  }

  const entries = [];
  let output = null;
  for (let index = 0; index < arguments_.length;) {
    const option = arguments_[index];
    if (option === "--platform") {
      const [platform, archive, signature, url] = arguments_.slice(index + 1, index + 5);
      if (!platform || !archive || !signature || !url) {
        throw new Error("--platform requires <target> <archive> <signature> <https-url>");
      }
      entries.push({ platform, archive, signature, url });
      index += 5;
    } else if (option === "--output") {
      output = arguments_[index + 1];
      if (!output) throw new Error("--output requires a path");
      index += 2;
    } else {
      throw new Error(`Unknown updater manifest option: ${option}`);
    }
  }
  const targets = entries.map(({ platform }) => platform);
  if (
    entries.length !== allowedPlatforms.size ||
    new Set(targets).size !== allowedPlatforms.size ||
    targets.some((target) => !allowedPlatforms.has(target))
  ) {
    throw new Error(`Multi-platform manifest requires exactly: ${[...allowedPlatforms].join(", ")}`);
  }
  if (!output) throw new Error("Multi-platform manifest requires --output <path>");
  return { entries, output };
}

function validatedPlatformEntry({ archive: archiveArgument, signature: signatureArgument, url }) {
  const archive = resolve(archiveArgument);
  const signaturePath = resolve(signatureArgument);
  if (signaturePath !== `${archive}.sig`) {
    throw new Error("Updater signature filename must match the signed archive");
  }
  for (const [label, path] of [["archive", archive], ["signature", signaturePath]]) {
    if (!existsSync(path) || !statSync(path).isFile() || statSync(path).size === 0) {
      throw new Error(`Updater ${label} is missing or empty: ${path}`);
    }
  }
  const downloadUrl = new URL(url);
  if (downloadUrl.protocol !== "https:" || downloadUrl.username || downloadUrl.password || downloadUrl.hash) {
    throw new Error("Updater download URL must be HTTPS without credentials or a fragment");
  }
  if (basename(downloadUrl.pathname) !== basename(archive)) {
    throw new Error("Updater download URL filename must match the signed archive");
  }
  const signature = readFileSync(signaturePath, "utf8").trim();
  if (!signature || signature.length > 16_384 || signature.includes("\0")) {
    throw new Error("Updater signature is invalid or exceeds 16 KiB");
  }
  return { url: downloadUrl.toString(), signature };
}

const { entries, output: outputArgument } = parseArguments(process.argv.slice(2));
const output = resolve(outputArgument);

const packageJson = JSON.parse(readFileSync(resolve("package.json"), "utf8"));
const version = packageJson.version;
if (typeof version !== "string" || !/^\d+\.\d+\.\d+(?:[-+][0-9A-Za-z.-]+)?$/.test(version)) {
  throw new Error("package.json contains an invalid release version");
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

const platforms = Object.fromEntries(
  entries.map((entry) => [entry.platform, validatedPlatformEntry(entry)]),
);
const manifest = {
  version,
  notes,
  pub_date: new Date(pubDate).toISOString(),
  platforms,
};

const temporary = resolve(dirname(output), `.${basename(output)}.${process.pid}.tmp`);
writeFileSync(temporary, `${JSON.stringify(manifest, null, 2)}\n`, { mode: 0o644, flag: "wx" });
renameSync(temporary, output);
