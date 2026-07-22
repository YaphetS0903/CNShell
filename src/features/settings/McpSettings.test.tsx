import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { ConnectionProfile, McpApprovalRule, McpClient, McpClientConfig, McpStatus } from "../../types";
import { McpSettings } from "./McpSettings";

const status: McpStatus = {
  enabled: true,
  running: true,
  address: "127.0.0.1:43100",
  generation: "generation",
  clientCount: 1,
  sessionCount: 0,
  pendingApprovalCount: 0,
  message: "MCP Broker 仅监听本机",
};

const client: McpClient = {
  id: "client-1", name: "Codex", status: "active",
  executablePath: null, executableSha256: null,
  createdAt: "2026-07-21T00:00:00Z", updatedAt: "2026-07-21T00:00:00Z",
  lastUsedAt: null, revokedAt: null,
  showHostnames: true,
  connectionIds: ["server-1"], tools: ["cnshell_list_connections"],
  remoteRoot: "/srv/app",
};

const connection: ConnectionProfile = {
  id: "server-1", folderId: null, protocol: "ssh", name: "测试服务器",
  host: "example.test", port: 22, username: "ubuntu", authType: "sshAgent",
  privateKeyPath: null, certificatePath: null, hostKeyPolicy: "strict", note: "",
  tags: [], encoding: "UTF-8", startupCommand: null, proxyId: null, environment: {},
  hasCredential: false, createdAt: "", updatedAt: "", lastConnectedAt: null,
};

const approvalRule: McpApprovalRule = {
  id: "10000000-0000-4000-8000-000000000001",
  clientId: "client-1",
  connectionId: "server-1",
  connectionName: "测试服务器",
  tool: "cnshell_run_command",
  targetSummary: `command:sha256:${"a".repeat(64)}`,
  createdAt: "2026-07-22T00:00:00Z",
  lastUsedAt: null,
};

const clientConfig: McpClientConfig = {
  clientId: client.id,
  clientName: client.name,
  command: "cnshell-mcp",
  args: [],
  codexToml: "[mcp_servers.cnshell]",
  json: '{"mcpServers":{"cnshell":{}}}',
};

describe("McpSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "isDesktop").mockReturnValue(true);
    vi.spyOn(api, "mcpStatus").mockResolvedValue(status);
    vi.spyOn(api, "mcpListClients").mockResolvedValue([client]);
    vi.spyOn(api, "mcpListAudit").mockResolvedValue([]);
    vi.spyOn(api, "mcpListLocalGrants").mockResolvedValue([]);
    vi.spyOn(api, "mcpListApprovalRules").mockResolvedValue([]);
    vi.spyOn(api, "mcpClientConfig").mockResolvedValue(clientConfig);
  });

  it("saves hostname visibility only for the selected client", async () => {
    const save = vi.spyOn(api, "mcpSaveClientGrants").mockImplementation(async () => client);
    const user = userEvent.setup();
    render(<McpSettings connections={[connection]} onError={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: /^Codex/ }));
    const privacy = await screen.findByRole("checkbox", { name: "允许此客户端看到主机地址和用户名" });
    expect(privacy).toBeChecked();
    await user.click(privacy);
    await user.click(screen.getByRole("button", { name: "保存授权并生成配置" }));
    expect(save).toHaveBeenCalledWith(expect.objectContaining({
      clientId: "client-1",
      showHostnames: false,
      remoteRoot: "/srv/app",
    }));
  });

  it("confirms saved grants and brings the generated client configuration into view", async () => {
    const scrollIntoView = vi.fn();
    Object.defineProperty(HTMLElement.prototype, "scrollIntoView", {
      configurable: true,
      value: scrollIntoView,
    });
    vi.spyOn(api, "mcpSaveClientGrants").mockResolvedValue(client);
    vi.spyOn(api, "mcpListClients").mockResolvedValueOnce([client]).mockResolvedValue([{ ...client }]);
    const user = userEvent.setup();
    render(<McpSettings connections={[connection]} onError={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: /^Codex/ }));
    await user.click(screen.getByRole("button", { name: "保存授权并生成配置" }));

    expect(await screen.findByRole("status")).toHaveTextContent("授权已保存，客户端配置已生成。");
    expect(screen.getByText("客户端配置")).toBeVisible();
    expect(api.mcpClientConfig).toHaveBeenCalledWith(client.id);
    await vi.waitFor(() => expect(scrollIntoView).toHaveBeenCalled());
  });

  it.each(["light", "dark"])("renders controls with the %s theme tokens", async (theme) => {
    document.documentElement.dataset.theme = theme;
    render(<McpSettings connections={[connection]} onError={vi.fn()} />);
    expect(await screen.findByText("Broker 正在运行")).toBeVisible();
    expect(screen.getByRole("button", { name: "刷新" })).toBeEnabled();
    expect(screen.getByRole("checkbox", { name: "启用 MCP" })).toBeChecked();
    delete document.documentElement.dataset.theme;
  });

  it("lists and revokes a saved exact command rule", async () => {
    vi.mocked(api.mcpListApprovalRules).mockResolvedValueOnce([approvalRule]).mockResolvedValue([]);
    const revoke = vi.spyOn(api, "mcpRevokeApprovalRule").mockResolvedValue();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const user = userEvent.setup();
    render(<McpSettings connections={[connection]} onError={vi.fn()} />);

    await user.click(await screen.findByRole("button", { name: /^Codex/ }));
    expect(await screen.findByText(approvalRule.targetSummary)).toBeVisible();
    await user.click(screen.getByRole("button", { name: "撤销精确规则 测试服务器" }));
    expect(revoke).toHaveBeenCalledWith(approvalRule.id);
  });
});
