import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

describe("release script gates", () => {
  const script = readFileSync(resolve("scripts/release.sh"), "utf8");

  it("uses the executable declared by the app bundle instead of assuming its case", () => {
    expect(script).toContain("Print :CFBundleExecutable");
    expect(script).toContain('EXECUTABLE_PATH="$APP_PATH/Contents/MacOS/$EXECUTABLE_NAME"');
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
    expect(script).toContain('[[ ! -e "$destination" && ! -L "$destination" ]]');
    expect(script).toContain('if [[ "$completed" != true ]]');
    expect(script).toContain('rm -rf "$destination"');
    expect(script).toContain("release archive contains unexpected entries");
  });
});
