import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("release script gates", () => {
  const script = readFileSync(resolve("scripts/release.sh"), "utf8");
  const moshBuildScript = readFileSync(
    resolve("scripts/build-mosh-sidecar.sh"),
    "utf8",
  );
  const freeRdpBuildScript = readFileSync(
    resolve("scripts/build-freerdp-sidecar.sh"),
    "utf8",
  );
  const kermitBuildScript = readFileSync(
    resolve("scripts/build-kermit-sidecar.sh"),
    "utf8",
  );
  const signingScript = readFileSync(
    resolve("scripts/sign-macos-binary.sh"),
    "utf8",
  );
  const releaseWorkflow = readFileSync(
    resolve(".github/workflows/release.yml"),
    "utf8",
  );
  const packageJson = JSON.parse(
    readFileSync(resolve("package.json"), "utf8"),
  ) as { scripts: Record<string, string> };
  const desktopBuildScript = readFileSync(
    resolve("scripts/build-desktop.mjs"),
    "utf8",
  );
  const workflow = readFileSync(resolve(".github/workflows/ci.yml"), "utf8");

  it("uses the executable declared by the app bundle instead of assuming its case", () => {
    expect(script).toContain("Print :CFBundleExecutable");
    expect(script).toContain(
      'EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/$EXECUTABLE_NAME"',
    );
    expect(script).not.toContain("Contents/MacOS/CNshell");
  });

  it("verifies the release artifact, platform floor, notarization, and updater signature", () => {
    expect(script).toContain("Print :LSMinimumSystemVersion");
    expect(script).toContain('lipo -archs "$EXECUTABLE_PATH"');
    expect(script).toContain('hdiutil verify "$DMG_PATH"');
    expect(script).toContain('xcrun stapler validate "$APP_PATH"');
    expect(script).toContain('xcrun stapler validate "$DMG_PATH"');
    expect(script).toContain("*.app.tar.gz.sig");
    expect(script).toContain('"$EXECUTABLE_PATH" --verify-updater-signature');
    expect(script).toContain("src-tauri/tauri.release.json");
    expect(script).toContain("generate-updater-manifest.mjs");
    expect(script.indexOf("generate-updater-manifest.mjs")).toBeGreaterThan(
      script.indexOf('"$EXECUTABLE_PATH" --verify-updater-signature'),
    );
    expect(script).toContain('"$BUNDLE_ROOT/latest.json"');
    expect(script).toContain('lipo -archs "$MOSH_CLIENT"');
    expect(script).toContain("Mosh-GPL-3.0-or-later.txt");
  });

  it("validates Mosh with an explicit terminal in non-interactive environments", () => {
    expect(moshBuildScript).toContain(
      'env TERM=xterm-256color "$OUTPUT/mosh-client" -c >/dev/null',
    );
    expect(script).toContain(
      'env TERM=xterm-256color "$MOSH_CLIENT" -c >/dev/null',
    );
    expect(workflow).toContain(
      'env TERM=xterm-256color "$mosh" -c >/dev/null',
    );
  });

  it("keeps optional host JSON libraries out of universal FreeRDP builds", () => {
    expect(freeRdpBuildScript).toContain('FREERDP_BUILD_REVISION="5"');
    expect(freeRdpBuildScript).toContain("-DWITH_JSON_DISABLED=ON");
  });

  it("imports a Developer ID certificate into an ephemeral CI keychain", () => {
    expect(releaseWorkflow).toContain("APPLE_CERTIFICATE_BASE64");
    expect(releaseWorkflow).toContain("APPLE_CERTIFICATE_PASSWORD");
    expect(releaseWorkflow).toContain("security import");
    expect(releaseWorkflow).toContain("security set-key-partition-list");
    expect(releaseWorkflow).toContain("security find-identity");
    expect(releaseWorkflow).toContain("security delete-keychain");
    expect(releaseWorkflow).toContain("UPDATER_DOWNLOAD_BASE_URL");
    expect(releaseWorkflow).toContain("bundle/latest.json");
  });

  it("removes release credentials before running the artifact upload action", () => {
    const cleanup = releaseWorkflow.indexOf(
      "- name: Remove release credentials",
    );
    const upload = releaseWorkflow.indexOf("actions/upload-artifact@");

    expect(cleanup).toBeGreaterThan(0);
    expect(cleanup).toBeLessThan(upload);
    expect(releaseWorkflow.slice(cleanup, upload)).toContain("if: always()");
    expect(releaseWorkflow.slice(cleanup, upload)).toContain(
      'test ! -e "$keychain_path"',
    );
    expect(releaseWorkflow.slice(cleanup, upload)).not.toContain("|| true");
    expect(releaseWorkflow.slice(upload)).toMatch(
      /actions\/upload-artifact@[0-9a-f]{40} # v7\.0\.1\n\s+if: success\(\)/,
    );
  });

  it("rebuilds and Developer ID signs every bundled sidecar", () => {
    expect(packageJson.scripts["build:desktop"]).toContain("build-desktop.mjs");
    expect(desktopBuildScript).toContain('["freerdp", "mosh", "kermit"]');
    for (const buildScript of [
      freeRdpBuildScript,
      moshBuildScript,
      kermitBuildScript,
    ]) {
      expect(buildScript).toContain("sign-macos-binary.sh");
    }
    expect(signingScript).toContain("--options runtime --timestamp");
    expect(signingScript).toContain("--options runtime --sign -");
    expect(script).toContain(
      'verify_developer_id_signature "$FREERDP_HELPER"',
    );
    expect(script).toContain(
      'verify_developer_id_signature "$MOSH_CLIENT"',
    );
    expect(script).toContain(
      'verify_developer_id_signature "$KERMIT_HELPER"',
    );
  });

  it("uses only system text tools in macOS packaging and release gates", () => {
    expect(kermitBuildScript).not.toMatch(/\brg\b/);
    expect(script).not.toMatch(/\brg\b/);
    expect(kermitBuildScript).toContain(
      'grep -F "G-Kermit $VERSION" >/dev/null',
    );
    expect(script).toContain("grep -Eq");
    expect(kermitBuildScript).not.toContain("grep -Fq");
    expect(script).not.toContain("| grep -Fq");
    for (const packagingGate of [
      freeRdpBuildScript,
      kermitBuildScript,
      script,
      workflow,
      releaseWorkflow,
    ]) {
      expect(packagingGate).not.toMatch(/\|\s*grep\s+-[EF]*q\b/);
    }
  });
});

