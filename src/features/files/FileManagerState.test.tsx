import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import type { RemoteFile, TerminalSession } from "../../types";
import { FileManager } from "./FileManager";

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
});
