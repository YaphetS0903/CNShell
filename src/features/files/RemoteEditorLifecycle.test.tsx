import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { beforeEach, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { canLeaveEditor } from "../../lib/editor-lifecycle";
import { useAppStore } from "../../store/app-store";
import { RemoteEditorHost } from "./RemoteEditorHost";
import { readTextDraft, writeTextDraft } from "./text-drafts";

const picker = vi.hoisted(() => vi.fn());
vi.mock("@tauri-apps/plugin-dialog", () => ({ open: picker }));

vi.mock("./RemoteCodeEditor", () => ({
  RemoteCodeEditor: ({
    value,
    onChange,
  }: {
    value: string;
    onChange: (value: string) => void;
  }) => (
    <textarea
      aria-label="远程文本内容"
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));

const target = {
  sessionId: "ssh-1",
  connectionId: "server-1",
  path: "/etc/config.txt",
};
beforeEach(() => {
  vi.restoreAllMocks();
  picker.mockReset();
  localStorage.clear();
  useAppStore.setState({
    remoteEditor: null,
    sessions: [],
    activeSessionId: "ssh-1",
    activePanel: "files",
  });
  vi.spyOn(window, "confirm").mockReturnValue(false);
  vi.spyOn(api, "openText").mockResolvedValue({
    content: "base",
    modifiedAt: 10,
  });
  vi.spyOn(api, "saveText").mockResolvedValue(undefined);
});

async function openEditor(next = target) {
  act(() => useAppStore.getState().openTextEditor(next));
  const host = render(<RemoteEditorHost />);
  await screen.findByLabelText("远程文本内容");
  return host;
}

function edit(value = "local draft") {
  fireEvent.change(screen.getByLabelText("远程文本内容"), {
    target: { value },
  });
}

it("keeps the editor and its save destination when switching sessions and tool panels", async () => {
  await openEditor();
  const editor = screen.getByLabelText("远程文本内容");
  edit();
  act(() => {
    useAppStore.getState().setActiveSession("ssh-2");
    useAppStore.getState().setPanel("transfers");
  });
  expect(screen.getByLabelText("远程文本内容")).toBe(editor);
  expect(editor).toHaveValue("local draft");
  expect(api.openText).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: "原子保存" }));
  await waitFor(() =>
    expect(api.saveText).toHaveBeenCalledWith(
      "ssh-1",
      target.path,
      "local draft",
      10,
    ),
  );
});

it("keeps a cancelled session close open and restores its draft in a new SSH session", async () => {
  const host = await openEditor();
  edit();
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(false),
  );
  expect(useAppStore.getState().remoteEditor).toEqual(target);
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("another-session")).toBe(
      true,
    ),
  );
  expect(useAppStore.getState().remoteEditor).toEqual(target);
  vi.mocked(window.confirm).mockReturnValue(true);
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(true),
  );
  expect(screen.queryByLabelText("远程文本内容")).not.toBeInTheDocument();
  host.unmount();
  await openEditor({ ...target, sessionId: "ssh-reconnected" });
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("local draft");
  expect(screen.getByRole("status")).toHaveTextContent("已恢复本地草稿");
});

it("does not replace the current document when leaving is cancelled", async () => {
  await openEditor();
  edit();
  act(() =>
    useAppStore.getState().openTextEditor({ ...target, path: "/another.txt" }),
  );
  expect(useAppStore.getState().remoteEditor).toEqual(target);
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("local draft");
});

it("recovers after an unmount and asks for conflict resolution if the remote changed", async () => {
  const host = await openEditor();
  edit();
  host.unmount();
  vi.mocked(api.openText).mockResolvedValue({
    content: "remote revision",
    modifiedAt: 11,
  });
  render(<RemoteEditorHost />);
  expect(
    await screen.findByText("远端文件在编辑期间发生变化"),
  ).toBeInTheDocument();
  expect(screen.getByText("local draft")).toBeInTheDocument();
  expect(screen.getByText("remote revision")).toBeInTheDocument();
  expect(api.saveText).not.toHaveBeenCalled();
  fireEvent.click(screen.getByRole("button", { name: "使用远端版本" }));
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("remote revision");
  expect(readTextDraft(target.connectionId, target.path)).toBeNull();
});

it("isolates drafts by connection and clears one only after an explicit discard", async () => {
  writeTextDraft(target.connectionId, target.path, {
    content: "server one draft",
    base: "base",
    modifiedAt: 10,
  });
  const other = await openEditor({ ...target, connectionId: "server-2" });
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("base");
  other.unmount();
  await openEditor();
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("server one draft");
  vi.mocked(window.confirm).mockReturnValue(true);
  fireEvent.click(screen.getAllByRole("button", { name: "关闭" }).at(-1)!);
  expect(readTextDraft(target.connectionId, target.path)).toBeNull();
});

