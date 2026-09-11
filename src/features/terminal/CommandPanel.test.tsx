import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { CommandPanel } from "./CommandPanel";

const session: TerminalSession = {
  id: "session-1",
  connectionId: "connection-1",
  sessionType: "terminal",
  title: "测试",
  status: "online",
  startedAt: "",
  lastError: null,
};

describe("CommandPanel", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "listSnippets").mockResolvedValue([]);
    vi.spyOn(api, "listHistorySummary").mockResolvedValue([]);
    vi.spyOn(api, "addHistory").mockResolvedValue();
    vi.spyOn(api, "terminalInput").mockResolvedValue();
    vi.spyOn(api, "saveSnippet").mockImplementation(async (snippet) => snippet);
  });

  it("collects template values and previews an injection-safe command", async () => {
    vi.spyOn(api, "listSnippets").mockResolvedValue([
      {
        id: "restart",
        name: "重启服务",
        command: "systemctl restart {{service}}",
        description: "",
        tags: [],
        sortOrder: 0,
      },
    ]);
    const user = userEvent.setup();
    render(<CommandPanel session={session} onError={vi.fn()} />);
    await user.click(
      await screen.findByRole("button", { name: "重启服务，填入命令" }),
    );
    expect(screen.getByRole("combobox", { name: "智能命令输入" })).toHaveValue(
      "systemctl restart {{service}}",
    );
    expect(api.terminalInput).not.toHaveBeenCalled();
    await user.click(
      screen.getByRole("button", { name: "执行快捷命令 重启服务" }),
    );
    const parameter = screen.getByRole("textbox", { name: "命令参数 service" });
    await user.type(parameter, "nginx; rm -rf /");
    expect(
      screen.getByText("systemctl restart 'nginx; rm -rf /'", {
        selector: "code",
      }),
    ).toBeInTheDocument();
    await user.click(
      within(screen.getByRole("dialog", { name: "填写命令参数" })).getByRole(
        "button",
        { name: "执行" },
      ),
    );
    await waitFor(() =>
      expect(api.terminalInput).toHaveBeenCalledWith(
        session.id,
        "systemctl restart 'nginx; rm -rf /'\n",
      ),
    );
  });

  it("requires explicit confirmation before a high-risk command", async () => {
    const confirmMock = vi.spyOn(window, "confirm").mockReturnValue(false);
    const user = userEvent.setup();
    render(<CommandPanel session={session} onError={vi.fn()} />);
    const input = screen.getByRole("combobox", { name: "智能命令输入" });
    await user.type(input, "rm -rf /{Enter}");
    expect(confirmMock).toHaveBeenCalled();
    expect(api.terminalInput).not.toHaveBeenCalled();
    confirmMock.mockReturnValue(true);
    await user.type(input, "{Enter}");
    await waitFor(() =>
      expect(api.terminalInput).toHaveBeenCalledWith(session.id, "rm -rf /\n"),
    );
  });

  it("saves a grouped and pinned shortcut without executing it", async () => {
    const user = userEvent.setup();
    render(<CommandPanel session={session} onError={vi.fn()} />);
    const input = screen.getByRole("combobox", { name: "智能命令输入" });
    await user.type(input, "systemctl status nginx");
    await user.click(screen.getByRole("button", { name: "保存" }));
    const dialog = screen.getByRole("dialog", { name: "保存快捷命令" });
    await user.type(
      within(dialog).getByRole("textbox", { name: "名称" }),
      "Nginx 状态",
    );
    await user.type(
      within(dialog).getByRole("textbox", { name: "分组" }),
      "服务管理",
    );
    await user.click(
      within(dialog).getByRole("checkbox", { name: "置顶显示" }),
    );
    await user.click(within(dialog).getByRole("button", { name: "保存" }));

    await waitFor(() =>
      expect(api.saveSnippet).toHaveBeenCalledWith(
        expect.objectContaining({
          name: "Nginx 状态",
          command: "systemctl status nginx",
          tags: ["服务管理", "cnshell:pinned"],
        }),
      ),
    );
    expect(api.terminalInput).not.toHaveBeenCalled();
  });

  it("shows history frequency and latest use time", async () => {
    vi.mocked(api.listHistorySummary).mockResolvedValue([
      {
        command: "uptime",
        count: 3,
        lastUsedAt: "2026-09-11T08:30:00+08:00",
      },
    ]);
    render(<CommandPanel session={session} onError={vi.fn()} />);
    expect(await screen.findByText("×3")).toBeVisible();
    expect(
      document.querySelector('time[datetime="2026-09-11T08:30:00+08:00"]'),
    ).not.toBeNull();
  });
});
