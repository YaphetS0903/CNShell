import packageInfo from "../../package.json";

export const appVersion = packageInfo.version;
export const releaseDate = "2026-09-08";

export const releaseChannel = appVersion.includes("beta")
  ? "Beta 候选版"
  : appVersion.includes("alpha")
    ? "Alpha 预览版"
    : "稳定版";
