import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { McpApproval } from "../../types";
import { McpApprovalCenter } from "./McpApprovalCenter";

const approval: McpApproval = {
  id: "approval-1", requestId: "request-1", clientId: "client-1", clientName: "Codex",
  connectionId: "server-1", connectionName: "测试服务器", tool: "cnshell_run_command",
  risk: "high", target: "command:sha256:digest", preview: "sudo systemctl restart nginx",
  canAllowSession: false, canSaveRule: false,
  createdAt: new Date(Date.now() - 1_000).toISOString(),
  expiresAt: new Date(Date.now() + 120_000).toISOString(),
};

describe("McpApprovalCenter", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    vi.spyOn(api, "onMcpApprovalChanged").mockResolvedValue(() => undefined);
  });

  it("supports Escape without losing a pending approval", async () => {
    vi.spyOn(api, "mcpListApprovals").mockResolvedValue([approval]);
    const user = userEvent.setup();
    render(<McpApprovalCenter onError={vi.fn()} />);
    expect(await screen.findByRole("complementary", { name: "MCP 审批中心" })).toBeVisible();
    expect(screen.getByText("sudo systemctl restart nginx")).toBeVisible();
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("complementary", { name: "MCP 审批中心" })).not.toBeInTheDocument();
    expect(screen.getByRole("button", { name: "打开 MCP 审批中心，1 项待处理" })).toBeVisible();
  });

  it("approves exactly once and refreshes the queue", async () => {
    vi.spyOn(api, "mcpListApprovals").mockResolvedValueOnce([approval]).mockResolvedValue([]);
    const approve = vi.spyOn(api, "mcpDecide").mockResolvedValue();
    const user = userEvent.setup();
    render(<McpApprovalCenter onError={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "允许一次" }));
    expect(approve).toHaveBeenCalledOnce();
    expect(approve).toHaveBeenCalledWith("approval-1", "once");
    await waitFor(() => expect(screen.getByText("没有待审批请求")).toBeVisible());
  });

  it("offers exact rules only for low-risk backend-approved commands", async () => {
    vi.spyOn(api, "mcpListApprovals").mockResolvedValue([{ ...approval, risk: "low", canAllowSession: true, canSaveRule: true }]);
    const decide = vi.spyOn(api, "mcpDecide").mockResolvedValue();
    const user = userEvent.setup();
    render(<McpApprovalCenter onError={vi.fn()} />);
    await user.click(await screen.findByRole("button", { name: "保存精确规则" }));
    expect(decide).toHaveBeenCalledWith("approval-1", "persistent");
  });

  it("does not offer exact rules for medium-risk commands", async () => {
    vi.spyOn(api, "mcpListApprovals").mockResolvedValue([{ ...approval, risk: "medium", canAllowSession: true, canSaveRule: true }]);
    render(<McpApprovalCenter onError={vi.fn()} />);
    await screen.findByRole("complementary", { name: "MCP 审批中心" });
    expect(screen.queryByRole("button", { name: "保存精确规则" })).not.toBeInTheDocument();
  });
});