it("preserves edits made during a save, including their updated draft baseline", async () => {
  let finish!: () => void;
  vi.mocked(api.saveText).mockReturnValueOnce(
    new Promise<void>((resolve) => {
      finish = resolve;
    }),
  );
  await openEditor();
  edit("first edit");
  fireEvent.click(screen.getByRole("button", { name: "原子保存" }));
  edit("second edit");
  act(() => {
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(false);
    expect(canLeaveEditor()).toBe(false);
  });
  expect(window.confirm).not.toHaveBeenCalled();
  vi.mocked(api.openText).mockResolvedValue({
    content: "first edit",
    modifiedAt: 11,
  });
  await act(async () => finish());
  expect(readTextDraft(target.connectionId, target.path)).toEqual({
    content: "second edit",
    base: "first edit",
    modifiedAt: 11,
  });
  vi.mocked(api.openText).mockResolvedValue({
    content: "second edit",
    modifiedAt: 12,
  });
  fireEvent.click(screen.getByRole("button", { name: "原子保存" }));
  await waitFor(() =>
    expect(readTextDraft(target.connectionId, target.path)).toBeNull(),
  );
});

it("blocks leaving when local draft storage is full until the user saves or discards", async () => {
  await openEditor();
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("quota", "QuotaExceededError");
  });
  edit();
  expect(screen.getByRole("alert")).toHaveTextContent("本地草稿保存失败");
  act(() => expect(canLeaveEditor()).toBe(false));
  expect(window.confirm).not.toHaveBeenCalled();
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("local draft");
  vi.mocked(window.confirm).mockReturnValue(true);
  fireEvent.click(screen.getAllByRole("button", { name: "关闭" }).at(-1)!);
  expect(useAppStore.getState().remoteEditor).toBeNull();
});

it("recovers the local draft even if reconnecting cannot read the remote file", async () => {
  writeTextDraft(target.connectionId, target.path, {
    content: "offline work",
    base: "base",
    modifiedAt: 10,
  });
  vi.mocked(api.openText).mockRejectedValue(new Error("connection closed"));
  await openEditor();
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("offline work");
  expect(screen.getByText("connection closed")).toBeInTheDocument();
  expect(readTextDraft(target.connectionId, target.path)?.content).toBe(
    "offline work",
  );
});

it("does not erase an unreadable stored draft without an explicit discard", async () => {
  const key = `cnshell-text-draft-v1:${JSON.stringify([target.connectionId, target.path])}`;
  localStorage.setItem(key, "unreadable old draft");
  await openEditor();
  expect(screen.getByRole("alert")).toHaveTextContent("原草稿仍保留");
  expect(localStorage.getItem(key)).toBe("unreadable old draft");
  act(() => expect(canLeaveEditor()).toBe(false));
  fireEvent.click(screen.getAllByRole("button", { name: "关闭" }).at(-1)!);
  expect(useAppStore.getState().remoteEditor).toEqual(target);
  vi.mocked(window.confirm).mockReturnValue(true);
  fireEvent.click(screen.getAllByRole("button", { name: "关闭" }).at(-1)!);
  expect(localStorage.getItem(key)).toBeNull();
});

it("waits for the external application picker before permitting session closure", async () => {
  let finish!: (application: string | null) => void;
  picker.mockReturnValue(
    new Promise<string | null>((resolve) => {
      finish = resolve;
    }),
  );
  await openEditor();
  fireEvent.click(screen.getByRole("button", { name: "选择应用" }));
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(false),
  );
  await act(async () => finish(null));
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(true),
  );
});

it("blocks session closure during external-edit operations and while a copy awaits import", async () => {
  let finish!: (
    value: Awaited<ReturnType<typeof api.startExternalEdit>>,
  ) => void;
  vi.spyOn(api, "startExternalEdit").mockReturnValue(
    new Promise((resolve) => {
      finish = resolve;
    }),
  );
  vi.spyOn(api, "discardExternalEdit").mockResolvedValue(undefined);
  await openEditor();
  fireEvent.click(screen.getByRole("button", { name: "外部应用" }));
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(false),
  );
  await act(async () =>
    finish({
      id: "copy",
      remotePath: target.path,
      localPath: "/tmp/copy",
      expectedModifiedAt: 10,
      startedAt: "now",
    }),
  );
  act(() => expect(canLeaveEditor()).toBe(false));
  expect(
    screen.getByText("外部编辑副本尚未回传，请先回传或放弃副本"),
  ).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "放弃" }));
  await waitFor(() =>
    expect(screen.queryByText("外部编辑副本已打开")).not.toBeInTheDocument(),
  );
  act(() =>
    expect(useAppStore.getState().prepareCloseSession("ssh-1")).toBe(true),
  );
});
