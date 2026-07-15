import { act, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { api } from "../../lib/api";
import type {
  TeamDevice,
  TeamMember,
  TeamRelayTerminalEvent,
  TeamRelayTerminalSession,
  TeamRelayWorkspaceBinding,
  TeamTerminalInvitation,
  TeamWorkspace,
  TerminalSession,
} from "../../types";
import { TeamTerminalCenter } from "./TeamTerminalCenter";

const terminalMock = vi.hoisted(() => {
  const dataHandlers: Array<(data: string) => void> = [];
  const writes: Uint8Array[] = [];
  return {
    dataHandlers,
    writes,
    Terminal: vi.fn(function (options: Record<string, unknown>) {
      return {
        options: { ...options },
        loadAddon: vi.fn(),
        open: vi.fn(),
        focus: vi.fn(),
        write: vi.fn((value: Uint8Array) => writes.push(value)),
        dispose: vi.fn(),
        onData: vi.fn((handler: (data: string) => void) => {
          dataHandlers.push(handler);
          return { dispose: vi.fn() };
        }),
      };
    }),
    FitAddon: vi.fn(function () {
      return { fit: vi.fn() };
    }),
  };
});

vi.mock("@xterm/xterm", () => ({ Terminal: terminalMock.Terminal }));
vi.mock("@xterm/addon-fit", () => ({ FitAddon: terminalMock.FitAddon }));

const workspace: TeamWorkspace = {
  id: "11111111-1111-4111-8111-111111111111",
  name: "Ops",
  localMemberId: "22222222-2222-4222-8222-222222222222",
  localRole: "owner",
  keyEpoch: 2,
  createdAt: "2026-07-15T00:00:00Z",
  updatedAt: "2026-07-15T00:00:00Z",
};

const binding: TeamRelayWorkspaceBinding = {
  workspaceId: workspace.id,
  profileId: "33333333-3333-4333-8333-333333333333",
  profileName: "Relay",
  baseUrl: "https://relay.example.com/",
  accountId: "44444444-4444-4444-8444-444444444444",
  deviceSessionExpiresAt: "2099-07-15T00:15:00Z",
  lastSyncedAt: workspace.updatedAt,
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

const operator: TeamMember = {
  ...owner,
  id: "55555555-5555-4555-8555-555555555555",
  displayName: "Bob",
  role: "operator",
};

const localDevice: TeamDevice = {
  id: "66666666-6666-4666-8666-666666666666",
  workspaceId: workspace.id,
  memberId: owner.id,
  name: "Alice Mac",
  encryptionPublicKey: `x25519:${"a".repeat(43)}`,
  signingPublicKey: `ed25519:${"b".repeat(43)}`,
  fingerprint: `sha256:${"c".repeat(64)}`,
  isLocal: true,
  status: "active",
  createdAt: workspace.createdAt,
  updatedAt: workspace.updatedAt,
  revokedAt: null,
};

const remoteDevice: TeamDevice = {
  ...localDevice,
  id: "77777777-7777-4777-8777-777777777777",
  memberId: operator.id,
  name: "Bob Mac",
  isLocal: false,
};

const sshSession: TerminalSession = {
  id: "88888888-8888-4888-8888-888888888888",
  connectionId: "99999999-9999-4999-8999-999999999999",
  sessionType: "terminal",
  title: "Production",
  status: "online",
  startedAt: workspace.createdAt,
  lastError: null,
};

const hostRoom: TeamRelayTerminalSession = {
  roomId: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  workspaceId: workspace.id,
  mode: "host",
  terminalSessionId: sshSession.id,
  localMemberId: owner.id,
  localDeviceId: localDevice.id,
  status: "online",
  lastError: null,
  lastOutputSequence: 2,
  participants: [
    {
      memberId: owner.id,
      deviceId: localDevice.id,
      role: "owner",
      joinedAt: workspace.createdAt,
    },
    {
      memberId: operator.id,
      deviceId: remoteDevice.id,
      role: "operator",
      joinedAt: workspace.createdAt,
    },
  ],
  controlLease: null,
  createdAt: workspace.createdAt,
};

const participantRoom: TeamRelayTerminalSession = {
  ...hostRoom,
  mode: "participant",
  terminalSessionId: null,
  localMemberId: operator.id,
  localDeviceId: remoteDevice.id,
};

const invitation: TeamTerminalInvitation = {
  schemaVersion: 1,
  roomId: participantRoom.roomId,
  workspaceId: workspace.id,
  keyEpoch: 2,
  hostMemberId: owner.id,
  hostDeviceId: localDevice.id,
  recipientMemberId: operator.id,
  recipientDeviceId: remoteDevice.id,
  ephemeralPublicKey: `x25519:${"d".repeat(43)}`,
  keyNonce: "e".repeat(16),
  wrappedRoomKey: "f".repeat(64),
  replayFromSequence: 0,
  nextInputSequence: 1,
  createdAt: workspace.createdAt,
  expiresAt: "2099-07-15T00:05:00Z",
  signature: `ed25519:${"g".repeat(86)}`,
};

let relayEventHandler: ((event: TeamRelayTerminalEvent) => void) | null = null;
let terminalOutputHandler:
  ((output: { sessionId: string; dataBase64: string }) => void) | null = null;

function mockBase(
  relayRooms: TeamRelayTerminalSession[] = [],
  pending: TeamTerminalInvitation[] = [],
) {
  vi.spyOn(api, "listTeamWorkspaces").mockResolvedValue([workspace]);
  vi.spyOn(api, "listTeamRelayBindings").mockResolvedValue([binding]);
  vi.spyOn(api, "listTeamRelayTerminalSessions").mockResolvedValue(relayRooms);
  vi.spyOn(api, "listTeamRelayTerminalInvitations").mockResolvedValue(
    pending.map((item) => ({ roomId: item.roomId, invitation: item })),
  );
  vi.spyOn(api, "listTeamMembers").mockResolvedValue([owner, operator]);
  vi.spyOn(api, "listTeamDevices").mockResolvedValue([
    localDevice,
    remoteDevice,
  ]);
  vi.spyOn(api, "onTeamRelayTerminalEvent").mockImplementation(
    async (handler) => {
      relayEventHandler = handler;
      return () => undefined;
    },
  );
  vi.spyOn(api, "onTerminalOutput").mockImplementation(async (handler) => {
    terminalOutputHandler = handler;
    return () => undefined;
  });
}

function renderCenter() {
  return render(
    <TeamTerminalCenter
      open
      onClose={vi.fn()}
      sessions={[sshSession]}
      activeSessionId={sshSession.id}
      onError={vi.fn()}
    />,
  );
}

describe("TeamTerminalCenter", () => {
  beforeEach(() => {
    vi.restoreAllMocks();
    relayEventHandler = null;
    terminalOutputHandler = null;
    terminalMock.dataHandlers.length = 0;
    terminalMock.writes.length = 0;
  });

  it("starts an online room from the active SSH session", async () => {
    const user = userEvent.setup();
    mockBase();
    const start = vi
      .spyOn(api, "startTeamRelayTerminalRoom")
      .mockResolvedValue(hostRoom);
    renderCenter();

    await user.click(await screen.findByRole("button", { name: "开始共享" }));

    await waitFor(() =>
      expect(start).toHaveBeenCalledWith(workspace.id, sshSession.id),
    );
    expect(await screen.findByText("主持中")).toBeInTheDocument();
  });

  it("routes invitations only to explicitly selected devices", async () => {
    const user = userEvent.setup();
    const uninvitedRoom = {
      ...hostRoom,
      participants: [hostRoom.participants[0]],
    };
    mockBase([uninvitedRoom]);
    const invite = vi
      .spyOn(api, "inviteTeamRelayTerminalDevices")
      .mockResolvedValue([invitation]);
    renderCenter();

    await user.click(
      await screen.findByRole("checkbox", { name: "邀请 Bob Mac" }),
    );
    await user.click(screen.getByRole("button", { name: "发送邀请" }));

    await waitFor(() =>
      expect(invite).toHaveBeenCalledWith(hostRoom.roomId, [remoteDevice.id]),
    );
  });

  it("forwards host terminal output through the room queue", async () => {
    mockBase([hostRoom]);
    const publish = vi
      .spyOn(api, "publishTeamRelayTerminalOutput")
      .mockResolvedValue();
    renderCenter();

    expect(await screen.findByText("主持中")).toBeInTheDocument();
    await waitFor(() => expect(terminalOutputHandler).not.toBeNull());
    act(() =>
      terminalOutputHandler?.({
        sessionId: sshSession.id,
        dataBase64: btoa("ready\r\n"),
      }),
    );

    await waitFor(() =>
      expect(publish).toHaveBeenCalledWith(hostRoom.roomId, btoa("ready\r\n")),
    );
  });

  it("accepts a pending invitation and opens the participant terminal", async () => {
    const user = userEvent.setup();
    mockBase([], [invitation]);
    const accept = vi
      .spyOn(api, "acceptTeamRelayTerminalInvitation")
      .mockResolvedValue(participantRoom);
    renderCenter();

    await user.click(await screen.findByRole("tab", { name: /加入/ }));
    await user.click(await screen.findByRole("button", { name: "接受" }));

    await waitFor(() => expect(accept).toHaveBeenCalledWith(invitation));
    expect(await screen.findByText("只读")).toBeInTheDocument();
    expect(screen.getByLabelText("共享终端输出")).toBeInTheDocument();
  });

  it("keeps viewers read-only until a matching lease enables serialized input", async () => {
    const user = userEvent.setup();
    mockBase([participantRoom]);
    const send = vi
      .spyOn(api, "sendTeamRelayTerminalInput")
      .mockResolvedValue();
    renderCenter();

    await user.click(await screen.findByRole("tab", { name: /加入/ }));
    expect(await screen.findByText("只读")).toBeInTheDocument();
    const controlled = {
      ...participantRoom,
      controlLease: {
        id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        memberId: operator.id,
        deviceId: remoteDevice.id,
        expiresAt: "2099-07-15T00:10:00Z",
        generation: 1,
      },
    };
    await waitFor(() => expect(relayEventHandler).not.toBeNull());
    act(() =>
      relayEventHandler?.({
        roomId: controlled.roomId,
        kind: "control",
        session: controlled,
        sequence: null,
        dataBase64: null,
      }),
    );

    expect(await screen.findByText("可控制")).toBeInTheDocument();
    act(() => terminalMock.dataHandlers.at(-1)?.("ls\r"));
    await waitFor(() =>
      expect(send).toHaveBeenCalledWith(
        controlled.roomId,
        controlled.controlLease.id,
        1,
        btoa("ls\r"),
      ),
    );
  });

  it("lets the host grant and revoke a bounded control lease", async () => {
    const user = userEvent.setup();
    mockBase([hostRoom]);
    const controlled = {
      ...hostRoom,
      controlLease: {
        id: "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb",
        memberId: operator.id,
        deviceId: remoteDevice.id,
        expiresAt: "2099-07-15T00:10:00Z",
        generation: 1,
      },
    };
    const grant = vi
      .spyOn(api, "grantTeamRelayTerminalControl")
      .mockResolvedValue(controlled);
    const revoke = vi
      .spyOn(api, "revokeTeamRelayTerminalControl")
      .mockResolvedValue(hostRoom);
    renderCenter();

    await user.click(await screen.findByRole("button", { name: "授予控制" }));
    await waitFor(() =>
      expect(grant).toHaveBeenCalledWith(hostRoom.roomId, remoteDevice.id, 60),
    );
    await user.click(await screen.findByRole("button", { name: "撤销控制" }));
    await waitFor(() => expect(revoke).toHaveBeenCalledWith(hostRoom.roomId));
  });
});
