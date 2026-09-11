import { render, screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type {
  BatchExecution,
  ConnectionProfile,
  TerminalSession,
} from "../../types";
import { BatchExecutionDialog } from "./BatchExecutionDialog";

const connection = (index: number): ConnectionProfile => ({
  id: `connection-${index}`,
  folderId: null,
  protocol: "ssh",
  name: `主机 ${index}`,
  host: `10.0.0.${index}`,
  port: 22,
  username: "root",
  authType: "sshAgent",
  privateKeyPath: null,
  certificatePath: null,
  hostKeyPolicy: "strict",
  note: "",
  tags: [],
  encoding: "UTF-8",
  startupCommand: null,
  proxyId: null,
  environment: {},
  hasCredential: false,
  createdAt: "",
  updatedAt: "",
  lastConnectedAt: null,
});
const execution: BatchExecution = {
  id: "batch-1",
  command: "uname -a",
  status: "failed",
  createdAt: "",
  targets: [
    {
      connectionId: "connection-1",
      name: "主机 1",
      status: "completed",
      stdout: "Linux",
      stderr: "",
      exitCode: 0,
      durationMs: 12,
      error: null,
    },
    {
      connectionId: "connection-2",
      name: "主机 2",
      status: "failed",
      stdout: "",
      stderr: "denied",
      exitCode: 1,
      durationMs: 18,
      error: null,
    },
  ],
};

describe("BatchExecutionDialog", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    useAppStore.setState({ sessions: [] });
    vi.spyOn(api, "listFolders").mockResolvedValue([
      { id: "production", name: "生产", parentId: null, sortOrder: 0 },
    ]);
    vi.spyOn(api, "onBatchExecution").mockResolvedValue(() => undefined);
    vi.spyOn(api, "startBatch").mockResolvedValue(execution);
    vi.spyOn(api, "terminalInput").mockResolvedValue();
  });

  it("previews 20 targets without collapsing failures into other results", async () => {
    const user = userEvent.setup();
    const items = Array.from({ length: 20 }, (_, index) =>
      connection(index + 1),
    );
    render(
      <BatchExecutionDialog
        connections={items}
        connect={vi.fn()}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );
    await user.click(screen.getByText("选择全部"));
    await user.type(
      screen.getByPlaceholderText("输入要在所有目标执行的命令"),
      "uname -a",
    );
    await user.click(screen.getByRole("button", { name: "预览执行" }));
    expect(screen.getByText(/批量命令会在以下/)).toHaveTextContent(
      "批量命令会在以下 20台主机执行",
    );
    await user.click(screen.getByRole("button", { name: "确认执行" }));
    await waitFor(() =>
      expect(api.startBatch).toHaveBeenCalledWith(
        items.map((item) => item.id),
        "uname -a",
        4,
      ),
    );
    expect(await screen.findByText("1/2 成功")).toBeInTheDocument();
    await user.click(screen.getByRole("button", { name: /主机 2/ }));
    expect(screen.getByText("denied")).toBeInTheDocument();
  });

  it("retries only failed targets", async () => {
    const user = userEvent.setup();
    render(
      <BatchExecutionDialog
        connections={[connection(1), connection(2)]}
        connect={vi.fn()}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );
    await user.click(screen.getByText("选择全部"));
    await user.type(
      screen.getByPlaceholderText("输入要在所有目标执行的命令"),
      "uname -a",
    );
    await user.click(screen.getByRole("button", { name: "预览执行" }));
    await user.click(screen.getByRole("button", { name: "确认执行" }));
    await screen.findByText("1/2 成功");
    vi.mocked(api.startBatch).mockClear();
    await user.click(screen.getByRole("button", { name: "仅重试失败项" }));
    await waitFor(() =>
      expect(api.startBatch).toHaveBeenCalledWith(
        ["connection-2"],
        "uname -a",
        4,
      ),
    );
  });

  it("opens selected sessions and sends synchronized input", async () => {
    const user = userEvent.setup();
    const items = [connection(1), connection(2)];
    const connect = vi.fn(async (profile: ConnectionProfile) => {
      const session: TerminalSession = {
        id: `session-${profile.id}`,
        connectionId: profile.id,
        sessionType: "terminal",
        title: profile.name,
        status: "online",
        startedAt: "",
        lastError: null,
      };
      useAppStore.getState().addSession(session);
    });
    render(
      <BatchExecutionDialog
        connections={items}
        connect={connect}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );
    await user.click(screen.getByText("选择全部"));
    await user.click(screen.getByRole("tab", { name: "同步输入" }));
    await user.click(screen.getByRole("button", { name: /建立 2 个同步会话/ }));
    expect(await screen.findByText("2 台主机已就绪")).toBeInTheDocument();
    await user.type(
      screen.getByRole("textbox", { name: "同步命令" }),
      "uptime",
    );
    await user.click(screen.getByRole("button", { name: "发送" }));
    await waitFor(() => expect(api.terminalInput).toHaveBeenCalledTimes(2));
    expect(api.terminalInput).toHaveBeenCalledWith(
      "session-connection-1",
      "uptime\r",
    );
  });

  it("filters targets and warns about duplicate endpoints", async () => {
    const user = userEvent.setup();
    const items = [
      {
        ...connection(1),
        name: "生产一",
        folderId: "production",
        tags: ["生产"],
      },
      {
        ...connection(2),
        name: "生产二",
        folderId: "production",
        tags: ["生产"],
        host: "10.0.0.1",
      },
      { ...connection(3), name: "开发", tags: ["开发"] },
    ];
    const online: TerminalSession = {
      id: "online-1",
      connectionId: items[0].id,
      sessionType: "terminal",
      title: items[0].name,
      status: "online",
      startedAt: "",
      lastError: null,
    };
    useAppStore.setState({ sessions: [online] });
    render(
      <BatchExecutionDialog
        connections={items}
        connect={vi.fn()}
        onClose={vi.fn()}
        onError={vi.fn()}
      />,
    );

    await user.click(screen.getByRole("tab", { name: "同步输入" }));
    expect(
      screen.getByRole("button", { name: "选择目标后建立会话" }),
    ).toBeDisabled();
    expect(screen.queryByText(/建立 0 个同步会话/)).not.toBeInTheDocument();

    await user.click(screen.getByRole("tab", { name: "批量命令" }));
    const folderSelect = screen.getByRole("combobox", {
      name: "按文件夹筛选目标",
    });
    await within(folderSelect).findByRole("option", { name: "生产" });
    await user.selectOptions(
      folderSelect,
      "production",
    );
    await user.selectOptions(
      screen.getByRole("combobox", { name: "按标签筛选目标" }),
      "生产",
    );
    expect(screen.getByRole("checkbox", { name: /生产一/ })).toBeVisible();
    expect(screen.getByRole("checkbox", { name: /生产二/ })).toBeVisible();
    expect(
      screen.queryByRole("checkbox", { name: /开发/ }),
    ).not.toBeInTheDocument();
    expect(screen.getByText(/已连接 · 生产/)).toBeVisible();

    await user.click(screen.getByText("选择筛选结果"));
    expect(screen.getByRole("status")).toHaveTextContent(
      "重复地址：10.0.0.1:22",
    );
  });
});
