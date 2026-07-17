import { useEffect, useState } from "react";
import type { PlatformCapabilities } from "../types";
import { api } from "./api";

const browserFallback: PlatformCapabilities = {
  operatingSystem: "browser",
  architecture: "unknown",
  displayName: "浏览器预览",
  shortcutModifier: "Ctrl",
  credentialStoreName: "系统凭据库",
  fileManagerName: "文件管理器",
  biometricName: "系统生物识别",
  rdp: { available: false, message: "桌面版可用" },
  mosh: { available: false, message: "桌面版可用" },
  kermit: { available: false, message: "桌面版可用" },
  x11: { available: false, message: "桌面版可用" },
  sshAgent: { available: false, message: "桌面版可用" },
  fido2: { available: false, message: "桌面版可用" },
  biometric: { available: false, message: "桌面版可用" },
  serial: { available: false, message: "桌面版可用" },
};

let cached = browserFallback;
let request: Promise<PlatformCapabilities> | null = null;

export function primaryShortcutPressed(
  event: Pick<KeyboardEvent, "metaKey" | "ctrlKey">,
  operatingSystem?: string,
): boolean {
  const detected =
    operatingSystem ??
    (typeof document !== "undefined" ? document.documentElement.dataset.platform : undefined);
  if (detected === "macos") return event.metaKey;
  if (detected === "windows") return event.ctrlKey;
  const browserRunsOnMac = typeof navigator !== "undefined" && /Mac/i.test(navigator.platform);
  return browserRunsOnMac ? event.metaKey : event.ctrlKey;
}

export function loadPlatformCapabilities(): Promise<PlatformCapabilities> {
  request ??= api.platformCapabilities().then((value) => {
    cached = value;
    document.documentElement.dataset.platform = value.operatingSystem;
    return value;
  });
  return request;
}

export function usePlatformCapabilities(): PlatformCapabilities {
  const [value, setValue] = useState(cached);
  useEffect(() => {
    let active = true;
    void loadPlatformCapabilities().then((next) => {
      if (active) setValue(next);
    });
    return () => {
      active = false;
    };
  }, []);
  return value;
}
