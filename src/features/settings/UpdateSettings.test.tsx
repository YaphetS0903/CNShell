import { render, screen } from "@testing-library/react";
import type { DownloadEvent } from "@tauri-apps/plugin-updater";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { UpdateSettings } from "./UpdateSettings";

const updater = vi.hoisted(() => ({ check: vi.fn() }));
vi.mock("@tauri-apps/plugin-updater", () => ({ check: updater.check }));

describe("UpdateSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    updater.check.mockReset();
  });

  it("does not contact an updater endpoint in browser preview", async () => {
    const user = userEvent.setup();
    render(<UpdateSettings onError={vi.fn()}/>);
    await user.click(screen.getByRole("button", { name: "检查更新" }));
    expect(await screen.findByRole("status")).toHaveTextContent("仅在 CNshell 桌面版");
    expect(updater.check).not.toHaveBeenCalled();
  });

  it("explains that candidate builds intentionally have no release endpoint", () => {
    render(<UpdateSettings onError={vi.fn()}/>);
    expect(screen.getByRole("status")).toHaveTextContent("候选版未配置正式更新通道");
  });

  it("shows signed update metadata and only installs after confirmation", async () => {
    const user = userEvent.setup();
    const close = vi.fn().mockResolvedValue(undefined);
    const downloadAndInstall = vi.fn().mockImplementation(async (
      onEvent: (event: DownloadEvent) => void,
    ) => {
      onEvent({ event: "Started", data: { contentLength: 100 } });
      onEvent({ event: "Progress", data: { chunkLength: 40 } });
      onEvent({ event: "Progress", data: { chunkLength: 60 } });
      onEvent({ event: "Finished" });
    });
    updater.check.mockResolvedValue({
      version: "0.2.0",
      currentVersion: "0.1.1",
      body: "Security fixes",
      close,
      downloadAndInstall,
    });
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(true);

    render(<UpdateSettings onError={vi.fn()}/>);
    await user.click(screen.getByRole("button", { name: "检查更新" }));
    expect(await screen.findByRole("status")).toHaveTextContent(
      "发现 CNshell 0.2.0（当前 0.1.1）",
    );
    expect(screen.getByText("Security fixes")).toBeInTheDocument();

    await user.click(
      screen.getByRole("button", { name: "下载并安装 0.2.0" }),
    );
    expect(confirm).toHaveBeenCalledWith(
      "下载并安装 CNshell 0.2.0？安装完成后请重新启动应用。",
    );
    expect(downloadAndInstall).toHaveBeenCalledOnce();
    expect(await screen.findByRole("status")).toHaveTextContent(
      "更新已安装",
    );
  });

  it("keeps the current version available when installation fails", async () => {
    const user = userEvent.setup();
    const onError = vi.fn();
    updater.check.mockResolvedValue({
      version: "0.2.0",
      currentVersion: "0.1.1",
      body: null,
      close: vi.fn().mockResolvedValue(undefined),
      downloadAndInstall: vi.fn().mockRejectedValue(new Error("bad signature")),
    });
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    vi.spyOn(window, "confirm").mockReturnValue(true);

    render(<UpdateSettings onError={onError}/>);
    await user.click(screen.getByRole("button", { name: "检查更新" }));
    await user.click(
      await screen.findByRole("button", { name: "下载并安装 0.2.0" }),
    );

    expect(await screen.findByRole("status")).toHaveTextContent(
      "当前版本保持可用",
    );
    expect(onError).toHaveBeenCalledWith(
      expect.stringContaining("bad signature"),
    );
  });
});
