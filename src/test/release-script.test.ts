import { createHash } from "node:crypto";
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
  const mcpBuildScript = readFileSync(
    resolve("scripts/build-mcp-sidecar.sh"),
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
  const betaReleaseWorkflow = readFileSync(
    resolve(".github/workflows/beta-release.yml"),
    "utf8",
  );
  const betaConfigContents = readFileSync(
    resolve("src-tauri/tauri.beta.json"),
    "utf8",
  );
  const betaConfig = JSON.parse(betaConfigContents) as {
    plugins: { updater: { endpoints: string[]; pubkey: string } };
  };
  const betaTesting = readFileSync(resolve("docs/BETA_TESTING.md"), "utf8");
  const betaReport = readFileSync(
    resolve(".github/ISSUE_TEMPLATE/beta_report.yml"),
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
    expect(script).toContain('lipo -archs "$MCP_HELPER"');
    expect(script).toContain("rmcp-Apache-2.0.txt");
  });

  it("validates Mosh with an explicit terminal in non-interactive environments", () => {
    expect(moshBuildScript).toContain(
      'env TERM=xterm-256color "$OUTPUT/mosh-client" -c >/dev/null',
    );
    expect(script).toContain(
      'env TERM=xterm-256color "$MOSH_CLIENT" -c >/dev/null',
    );
    expect(workflow).toContain('env TERM=xterm-256color "$mosh" -c >/dev/null');
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

  it("keeps the unsigned Beta independent of Apple credentials while signing updater artifacts", () => {
    expect(betaReleaseWorkflow).not.toContain("APPLE_CERTIFICATE_BASE64");
    expect(betaReleaseWorkflow).not.toContain("APPLE_CERTIFICATE_PASSWORD");
    expect(betaReleaseWorkflow).not.toContain("APPLE_API_ISSUER");
    expect(betaReleaseWorkflow).not.toContain("APPLE_API_KEY_CONTENT");
    expect(betaReleaseWorkflow).toContain('APPLE_SIGNING_IDENTITY: "-"');
    expect(betaReleaseWorkflow).toContain("secrets.TAURI_SIGNING_PRIVATE_KEY");
    expect(betaReleaseWorkflow).toContain(
      "secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD",
    );
    expect(betaReleaseWorkflow).toContain("--verify-updater-signature");
  });

  it("publishes a verified four-platform Beta without overwriting a public release", () => {
    for (const platform of [
      "darwin-aarch64",
      "darwin-x86_64",
      "windows-x86_64",
      "windows-aarch64",
    ]) {
      expect(betaReleaseWorkflow).toContain(platform);
    }
    for (const artifact of [
      "SHA256SUMS.txt",
      "freerdp-3.28.0.tar.gz",
      "freerdp-cnshell-windows-port-source.zip",
      "mosh-1.4.0.tar.gz",
      "protobuf-all-21.12.tar.gz",
      "mosh-windows-port-source.zip",
      "gku201.tar.gz",
      "gkermit-windows-port-source.zip",
    ]) {
      expect(betaReleaseWorkflow).toContain(artifact);
    }
    expect(betaReleaseWorkflow).toContain("gh release create");
    expect(betaReleaseWorkflow).toContain("--draft --prerelease");
    expect(betaReleaseWorkflow).toContain(
      "Refusing to overwrite an existing public release",
    );
    expect(betaReleaseWorkflow).toContain("updates/beta/latest.json");
    const createDraft = betaReleaseWorkflow.indexOf("gh release create");
    const publishPreRelease = betaReleaseWorkflow.indexOf(
      "--draft=false --prerelease",
    );
    expect(createDraft).toBeGreaterThan(0);
    expect(publishPreRelease).toBeGreaterThan(createDraft);
    expect(betaReleaseWorkflow.indexOf("remoteHash")).toBeLessThan(
      publishPreRelease,
    );
  });

  it("pins the Beta updater endpoint and public key", () => {
    expect(betaConfig.plugins.updater.endpoints).toEqual([
      "https://raw.githubusercontent.com/YaphetS0903/CNShell/main/updates/beta/latest.json",
    ]);
    expect(betaConfig.plugins.updater.pubkey).toHaveLength(152);
    expect(
      createHash("sha256")
        .update(betaConfig.plugins.updater.pubkey)
        .digest("hex"),
    ).toBe("ffa2a9cc94b85eec6df771fcc6fffd5bd51933b83061d216d45582995b45331d");
  });

  it("documents unsigned Beta warnings and the real-device feedback matrix", () => {
    expect(betaTesting).toContain("Gatekeeper");
    expect(betaTesting).toContain("SmartScreen");
    expect(betaTesting).toContain("不要关闭 Gatekeeper");
    expect(betaTesting).toContain("不要关闭 SmartScreen");
    expect(betaTesting).toContain("不能替代 Developer ID");
    expect(betaTesting).toContain("Windows 10 22H2");
    expect(betaTesting).toContain("Windows 11 ARM64");
    expect(betaTesting).toContain("中文 IME");
    expect(betaTesting).toContain("100%/125%/150%/200% DPI");
    expect(betaReport).toContain("Windows 10 22H2 x64");
    expect(betaReport).toContain("Windows 11 ARM64");
    expect(betaReport).toContain("中文输入");
    expect(betaReport).toContain("DPI");
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
    expect(desktopBuildScript).toContain('["freerdp", "mosh", "kermit", "mcp"]');
    for (const buildScript of [
      freeRdpBuildScript,
      moshBuildScript,
      kermitBuildScript,
      mcpBuildScript,
    ]) {
      expect(buildScript).toContain("sign-macos-binary.sh");
    }
    expect(signingScript).toContain("--options runtime --timestamp");
    expect(signingScript).toContain("--options runtime --sign -");
    expect(script).toContain('verify_developer_id_signature "$FREERDP_HELPER"');
    expect(script).toContain('verify_developer_id_signature "$MOSH_CLIENT"');
    expect(script).toContain('verify_developer_id_signature "$KERMIT_HELPER"');
    expect(script).toContain('verify_developer_id_signature "$MCP_HELPER"');
  });

  it("stages the MCP binary where Tauri expects universal Cargo binaries", () => {
    expect(mcpBuildScript).toContain(
      'TAURI_UNIVERSAL_OUTPUT="$TARGET/universal-apple-darwin/release/cnshell-mcp"',
    );
    expect(mcpBuildScript).toContain(
      'cp "$OUTPUT/cnshell-mcp" "$TAURI_UNIVERSAL_OUTPUT"',
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
  const betaReleaseWorkflow = readFileSync(
    resolve(".github/workflows/beta-release.yml"),
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
  const windowsMoshBuilder = readFileSync(
    resolve("scripts/build-mosh-sidecar.ps1"),
    "utf8",
  );
  const windowsMoshTest = readFileSync(
    resolve("scripts/test-mosh-windows.ps1"),
    "utf8",
  );
  const windowsMoshCompat = readFileSync(
    resolve("scripts/mosh-windows/mosh-windows-compat.cc"),
    "utf8",
  );
  const windowsMoshCmake = readFileSync(
    resolve("scripts/mosh-windows/CMakeLists.txt"),
    "utf8",
  );
  const windowsMoshPrefix = readFileSync(
    resolve("scripts/mosh-windows/include/mosh-windows-prefix.h"),
    "utf8",
  );
  const windowsMoshSocket = readFileSync(
    resolve("scripts/mosh-windows/include/sys/socket.h"),
    "utf8",
  );
  const windowsMoshUnistd = readFileSync(
    resolve("scripts/mosh-windows/include/unistd.h"),
    "utf8",
  );
  const desktopBuildScript = readFileSync(
    resolve("scripts/build-desktop.mjs"),
    "utf8",
  );
  const windowsKermitIo = readFileSync(
    resolve("scripts/kermit-windows/gkermit-windows-io.c"),
    "utf8",
  );
  const windowsKermitCompat = readFileSync(
    resolve("scripts/kermit-windows/gkermit-windows-compat.h"),
    "utf8",
  );
  const dependabot = readFileSync(resolve(".github/dependabot.yml"), "utf8");
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

    for (const contents of [
      workflow,
      releaseWorkflow,
      windowsPackageWorkflow,
      betaReleaseWorkflow,
    ]) {
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
    expect(
      workflow.match(/components: clippy/g)?.length ?? 0,
    ).toBeGreaterThanOrEqual(1);
    expect(releaseWorkflow.match(/components: clippy/g)).toHaveLength(1);
    expect(workflow.match(/persist-credentials: false/g)).toHaveLength(
      workflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    expect(releaseWorkflow.match(/persist-credentials: false/g)).toHaveLength(
      releaseWorkflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    expect(
      windowsPackageWorkflow.match(/persist-credentials: false/g),
    ).toHaveLength(
      windowsPackageWorkflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    expect(
      betaReleaseWorkflow.match(/persist-credentials: false/g),
    ).toHaveLength(
      betaReleaseWorkflow.match(/actions\/checkout@/g)?.length ?? 0,
    );
    for (const contents of [
      workflow,
      releaseWorkflow,
      windowsPackageWorkflow,
      betaReleaseWorkflow,
    ]) {
      expect(contents).toMatch(/^permissions:\n {2}contents: read$/m);
    }
    expect(releaseWorkflow).toContain(uploadArtifact);
    expect(betaReleaseWorkflow).toContain(uploadArtifact);
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
    expect(installerTest).toContain(
      'Join-Path $DataDirectory "cnshell.sqlite"',
    );
    expect(installerTest).toContain(
      "Installed CNshell did not start a WebView2 renderer",
    );
    expect(installerTest).toContain(
      "did not accept a native window close request",
    );
    expect(installerTest).toContain("cmdkey.exe");
    expect(installerTest).toContain("Assert-UserData");
    expect(installerTest).toContain("Assert-TestCredential");
    expect(installerTest).toContain("Remove-TestCredential");
    expect(installerTest).toContain(
      "created a desktop shortcut without an explicit user choice",
    );
    expect(installerTest).toContain(
      "removed the user's existing desktop shortcut during upgrade",
    );
    expect(installerTest).toContain("New-ExistingDesktopShortcut");
    expect(installerTest).toContain("start menu shortcut was not created");
    expect(installerTest).toContain("Assert-BundledResources");
    expect(installerTest).toContain('"freerdp\\source\\freerdp-3.28.0.tar.gz"');
    expect(installerTest).toContain('"mosh\\mosh-client.exe"');
    expect(installerTest).toContain('"mosh\\source\\mosh-1.4.0.tar.gz"');
    expect(installerTest).toContain('"kermit\\source\\gku201.tar.gz"');
    expect(installerTest).toContain("Assert-CNshellStarts");
    expect(installerTest).toContain("-RedirectStandardOutput $preflightOutput");
    expect(installerTest).toContain("-RedirectStandardError $preflightError");
    expect(installerTest).toContain("$StartupTimeoutSeconds = 30");
    expect(installerTest).toContain("Start-Sleep -Milliseconds 500");
    expect(installerTest).not.toContain("Start-Sleep -Seconds 5");
    const installerHooks = readFileSync(
      resolve("src-tauri/windows/installer-hooks.nsh"),
      "utf8",
    );
    expect(installerHooks).toContain("!include WinVer.nsh");
    expect(installerHooks).toContain("${AtLeastBuild} 19045");
    expect(installerHooks).toContain("Windows 10 22H2");
    expect(peVerifier).toContain("0x8664");
    expect(peVerifier).toContain("0xAA64");
    expect(peVerifier).toContain("RequireWindowsGui");
    expect(windowsPackageWorkflow).toContain("-RequireWindowsGui");
    expect(releaseWorkflow).toContain("-RequireWindowsGui");
    expect(betaReleaseWorkflow).toContain("-RequireWindowsGui");
    expect(windowsFreeRdpBuilder).toContain("Get-VisualStudioGenerator");
    expect(windowsFreeRdpBuilder).toContain("vswhere.exe");
    expect(windowsFreeRdpBuilder).toContain('"x64-windows-static"');
    expect(windowsFreeRdpBuilder).toContain('"arm64-windows-static"');
    expect(windowsFreeRdpBuilder).not.toContain("windows-static-md");
    expect(windowsFreeRdpBuilder).not.toContain('-G "Visual Studio 17 2022"');
    expect(windowsFreeRdpBuilder).toContain(
      'Copy-Item -Force $Archive (Join-Path $SourceOutput "freerdp-$FreeRdpVersion.tar.gz")',
    );
    expect(windowsFreeRdpBuilder).toContain("freerdp-sdl-user-close.patch");
    expect(windowsFreeRdpBuilder).toContain("freerdp-sdl-state-marker.patch");
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
    expect(windowsKermitCompat).toContain("#ifdef NOGETENV");
    expect(windowsKermitCompat).toContain("#define gptr ((char *)0)");
    expect(windowsKermitCompat).toContain("#define __STDC__ 1");
    expect(windowsKermitBuilder).toContain("RedirectStandardError");
    expect(windowsPackageWorkflow).toContain("npm run build:mosh");
    expect(windowsPackageWorkflow).toContain("test-mosh-windows.ps1");
    expect(releaseWorkflow).toContain("test-mosh-windows.ps1");
    expect(releaseWorkflow).toContain("mosh-windows-port-source.zip");
    expect(windowsMoshBuilder).toContain("test-mosh-windows.ps1");
    expect(windowsMoshBuilder).toContain(
      'WindowsPortDestination "test-mosh-windows.ps1"',
    );
    expect(windowsMoshTest).toContain("RedirectStandardOutput");
    expect(windowsMoshTest).toContain("RedirectStandardError");
    expect(windowsMoshTest).toContain("$process.ExitCode");
    expect(windowsMoshTest).toContain("encrypted UDP loopback passed");
    expect(windowsMoshBuilder).toContain(
      "872e4b134e5df29c8933dff12350785054d2fd2839b5ae6b5587b14db1465ddd",
    );
    expect(windowsMoshBuilder).toContain(
      "2c6a36c7b5a55accae063667ef3c55f2642e67476d96d355ff0acb13dbb47f09",
    );
    expect(windowsMoshBuilder).toContain("verify-windows-pe.ps1");
    expect(windowsMoshBuilder).not.toContain("has not completed");
    expect(windowsMoshBuilder).not.toContain("MSYS2 build gate");
    expect(windowsMoshBuilder).toContain("Mosh ${MoshVersion}:");
    expect(windowsMoshBuilder).toContain(
      "Protocol Buffers ${ProtobufVersion}:",
    );
    expect(windowsMoshBuilder).toContain('"zlib:$Triplet"');
    expect(windowsMoshBuilder).toContain("CMAKE_POLICY_DEFAULT_CMP0091=NEW");
    expect(windowsMoshBuilder).toContain(
      "CMAKE_MSVC_RUNTIME_LIBRARY=MultiThreaded",
    );
    expect(windowsMoshBuilder).toContain(".cnshell-msvc-static-runtime");
    expect(windowsMoshBuilder).toContain("Replace-PinnedText");
    expect(windowsMoshBuilder).toContain("withoutPatchedTarget");
    expect(windowsMoshBuilder).toContain("both patched and unpatched targets");
    expect(windowsMoshBuilder).toContain(
      "Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $MoshSource",
    );
    expect(windowsMoshBuilder).toContain('L"Crypto exception: %hs"');
    expect(windowsMoshCmake).toContain('"${MOSH_SRC}/util/swrite.cc"');
    expect(windowsMoshCmake).toContain('"${MOSH_SRC}/util/timestamp.cc"');
    expect(windowsMoshCmake).toContain("find_package(ZLIB REQUIRED)");
    expect(windowsMoshCmake).toContain("ZLIB::ZLIB");
    expect(windowsPackageWorkflow).toContain("windows-sidecars-v4-");
    expect(releaseWorkflow).toContain("windows-sidecars-v4-");
    expect(windowsMoshPrefix).toContain("#define __attribute(value)");
    expect(windowsMoshPrefix).toContain("cnshell_read");
    expect(windowsMoshPrefix).toContain("cnshell_write");
    expect(windowsMoshBuilder).toContain("cnshell_read( fd, buf, buf_size )");
    expect(windowsMoshBuilder).toContain(
      "cnshell_write( fd, str + total_bytes_written,",
    );
    expect(windowsMoshUnistd).not.toContain("#define read");
    expect(windowsMoshUnistd).not.toContain("#define write");
    expect(windowsMoshSocket).not.toContain("struct cmsghdr {");
    expect(windowsMoshSocket).toContain("#undef CMSG_FIRSTHDR");
    expect(windowsMoshCompat).toContain("WSADuplicateSocketW");
    expect(windowsMoshCompat).toContain("GetConsoleScreenBufferInfo");
    expect(desktopBuildScript).toContain('"mosh", "mosh-client.exe"');
    expect(desktopBuildScript).toContain("process.env.npm_execpath");
    expect(desktopBuildScript).not.toContain('"npm.cmd"');
  });

  it("assembles a protected four-platform draft release", () => {
    expect(releaseWorkflow).toContain("darwin-aarch64");
    expect(releaseWorkflow).toContain("darwin-x86_64");
    expect(releaseWorkflow).toContain("windows-x86_64");
    expect(releaseWorkflow).toContain("windows-aarch64");
    expect(releaseWorkflow).toContain("SHA256SUMS.txt");
    expect(releaseWorkflow).toContain("gkermit-windows-port-source.zip");
    expect(releaseWorkflow).toContain("freerdp-3.28.0.tar.gz");
    expect(releaseWorkflow).toContain(
      "freerdp-cnshell-windows-port-source.zip",
    );
    expect(releaseWorkflow).toContain("mosh-windows-port-source.zip");
    expect(releaseWorkflow).toContain("mosh-1.4.0.tar.gz");
    expect(releaseWorkflow).toContain("protobuf-all-21.12.tar.gz");
    expect(releaseWorkflow).toContain("gh release create");
    expect(releaseWorkflow).toContain('$tag.Contains("-")');
    expect(releaseWorkflow).toContain('"--prerelease"');
    expect(releaseWorkflow).toContain("--draft");
    expect(releaseWorkflow).toContain("contents: write");
    expect(releaseWorkflow).toContain(
      "Refusing to overwrite an existing public release",
    );
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
