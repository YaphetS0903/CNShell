import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { ConnectionBackupSettings } from "./ConnectionBackupSettings";

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

describe("ConnectionBackupSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    dialog.open.mockReset();
    dialog.save.mockReset();
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
  });

  it("collects and confirms an export passphrase before choosing a destination", async () => {
    dialog.save.mockResolvedValue(
      "/Users/test/CNshell-connections-encrypted.cnshell.json",
    );
    const exportConnections = vi
      .spyOn(api, "exportConnections")
      .mockResolvedValue(undefined);
    const prompt = vi.spyOn(window, "prompt");
    const user = userEvent.setup();

    render(
      <ConnectionBackupSettings
        onChanged={vi.fn().mockResolvedValue(undefined)}
        onError={vi.fn()}
      />,
    );

    await user.click(
      screen.getByRole("button", { name: "导出含凭据的加密备份" }),
    );
    expect(dialog.save).not.toHaveBeenCalled();
    expect(screen.getByLabelText("导出口令")).toHaveFocus();
    expect(screen.getByText(/完成后将集中为一次授权/)).toBeInTheDocument();

    await user.type(screen.getByLabelText("导出口令"), "short");
    await user.type(screen.getByLabelText("确认口令"), "short");
    await user.click(
      screen.getByRole("button", { name: "选择保存位置并导出" }),
    );
    expect(screen.getByRole("alert")).toHaveTextContent("至少 8 位");
    expect(dialog.save).not.toHaveBeenCalled();

    await user.clear(screen.getByLabelText("导出口令"));
    await user.clear(screen.getByLabelText("确认口令"));
    await user.type(screen.getByLabelText("导出口令"), "portable-secret");
    await user.type(screen.getByLabelText("确认口令"), "portable-secret");
    await user.click(
      screen.getByRole("button", { name: "选择保存位置并导出" }),
    );

    await waitFor(() =>
      expect(exportConnections).toHaveBeenCalledWith(
        "/Users/test/CNshell-connections-encrypted.cnshell.json",
        true,
        "portable-secret",
      ),
    );
    expect(prompt).not.toHaveBeenCalled();
    expect(
      await screen.findByText("已导出含凭据的加密备份"),
    ).toBeInTheDocument();
    expect(screen.queryByLabelText("导出口令")).not.toBeInTheDocument();
  });

  it("requests the same passphrase inside the app when importing an encrypted backup", async () => {
    dialog.open.mockResolvedValue("C:\\Users\\test\\connections.cnshell.json");
    const importConnections = vi
      .spyOn(api, "importConnections")
      .mockRejectedValueOnce(new Error("该备份需要口令"))
      .mockResolvedValueOnce(3);
    const onChanged = vi.fn().mockResolvedValue(undefined);
    const user = userEvent.setup();

    render(
      <ConnectionBackupSettings onChanged={onChanged} onError={vi.fn()} />,
    );

    await user.click(screen.getByRole("button", { name: "导入备份" }));
    const input = await screen.findByLabelText("备份口令");
    await user.type(input, "portable-secret");
    await user.click(screen.getByRole("button", { name: "解密并导入" }));

    await waitFor(() =>
      expect(importConnections).toHaveBeenNthCalledWith(
        2,
        "C:\\Users\\test\\connections.cnshell.json",
        "portable-secret",
      ),
    );
    expect(dialog.open).toHaveBeenCalledTimes(1);
    expect(onChanged).toHaveBeenCalledOnce();
    expect(
      await screen.findByText("已从加密备份导入 3 条连接"),
    ).toBeInTheDocument();
  });
});
