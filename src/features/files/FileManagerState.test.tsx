import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import type { RemoteFile, TerminalSession } from "../../types";
import { FileManager } from "./FileManager";

const nativeDrop = vi.hoisted(() => ({ onDragDropEvent: vi.fn() }));

vi.mock("@tauri-apps/api/webview", () => ({
  getCurrentWebview: () => ({ onDragDropEvent: nativeDrop.onDragDropEvent }),
}));

const session = (id: string): TerminalSession => ({
  id,
  connectionId: `connection-${id}`,
  sessionType: "terminal",
  title: id,
  status: "online",
  startedAt: "",
  lastError: null,
});

const directory = (name: string, path: string): RemoteFile => ({
  name,
  path,
  kind: "directory",
  size: 0,
  modifiedAt: null,
  permissions: "drwxr-xr-x",
  owner: 0,
  group: 0,
});

const file = (name: string, path: string): RemoteFile => ({
  name,
  path,
  kind: "file",
  size: 16,
  modifiedAt: null,
  permissions: "-rw-r--r--",
  owner: 0,
  group: 0,
});

describe("FileManager navigation state", () => {
  beforeEach(() => {
    vi.stubGlobal(
      "ResizeObserver",
      class {
        observe() {}
        disconnect() {}
      },
    );
    workspaceRuntime.remoteFileBrowserBySession.clear();
    nativeDrop.onDragDropEvent.mockReset();
    nativeDrop.onDragDropEvent.mockResolvedValue(vi.fn());
    vi.spyOn(api, "listFiles").mockImplementation(async (_sessionId, path) => {
      if (path === "/") return [directory("home", "/home"), file("notes.txt", "/notes.txt")];
      if (path === "/home") return [directory("ubuntu", "/home/ubuntu")];
      return [];
    });
  });

  afterEach(() => {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
  });

  it("restores the current path and expanded folders after the panel remounts", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);

    await user.click(await screen.findByRole("button", { name: "home" }));
    expect(await screen.findByRole("button", { name: "ubuntu" })).toBeVisible();
    expect(screen.getByLabelText("远程路径")).toHaveValue("/home");

    first.unmount();
    render(<FileManager session={session("one")} />);

    expect(screen.getByLabelText("远程路径")).toHaveValue("/home");
    expect(await screen.findByRole("button", { name: "ubuntu" })).toBeVisible();
    expect(screen.getByRole("button", { name: "折叠 home" })).toBeVisible();
  });

  it("keeps navigation state isolated between SSH sessions", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);
    await user.click(await screen.findByRole("button", { name: "home" }));
    expect(screen.getByLabelText("远程路径")).toHaveValue("/home");

    first.unmount();
    render(<FileManager session={session("two")} />);
    expect(screen.getByLabelText("远程路径")).toHaveValue("/");
  });

  it("opens and dismisses the selected file action menu", async () => {
    const user = userEvent.setup();
    render(<FileManager session={session("one")} />);

    await user.click(await screen.findByRole("row", { name: /notes\.txt/ }));
    const more = screen.getByRole("button", { name: "更多文件操作" });
    expect(screen.queryByRole("button", { name: "编辑文本" })).not.toBeInTheDocument();

    await user.click(more);
    expect(screen.getByRole("button", { name: "编辑文本" })).toBeVisible();
    await user.click(more);
    expect(screen.queryByRole("button", { name: "编辑文本" })).not.toBeInTheDocument();

    await user.click(more);
    await user.click(screen.getByLabelText("远程路径"));
    expect(screen.queryByRole("button", { name: "编辑文本" })).not.toBeInTheDocument();

    await user.click(more);
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "编辑文本" })).not.toBeInTheDocument();

    await user.click(more);
    await user.click(screen.getByRole("button", { name: "复制路径" }));
    expect(screen.queryByRole("button", { name: "编辑文本" })).not.toBeInTheDocument();
  });

  it("queues files dropped through Tauri's native desktop event", async () => {
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    const enqueue = vi.spyOn(api, "enqueueTransfer").mockResolvedValue({} as never);
    const { container } = render(<FileManager session={session("one")} />);
    await screen.findByRole("row", { name: /notes\.txt/ });
    const browser = container.querySelector<HTMLDivElement>(".file-browser");
    expect(browser).not.toBeNull();
    vi.spyOn(browser!, "getBoundingClientRect").mockReturnValue({
      left: 0,
      top: 0,
      right: 800,
      bottom: 500,
    } as DOMRect);

    await waitFor(() => expect(nativeDrop.onDragDropEvent).toHaveBeenCalledOnce());
    const handler = nativeDrop.onDragDropEvent.mock.calls[0][0] as (event: {
      payload:
        | { type: "enter" | "over"; position: { x: number; y: number } }
        | { type: "drop"; paths: string[]; position: { x: number; y: number } }
        | { type: "leave" };
    }) => void;

    await act(async () => {
      handler({ payload: { type: "over", position: { x: 200, y: 300 } } });
    });
    expect(screen.getByText("拖放上传到 /")).toBeVisible();

    await act(async () => {
      handler({
        payload: {
          type: "drop",
          paths: ["/tmp/cnshell native drop.txt"],
          position: { x: 200, y: 300 },
        },
      });
    });
    await waitFor(() =>
      expect(enqueue).toHaveBeenCalledWith({
        sessionId: "one",
        direction: "upload",
        source: "/tmp/cnshell native drop.txt",
        destination: "/cnshell native drop.txt",
        conflictPolicy: "ask",
      }),
    );
    expect(screen.queryByText("拖放上传到 /")).not.toBeInTheDocument();
  });
});
