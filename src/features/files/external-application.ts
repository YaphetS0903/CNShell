import type { PlatformCapabilities } from "../../types";

type ApplicationPlatform = Pick<PlatformCapabilities, "operatingSystem" | "displayName">;

export function externalApplicationDialogOptions(platform: ApplicationPlatform, title: string) {
  const windows = platform.operatingSystem === "windows";
  return {
    multiple: false as const,
    directory: !windows,
    defaultPath: windows ? undefined : "/Applications",
    title,
    ...(windows
      ? { filters: [{ name: "Windows 应用", extensions: ["exe", "com", "bat", "cmd"] }] }
      : {}),
  };
}