describe("relay age release verification", () => {
  const script = readFileSync(
    resolve("services/team-relay/scripts/verify-age-release.sh"),
    "utf8",
  );
  const signerKeys = readFileSync(
    resolve("services/team-relay/age-sigsum-key.pub"),
    "utf8",
  );

  it("pins the verifier and checks the transparency proof before extraction", () => {
    expect(script).toContain("sigsum-verify (sigsum-go module) v0.13.1");
    expect(script).toContain('version="${CNSHELL_AGE_VERSION:-v1.3.1}"');
    expect(script).toContain("v1.2.1 | v1.3.1");
    expect(script).toContain("-P sigsum-generic-2025-1");
    expect(script).toContain('signer_keys="$root/age-sigsum-key.pub"');
    expect(script.indexOf('"$verify_bin" -k')).toBeGreaterThan(0);
    expect(script.indexOf('tar -xzf "$archive"')).toBeGreaterThan(
      script.indexOf('"$verify_bin" -k'),
    );
    expect(signerKeys.trim().split("\n")).toHaveLength(2);
    expect(signerKeys).not.toContain("PRIVATE KEY");
  });

  it("rejects existing destinations and removes partial output on failure", () => {
    expect(script).toContain(
      '[[ ! -e "$destination" && ! -L "$destination" ]]',
    );
    expect(script).toContain('if [[ "$completed" != true ]]');
    expect(script).toContain('rm -rf "$destination"');
    expect(script).toContain("release archive contains unexpected entries");
  });
});

