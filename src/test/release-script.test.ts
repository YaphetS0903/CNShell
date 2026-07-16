import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("release script gates", () => {
  const script = readFileSync(resolve("scripts/release.sh"), "utf8");
  const moshBuildScript = readFileSync(
    resolve("scripts/build-mosh-sidecar.sh"),
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
    expect(workflow).toContain("actions/checkout@v5");
    expect(workflow).not.toContain("actions/checkout@v4");
    expect(workflow).toContain("actions/setup-node@v5");
    expect(workflow).not.toContain("actions/setup-node@v4");
    expect(workflow).toContain("runs-on: macos-15");
    expect(workflow).not.toContain("runs-on: macos-13");
    expect(releaseWorkflow).toContain("actions/checkout@v5");
    expect(releaseWorkflow).not.toContain("actions/checkout@v4");
    expect(releaseWorkflow).toContain("actions/setup-node@v5");
    expect(releaseWorkflow).not.toContain("actions/setup-node@v4");
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
});
