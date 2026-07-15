import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type { TeamRelayProfile, TeamRelayWorkspaceBinding, TeamWorkspace } from "../../types";
import { TeamRelaySettings } from "./TeamRelaySettings";

const workspace: TeamWorkspace = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "Ops",
  localMemberId: "22222222-2222-4222-8222-222222222222",
  localRole: "owner",
  keyEpoch: 2,
  createdAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:00Z",
};

const profile: TeamRelayProfile = {
  id: "33333333-3333-4333-8333-333333333333",
  name: "CNshell Relay",
  baseUrl: "https://relay.example.com/",
  accountId: null,
  accountEmail: null,
  hasAccountSession: false,
  accountSessionExpiresAt: null,
  createdAt: workspace.createdAt,
  updatedAt: workspace.updatedAt,
};

const binding: TeamRelayWorkspaceBinding = {
  workspaceId: workspace.id,
  profileId: profile.id,
  profileName: profile.name,
  baseUrl: profile.baseUrl,
  accountId: "44444444-4444-4444-8444-444444444444",
  deviceSessionExpiresAt: "2026-07-15T00:15:00Z",
  lastSyncedAt: "2026-07-15T00:00:00Z",
};

describe("TeamRelaySettings", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
  });

  it("submits account credentials only through the login command", async () => {
    const user = userEvent.setup();
    vi.spyOn(api, "listTeamRelayProfiles").mockResolvedValue([profile]);
    const login = vi.spyOn(api, "loginTeamRelayAccount").mockResolvedValue({
      ...profile,
      accountId: binding.accountId,
      accountEmail: "alice@example.com",
      hasAccountSession: true,
      accountSessionExpiresAt: "2026-07-15T00:10:00Z",
    });
    render(<TeamRelaySettings workspaces={[workspace]} workspaceId={workspace.id} binding={null} canManageMembers onChanged={vi.fn()} onError={vi.fn()}/>);

    await user.type(await screen.findByLabelText("邮箱"), "alice@example.com");
    await user.type(screen.getByLabelText("密码"), "correct horse battery staple");
    await user.click(screen.getByRole("button", { name: "登录" }));
    await waitFor(() => expect(login).toHaveBeenCalledWith({
      profileId: profile.id,
      email: "alice@example.com",
      password: "correct horse battery staple",
      displayName: null,
    }));
  });

  it("creates a bounded online invitation for a connected workspace", async () => {
    const user = userEvent.setup();
    const onlineProfile = {
      ...profile,
      accountId: binding.accountId,
      accountEmail: "alice@example.com",
      hasAccountSession: true,
      accountSessionExpiresAt: "2026-07-15T00:10:00Z",
    };
    vi.spyOn(api, "listTeamRelayProfiles").mockResolvedValue([onlineProfile]);
    const create = vi.spyOn(api, "createTeamRelayInvitation").mockResolvedValue({
      invitationId: "55555555-5555-4555-8555-555555555555",
      token: "a".repeat(43),
      memberId: "66666666-6666-4666-8666-666666666666",
      email: "bob@example.com",
      role: "operator",
      expiresAt: "2026-07-16T00:00:00Z",
    });
    render(<TeamRelaySettings workspaces={[workspace]} workspaceId={workspace.id} binding={binding} canManageMembers onChanged={vi.fn()} onError={vi.fn()}/>);

    await user.type(await screen.findByLabelText("成员邮箱"), "bob@example.com");
    await user.selectOptions(screen.getByLabelText("角色"), "operator");
    await user.click(screen.getByRole("button", { name: "创建邀请" }));
    await waitFor(() => expect(create).toHaveBeenCalledWith({
      workspaceId: workspace.id,
      email: "bob@example.com",
      role: "operator",
    }));
    expect(await screen.findByText("a".repeat(43))).toBeInTheDocument();
  });
});