describe("relay container smoke", () => {
  const script = readFileSync(
    resolve("services/team-relay/scripts/container-test.sh"),
    "utf8",
  );
  const workflow = readFileSync(resolve(".github/workflows/ci.yml"), "utf8");
  const releaseWorkflow = readFileSync(
    resolve(".github/workflows/release.yml"),
    "utf8",
  );
  const windowsPackageWorkflow = readFileSync(
    resolve(".github/workflows/windows-package.yml"),
    "utf8",
  );
  const windowsFreeRdpBuilder = readFileSync(
    resolve("scripts/build-freerdp-sidecar.ps1"),
    "utf8",
  );
  const windowsKermitBuilder = readFileSync(
    resolve("scripts/build-kermit-sidecar.ps1"),
    "utf8",
  );
  const windowsKermitIo = readFileSync(
    resolve("scripts/kermit-windows/gkermit-windows-io.c"),
    "utf8",
  );
  const dependabot = readFileSync(
    resolve(".github/dependabot.yml"),
    "utf8",
  );
  const playwrightConfig = readFileSync(
    resolve("playwright.config.ts"),
    "utf8",
  );
  const compose = readFileSync(
    resolve("services/team-relay/docker-compose.example.yml"),
    "utf8",
  );
  const dockerfile = readFileSync(
    resolve("services/team-relay/Dockerfile"),
    "utf8",
  );

  it("runs the real Docker and Compose path on a hosted Linux runner", () => {
    expect(workflow).toContain("relay-container:");
    expect(workflow).toContain("runs-on: ubuntu-latest");
    expect(workflow).toContain("npm run test:relay-container");
    expect(workflow).toContain("runs-on: macos-15");
    expect(workflow).not.toContain("runs-on: macos-13");
    expect(releaseWorkflow).toContain("runs-on: macos-15");
    expect(releaseWorkflow).not.toContain("runs-on: macos-13");
    expect(workflow).toContain("npx playwright install webkit");
    expect(releaseWorkflow).toContain("npx playwright install webkit");
    expect(playwrightConfig).toContain('browserName: "webkit"');
    expect(workflow).toContain("python3 -m venv");
    expect(workflow).toContain("scripts/requirements-pty-fixture.txt");
    expect(workflow).not.toContain("pip install --user");
    expect(script).toContain("docker compose");
    expect(script).toContain("up --detach --build");
    expect(script).toContain("/health");
    expect(script).toContain("/ready");
    expect(script).toContain("/metrics");
  });

  it("pins external actions and build toolchains exactly", () => {
    const checkout =
      "actions/checkout@9c091bb21b7c1c1d1991bb908d89e4e9dddfe3e0 # v7.0.0";
    const setupNode =
      "actions/setup-node@820762786026740c76f36085b0efc47a31fe5020 # v7.0.0";
    const rustToolchain =
      "dtolnay/rust-toolchain@4cda84d5c5c54efe2404f9d843567869ab1699d4";
    const rustCache =
      "Swatinem/rust-cache@c19371144df3bb44fab255c43d04cbc2ab54d1c4 # v2.9.1";
    const uploadArtifact =
      "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a # v7.0.1";

    for (const contents of [workflow, releaseWorkflow, windowsPackageWorkflow]) {
      const actionLines = contents
        .split(/\r?\n/)
        .filter((line) => /^\s*- uses:/.test(line));
      expect(actionLines.length).toBeGreaterThan(0);
      for (const line of actionLines) {
        expect(line).toMatch(/^\s*- uses: [^@\s]+@[0-9a-f]{40}(?: # .+)?$/);
      }
      expect(contents).toContain(checkout);
      expect(contents).toContain(setupNode);
      expect(contents).toContain("node-version: 20.20.2");
      expect(contents).toContain(rustToolchain);
      expect(contents).toContain("toolchain: 1.96.0");
      expect(contents).toContain(rustCache);
      for (const match of contents.matchAll(
        /^\s+(?:node-version|toolchain): (.+)$/gm,
      )) {
        expect(["20.20.2", "1.96.0"]).toContain(match[1]);
      }
    }
    expect(workflow.match(/components: clippy/g)?.length ?? 0).toBeGreaterThanOrEqual(1);
    expect(releaseWorkflow.match(/components: clippy/g)).toHaveLength(1);
    expect(workflow.match(/persist-credentials: false/g)).toHaveLength(
      workflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    expect(releaseWorkflow.match(/persist-credentials: false/g)).toHaveLength(
      releaseWorkflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    expect(windowsPackageWorkflow.match(/persist-credentials: false/g)).toHaveLength(
      windowsPackageWorkflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    for (const contents of [workflow, releaseWorkflow, windowsPackageWorkflow]) {
      expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    }
    expect(releaseWorkflow).toContain(uploadArtifact);
    expect(dependabot).toContain("package-ecosystem: github-actions");
    expect(dependabot).toContain("interval: monthly");
  });

  it("builds both Windows architectures and runs the x64 NSIS lifecycle", () => {
    const installerTest = readFileSync(
      resolve("scripts/test-windows-installer.ps1"),
      "utf8",
    );
    const peVerifier = readFileSync(
      resolve("scripts/verify-windows-pe.ps1"),
      "utf8",
    );

    expect(windowsPackageWorkflow).toContain("x86_64-pc-windows-msvc");
    expect(windowsPackageWorkflow).toContain("aarch64-pc-windows-msvc");
    expect(windowsPackageWorkflow).toContain("npm run build:freerdp");
    expect(windowsPackageWorkflow).toContain('"scripts/patches/**"');
    expect(windowsPackageWorkflow).toContain("test-windows-installer.ps1");
    expect(windowsPackageWorkflow).toContain("verify-windows-pe.ps1");
    expect(installerTest).toContain('if ($env:CI -ne "true")');
    expect(installerTest).toContain("CNshell in-place upgrade removed user data");
    expect(installerTest).toContain("CNshell uninstall removed user data without explicit consent");
    expect(installerTest).toContain("created a desktop shortcut without an explicit user choice");
    expect(installerTest).toContain("start menu shortcut was not created");
    expect(installerTest).toContain("Assert-CNshellStarts");
    expect(peVerifier).toContain("0x8664");
    expect(peVerifier).toContain("0xAA64");
    expect(windowsFreeRdpBuilder).toContain("Get-VisualStudioGenerator");
    expect(windowsFreeRdpBuilder).toContain("vswhere.exe");
    expect(windowsFreeRdpBuilder).toContain('"x64-windows-static"');
    expect(windowsFreeRdpBuilder).toContain('"arm64-windows-static"');
    expect(windowsFreeRdpBuilder).not.toContain("windows-static-md");
    expect(windowsFreeRdpBuilder).not.toContain('-G "Visual Studio 17 2022"');
    expect(windowsPackageWorkflow).toContain("npm run build:kermit");
    expect(windowsPackageWorkflow).toContain(
      "kermit::tests::bundled_helpers_interoperate_in_external_protocol_mode",
    );
    expect(windowsKermitBuilder).toContain(
      "19f9ac00d7b230d0a841928a25676269363c2925afc23e62704cde516fc1abbd",
    );
    expect(windowsKermitBuilder).toContain("verify-windows-pe.ps1");
    expect(windowsKermitBuilder).not.toContain("has not completed");
    expect(windowsKermitIo).toContain("PeekNamedPipe");
    expect(windowsKermitIo).toContain("MultiByteToWideChar");
  });

  it("assembles a protected four-platform draft release", () => {
    expect(releaseWorkflow).toContain("darwin-aarch64");
    expect(releaseWorkflow).toContain("darwin-x86_64");
    expect(releaseWorkflow).toContain("windows-x86_64");
    expect(releaseWorkflow).toContain("windows-aarch64");
    expect(releaseWorkflow).toContain("SHA256SUMS.txt");
    expect(releaseWorkflow).toContain("gkermit-windows-port-source.zip");
    expect(releaseWorkflow).toContain("gh release create");
    expect(releaseWorkflow).toContain("--draft");
    expect(releaseWorkflow).toContain("contents: write");
    expect(releaseWorkflow).toContain("Refusing to overwrite an existing public release");
  });

  it("checks container isolation, persistence, loopback binding, and graceful stop", () => {
    expect(compose).toContain(
      "127.0.0.1:${CNSHELL_RELAY_HOST_PORT:-8787}:8787",
    );
    expect(script).toContain("ReadonlyRootfs");
    expect(script).toContain("no-new-privileges:true");
    expect(script).toContain("10001:10001");
    expect(script).toContain("volume true");
    expect(script).toContain("stop --timeout 30 relay");
    expect(script).toContain("{{.State.ExitCode}}");
    expect(dockerfile).toContain(
      "rust:1.96-bookworm@sha256:a339861ae23e9abb272cea45dfafde21760d2ce6577a70f8a926153677902663",
    );
    expect(dockerfile).toContain(
      "debian:bookworm-slim@sha256:7b140f374b289a7c2befc338f42ebe6441b7ea838a042bbd5acbfca6ec875818",
    );
  });

  it("creates the generated FreeRDP directory before clean-clone Rust checks", () => {
    const prepareResources = "mkdir -p src-tauri/resources/freerdp";
    const rustTests = "cargo test --manifest-path src-tauri/Cargo.toml";

    expect(workflow).toContain(prepareResources);
    expect(workflow.indexOf(prepareResources)).toBeLessThan(
      workflow.indexOf(rustTests),
    );
  });

  it("verifies privacy metadata and all hardened universal sidecars", () => {
    expect(workflow).toContain("NSMicrophoneUsageDescription");
    expect(workflow).toContain("flags=.*runtime");
    expect(workflow).toContain("G-Kermit-GPL-2.0.txt");
    expect(workflow).toContain("gku201.tar.gz");
    expect(workflow).toContain("G-Kermit 2.01");
  });
});
