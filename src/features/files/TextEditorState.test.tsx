import {
  act,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, it, vi } from "vitest";
import { TextEditor } from "./TextEditor";
import { api } from "../../lib/api";

// Exercise document state independently of CodeMirror's DOM implementation.
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

function deferred<T>() {
  let resolve!: (value: T) => void;
  let reject!: (reason: unknown) => void;
  const promise = new Promise<T>((yes, no) => {
    resolve = yes;
    reject = no;
  });
  return { promise, resolve, reject };
}

beforeEach(() => {
  localStorage.clear();
  vi.restoreAllMocks();
  vi.spyOn(api, "openText").mockResolvedValue({
    content: "base",
    modifiedAt: 10,
  });
  vi.spyOn(api, "saveText").mockResolvedValue(undefined);
});

it.each(["button", "escape", "backdrop"])(
  "protects unsaved edits when closing via %s",
  async (action) => {
    const onClose = vi.fn();
    const confirm = vi.spyOn(window, "confirm").mockReturnValue(false);
    render(
      <TextEditor
        sessionId="session-1"
        path="/tmp/config.txt"
        onClose={onClose}
      />,
    );
    fireEvent.change(await screen.findByLabelText("远程文本内容"), {
      target: { value: "unsaved work" },
    });
    const close = () => {
      if (action === "button")
        fireEvent.click(
          screen.getAllByRole("button", { name: "关闭" }).at(-1)!,
        );
      if (action === "escape") fireEvent.keyDown(document, { key: "Escape" });
      if (action === "backdrop")
        fireEvent.mouseDown(screen.getByRole("presentation"));
    };
    close();
    expect(confirm).toHaveBeenCalledWith(expect.stringContaining("未保存"));
    expect(onClose).not.toHaveBeenCalled();
    expect(screen.getByLabelText("远程文本内容")).toHaveValue("unsaved work");
    confirm.mockReturnValue(true);
    close();
    expect(onClose).toHaveBeenCalledOnce();
  },
);

it("keeps edits made during save and saves them using the new baseline", async () => {
  const saving = deferred<void>();
  vi.mocked(api.saveText).mockReturnValueOnce(saving.promise);
  vi.mocked(api.openText)
    .mockResolvedValueOnce({ content: "base", modifiedAt: 10 })
    .mockResolvedValueOnce({ content: "first edit", modifiedAt: 11 })
    .mockResolvedValueOnce({
      content: "second edit while saving",
      modifiedAt: 12,
    });
  const onClose = vi.fn();
  render(
    <TextEditor
      sessionId="session-1"
      path="/tmp/config.txt"
      onClose={onClose}
    />,
  );
  const editor = await screen.findByLabelText("远程文本内容");
  fireEvent.change(editor, { target: { value: "first edit" } });
  await userEvent.click(screen.getByRole("button", { name: "原子保存" }));
  fireEvent.keyDown(document, { key: "Escape" });
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.change(editor, { target: { value: "second edit while saving" } });
  await act(async () => saving.resolve());
  expect(editor).toHaveValue("second edit while saving");
  expect(screen.getByText(/未保存/)).toBeInTheDocument();
  await userEvent.click(screen.getByRole("button", { name: "原子保存" }));
  await waitFor(() =>
    expect(api.saveText).toHaveBeenLastCalledWith(
      "session-1",
      "/tmp/config.txt",
      "second edit while saving",
      11,
    ),
  );
});

it("uses the latest local edits when a pending save reports a conflict", async () => {
  const saving = deferred<void>();
  vi.mocked(api.saveText).mockReturnValueOnce(saving.promise);
  vi.mocked(api.openText)
    .mockResolvedValueOnce({ content: "base", modifiedAt: 10 })
    .mockResolvedValueOnce({ content: "remote change", modifiedAt: 11 });
  render(
    <TextEditor
      sessionId="session-1"
      path="/tmp/config.txt"
      onClose={vi.fn()}
    />,
  );
  const editor = await screen.findByLabelText("远程文本内容");
  fireEvent.change(editor, { target: { value: "first edit" } });
  await userEvent.click(screen.getByRole("button", { name: "原子保存" }));
  fireEvent.change(editor, { target: { value: "latest edit" } });
  await act(async () => saving.reject(new Error("远端文件已被其他程序修改")));
  expect(await screen.findByText("latest edit")).toBeInTheDocument();
  await userEvent.click(
    screen.getByRole("button", { name: "继续编辑本地版本" }),
  );
  expect(screen.getByLabelText("远程文本内容")).toHaveValue("latest edit");
});

it("closes a clean document without a discard prompt", async () => {
  const onClose = vi.fn();
  const confirm = vi.spyOn(window, "confirm");
  render(
    <TextEditor
      sessionId="session-1"
      path="/tmp/config.txt"
      onClose={onClose}
    />,
  );
  await screen.findByLabelText("远程文本内容");
  fireEvent.keyDown(document, { key: "Escape" });
  expect(confirm).not.toHaveBeenCalled();
  expect(onClose).toHaveBeenCalledOnce();
});
