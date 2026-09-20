import { Profiler } from "react";
import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, it, vi } from "vitest";
import { ConnectionSidebar } from "./ConnectionSidebar";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import { nextConnectionCopyName } from "./connection-names";

const dialog = vi.hoisted(() => ({ open: vi.fn(), save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

it("generates an incrementing name when a connection is copied repeatedly", () => {
  expect(
    nextConnectionCopyName("Server", [
      "Server",
      "Server 副本",
      "Server 副本 2",
    ]),
  ).toBe("Server 副本 3");
  expect(
    nextConnectionCopyName("Server 副本 2", ["Server", "Server 副本"]),
  ).toBe("Server 副本 2");
});

it("does not rerender the connection list for transfer progress or unrelated errors", async () => {
  vi.spyOn(api, "listFolders").mockResolvedValue([]);
  useAppStore.setState({ connections: [], transfers: [], error: null });
  const renderCount = vi.fn();
  await act(async () => {
    render(
      <Profiler id="connections" onRender={renderCount}>
        <ConnectionSidebar connect={vi.fn()} />
      </Profiler>,
    );
  });
  expect(screen.getByRole("textbox", { name: "搜索连接" })).toBeInTheDocument();
  renderCount.mockClear();
  for (let index = 0; index < 10; index += 1) {
    act(() =>
      useAppStore.setState({ transfers: [], error: `unrelated-${index}` }),
    );
  }
  expect(renderCount).not.toHaveBeenCalled();
  act(() => useAppStore.setState({ connections: [] }));
  expect(renderCount).toHaveBeenCalledOnce();
  vi.restoreAllMocks();
});

it("imports an encrypted backup without relying on a system prompt", async () => {
  vi.restoreAllMocks();
  dialog.open.mockReset();
  dialog.open.mockResolvedValue("C:\\Users\\test\\connections.cnshell.json");
  vi.spyOn(api, "isDesktop").mockReturnValue(true);
  vi.spyOn(api, "listFolders").mockResolvedValue([]);
  vi.spyOn(api, "listConnections").mockResolvedValue([]);
  const importConnections = vi
    .spyOn(api, "importConnections")
    .mockRejectedValueOnce(new Error("该备份已加密，请输入导出口令"))
    .mockResolvedValueOnce(2);
  const prompt = vi.spyOn(window, "prompt");
  useAppStore.setState({ connections: [], error: null });
  const user = userEvent.setup();

  render(<ConnectionSidebar connect={vi.fn()} />);
  await user.click(screen.getByRole("button", { name: "导入连接" }));

  const modal = await screen.findByRole("dialog", { name: "导入加密备份" });
  await user.type(screen.getByLabelText("备份口令"), "portable-secret");
  await user.click(screen.getByRole("button", { name: "解密并导入" }));

  await waitFor(() =>
    expect(importConnections).toHaveBeenNthCalledWith(
      2,
      "C:\\Users\\test\\connections.cnshell.json",
      "portable-secret",
    ),
  );
  expect(dialog.open).toHaveBeenCalledTimes(1);
  expect(prompt).not.toHaveBeenCalled();
  await waitFor(() => expect(modal).not.toBeInTheDocument());
  vi.restoreAllMocks();
});
