import { act, render, screen, waitFor, within } from "@testing-library/react";
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
    localStorage.clear();
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
      if (path === "/")
        return [directory("home", "/home"), file("notes.txt", "/notes.txt")];
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
    expect(
      within(
        screen.getByRole("navigation", { name: "当前远程路径" }),
      ).getByRole("button", { name: "home" }),
    ).toHaveAttribute("aria-current", "page");

    first.unmount();
    render(<FileManager session={session("one")} />);

    expect(
      within(
        screen.getByRole("navigation", { name: "当前远程路径" }),
      ).getByRole("button", { name: "home" }),
    ).toHaveAttribute("aria-current", "page");
    expect(await screen.findByRole("button", { name: "ubuntu" })).toBeVisible();
    expect(screen.getByRole("button", { name: "折叠 home" })).toBeVisible();
  });

  it("keeps navigation state isolated between SSH sessions", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);
    await user.click(await screen.findByRole("button", { name: "home" }));
    expect(
      within(
        screen.getByRole("navigation", { name: "当前远程路径" }),
      ).getByRole("button", { name: "home" }),
    ).toHaveAttribute("aria-current", "page");

    first.unmount();
    render(<FileManager session={session("two")} />);
    expect(
      within(
        screen.getByRole("navigation", { name: "当前远程路径" }),
      ).getByRole("button", { name: "根目录 /" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("opens and dismisses the selected file action menu", async () => {
    const user = userEvent.setup();
    render(<FileManager session={session("one")} />);

    await user.click(await screen.findByRole("row", { name: /notes\.txt/ }));
    const more = screen.getByRole("button", { name: "更多文件操作" });
    expect(
      screen.queryByRole("button", { name: "编辑文本" }),
    ).not.toBeInTheDocument();

    await user.click(more);
    expect(screen.getByRole("button", { name: "编辑文本" })).toBeVisible();
    await user.click(more);
    expect(
      screen.queryByRole("button", { name: "编辑文本" }),
    ).not.toBeInTheDocument();

    await user.click(more);
    await user.click(screen.getByRole("button", { name: "编辑远程路径" }));
    expect(document.activeElement).toBe(screen.getByLabelText("远程路径"));
    expect(
      screen.queryByRole("button", { name: "编辑文本" }),
    ).not.toBeInTheDocument();

    await user.click(more);
    await user.keyboard("{Escape}");
    expect(
      screen.queryByRole("button", { name: "编辑文本" }),
    ).not.toBeInTheDocument();

    await user.click(more);
    await user.click(screen.getByRole("button", { name: "复制路径" }));
    expect(
      screen.queryByRole("button", { name: "编辑文本" }),
    ).not.toBeInTheDocument();
  });

  it("filters the current directory and reports the visible item count", async () => {
    const user = userEvent.setup();
    render(<FileManager session={session("one")} />);

    expect(
      await screen.findByRole("row", { name: /notes\.txt/ }),
    ).toBeVisible();
    await user.type(
      screen.getByRole("textbox", { name: "筛选当前目录文件" }),
      "notes",
    );

    expect(screen.getByText("1/2")).toBeVisible();
    expect(screen.getByRole("row", { name: /notes\.txt/ })).toBeVisible();
    expect(screen.queryByRole("row", { name: /home/ })).not.toBeInTheDocument();
  });

  it("navigates with breadcrumbs and remembers visible columns", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);
    await user.click(await screen.findByRole("button", { name: "home" }));
    const breadcrumbs = screen.getByRole("navigation", {
      name: "当前远程路径",
    });
    expect(
      within(breadcrumbs).getByRole("button", { name: "home" }),
    ).toHaveAttribute("aria-current", "page");
    await user.click(
      within(breadcrumbs).getByRole("button", { name: "根目录 /" }),
    );
    expect(
      within(breadcrumbs).getByRole("button", { name: "根目录 /" }),
    ).toHaveAttribute("aria-current", "page");

    await user.click(screen.getByLabelText("选择显示列"));
    await user.click(screen.getByRole("checkbox", { name: "类型" }));
    expect(screen.getByRole("table")).toHaveAttribute("aria-colcount", "5");
    expect(
      screen.queryByRole("columnheader", { name: "类型" }),
    ).not.toBeInTheDocument();
    expect(localStorage.getItem("cnshell-file-columns")).not.toContain("kind");

    first.unmount();
    render(<FileManager session={session("one")} />);
    expect(screen.getByRole("table")).toHaveAttribute("aria-colcount", "5");
    expect(
      screen.queryByRole("columnheader", { name: "类型" }),
    ).not.toBeInTheDocument();
  });

  it("edits the path in place and keeps an invalid path available to fix", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "listFiles").mockImplementation(async (_sessionId, path) => {
      if (path === "/missing") throw new Error("目录不存在");
      if (path === "/home") return [directory("ubuntu", "/home/ubuntu")];
      return [directory("home", "/home")];
    });
    render(<FileManager session={session("one")} />);
    await screen.findByRole("button", { name: "home" });

    await user.keyboard("{Meta>}l{/Meta}");
    const input = screen.getByRole("textbox", { name: "远程路径" });
    expect(document.activeElement).toBe(input);
    await user.clear(input);
    await user.type(input, "/missing{Enter}");
    expect(await screen.findByRole("alert")).toHaveTextContent("目录不存在");
    expect(input).toHaveValue("/missing");

    await user.clear(input);
    await user.type(input, "/home{Enter}");
    await waitFor(() =>
      expect(
        within(
          screen.getByRole("navigation", { name: "当前远程路径" }),
        ).getByRole("button", { name: "home" }),
      ).toHaveAttribute("aria-current", "page"),
    );

    await user.click(screen.getByRole("button", { name: "编辑远程路径" }));
    expect(screen.getByLabelText("远程路径")).toHaveValue("/home");
    await user.keyboard("{Escape}");
    expect(screen.queryByLabelText("远程路径")).not.toBeInTheDocument();
  });

  it("opens the remote user's home directory", async () => {
    const user = userEvent.setup();
    const home = vi
      .spyOn(api, "remoteHomeDirectory")
      .mockResolvedValue("/home");
    render(<FileManager session={session("one")} />);

    await user.click(screen.getByRole("button", { name: "用户主目录" }));
    await waitFor(() => expect(home).toHaveBeenCalledWith("one"));
    expect(
      within(
        screen.getByRole("navigation", { name: "当前远程路径" }),
      ).getByRole("button", { name: "home" }),
    ).toHaveAttribute("aria-current", "page");
  });

  it("lets users enlarge the file area text and remembers the choice", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);
    expect(screen.getByTitle("文件区当前字号")).toHaveTextContent("自11px");
    await user.click(screen.getByRole("button", { name: "增大文件区字号" }));
    expect(screen.getByTitle("文件区当前字号")).toHaveTextContent("12px");
    expect(localStorage.getItem("cnshell-files-font-size")).toBe("12");
    first.unmount();
    render(<FileManager session={session("one")} />);
    expect(screen.getByTitle("文件区当前字号")).toHaveTextContent("12px");
  });

  it("resizes file columns from the keyboard and remembers their widths", async () => {
    const user = userEvent.setup();
    const first = render(<FileManager session={session("one")} />);
    const separator = screen.getByRole("separator", {
      name: "调整大小列宽",
    });
    separator.focus();
    await user.keyboard("{ArrowRight}");

    expect(separator).toHaveAttribute("aria-valuenow", "85");
    expect(localStorage.getItem("cnshell-file-column-widths")).toContain(
      '"size":85',
    );
    first.unmount();
    render(<FileManager session={session("one")} />);
    expect(
      screen.getByRole("separator", { name: "调整大小列宽" }),
    ).toHaveAttribute("aria-valuenow", "85");
  });

  it("queues files dropped through Tauri's native desktop event", async () => {
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    const enqueue = vi
      .spyOn(api, "enqueueTransfer")
      .mockResolvedValue({} as never);
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

    await waitFor(() =>
      expect(nativeDrop.onDragDropEvent).toHaveBeenCalledOnce(),
    );
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

  it("shows an actionable directory error and retries without remounting", async () => {
    const listFiles = vi
      .spyOn(api, "listFiles")
      .mockRejectedValue(
        new Error("目录读取超时，已重置 SFTP 文件连接，请重试"),
      );
    const user = userEvent.setup();

    render(<FileManager session={session("one")} />);

    expect(await screen.findByText("无法读取目录")).toBeVisible();
    const retry = screen.getByRole("button", { name: "重试读取目录" });
    const callsBeforeRetry = listFiles.mock.calls.length;
    await user.click(retry);

    await waitFor(() =>
      expect(listFiles.mock.calls.length).toBeGreaterThan(callsBeforeRetry),
    );
    expect(await screen.findByText("无法读取目录")).toBeVisible();
  });

  it("shares an in-flight listing between the main table and directory tree", async () => {
    let resolveRoot: ((files: RemoteFile[]) => void) | undefined;
    const listFiles = vi
      .spyOn(api, "listFiles")
      .mockImplementation((_sessionId, path) => {
        if (path !== "/") return Promise.resolve([]);
        return new Promise<RemoteFile[]>((resolve) => {
          resolveRoot = resolve;
        });
      });

    render(<FileManager session={session("one")} />);

    await waitFor(() => expect(listFiles).toHaveBeenCalledTimes(1));
    resolveRoot?.([directory("home", "/home")]);
    expect(await screen.findByRole("row", { name: /home/ })).toBeVisible();
    expect(
      listFiles.mock.calls.filter(([, target]) => target === "/"),
    ).toHaveLength(1);
  });
});
