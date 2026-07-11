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
  });
});
