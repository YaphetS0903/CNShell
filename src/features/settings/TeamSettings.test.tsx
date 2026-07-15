import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { TeamAuditEvent, TeamMember, TeamPermissionReport, TeamWorkspace } from "../../types";
import { TeamSettings } from "./TeamSettings";

const dialog = vi.hoisted(() => ({ save: vi.fn() }));
vi.mock("@tauri-apps/plugin-dialog", () => dialog);

const workspace: TeamWorkspace = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "Ops",
  localMemberId: "22222222-2222-4222-8222-222222222222",
  localRole: "owner",
  keyEpoch: 2,
  createdAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:00Z",
};
const owner: TeamMember = {
  id: workspace.localMemberId,
  workspaceId: workspace.id,
  displayName: "Alice",
  role: "owner",
  status: "active",
  joinedAt: workspace.createdAt,
  updatedAt: workspace.updatedAt,
  removedAt: null,
};
const viewer: TeamMember = {
  ...owner,
  id: "33333333-3333-4333-8333-333333333333",
  displayName: "Bob",
  role: "viewer",
};
const permissions: TeamPermissionReport = {
  workspaceId: workspace.id,
  memberId: owner.id,
  role: "owner",
  permissions: ["memberManage", "ownerManage", "auditRead", "auditExport"],
};
const audit: TeamAuditEvent = {
  id: "44444444-4444-4444-8444-444444444444",
  workspaceId: workspace.id,
  actorMemberId: owner.id,
  action: "member-added",
  targetType: "member",
  targetId: viewer.id,
  createdAt: workspace.createdAt,
};

describe("TeamSettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    dialog.save.mockReset();
    vi.spyOn(api, "listTeamWorkspaces").mockResolvedValue([workspace]);
    vi.spyOn(api, "getTeamPermissions").mockResolvedValue(permissions);
    vi.spyOn(api, "listTeamMembers").mockResolvedValue([owner, viewer]);
    vi.spyOn(api, "listTeamAudit").mockResolvedValue([audit]);
  });

  it("shows the local role and enforces explicit member mutations", async () => {
    const user = userEvent.setup();
    vi.spyOn(window, "confirm").mockReturnValue(true);
    const saveMember = vi.spyOn(api, "saveTeamMember").mockResolvedValue({ ...viewer, role: "operator" });
    const removeMember = vi.spyOn(api, "removeTeamMember").mockResolvedValue();
    render(<TeamSettings onError={vi.fn()}/>);

    expect(await screen.findByText(/本机角色：Owner · 密钥 epoch 2/)).toBeInTheDocument();
    expect(await screen.findByText(/member-added/)).toBeInTheDocument();
    await user.selectOptions(screen.getByLabelText("修改 Bob 角色"), "operator");
    await waitFor(() => expect(saveMember).toHaveBeenCalledWith(expect.objectContaining({ memberId: viewer.id, role: "operator" })));
    await user.click(screen.getByRole("button", { name: "移除 Bob" }));
    await waitFor(() => expect(removeMember).toHaveBeenCalledWith(workspace.id, viewer.id));
  });

  it("creates a local owner workspace", async () => {
    const user = userEvent.setup();
    const create = vi.spyOn(api, "createTeamWorkspace").mockResolvedValue(workspace);
    render(<TeamSettings onError={vi.fn()}/>);

    await user.type(screen.getByLabelText("团队名称"), "Platform");
    await user.type(screen.getByLabelText("Owner 名称"), "Chen");
    await user.click(screen.getByRole("button", { name: "创建" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith({ name: "Platform", ownerName: "Chen" }));
  });
});
