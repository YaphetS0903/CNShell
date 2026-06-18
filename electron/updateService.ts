import type { BrowserWindow } from "electron";
import electronUpdater from "electron-updater";
import type { CheckForUpdatesRequest, UpdateStatus } from "../src/shared/ipc.js";

const { autoUpdater } = electronUpdater;

export class UpdateService {
  private status: UpdateStatus = {
    state: "idle",
    channel: "latest"
  };

  constructor(private readonly window: BrowserWindow) {
    autoUpdater.autoDownload = false;
    autoUpdater.on("checking-for-update", () => this.setStatus({ state: "checking" }));
    autoUpdater.on("update-available", (info) => {
      this.setStatus({
        state: "available",
        version: info.version,
        message: "Update available"
      });
      void autoUpdater.downloadUpdate();
    });
    autoUpdater.on("update-not-available", (info) =>
      this.setStatus({
        state: "not-available",
        version: info.version,
        message: "CNshell is up to date"
      })
    );
    autoUpdater.on("download-progress", (progress) =>
      this.setStatus({
        state: "downloading",
        percent: Math.round(progress.percent),
        message: `${Math.round(progress.percent)}%`
      })
    );
    autoUpdater.on("update-downloaded", (info) =>
      this.setStatus({
        state: "downloaded",
        version: info.version,
        message: "Update ready to install"
      })
    );
    autoUpdater.on("error", (error) =>
      this.setStatus({
        state: "error",
        message: error.message
      })
    );
  }

  getStatus() {
    return this.status;
  }

  async check(request: CheckForUpdatesRequest = {}) {
    const channel = request.channel?.trim() || "latest";
    autoUpdater.channel = channel;
    this.setStatus({ state: "checking", channel, message: "Checking for updates" });
    await autoUpdater.checkForUpdates();
    return this.status;
  }

  quitAndInstall() {
    if (this.status.state !== "downloaded") {
      return false;
    }

    autoUpdater.quitAndInstall(false, true);
    return true;
  }

  private setStatus(patch: Partial<UpdateStatus>) {
    this.status = {
      ...this.status,
      ...patch,
      channel: patch.channel ?? this.status.channel
    };
    this.window.webContents.send("updates:status", this.status);
  }
}
