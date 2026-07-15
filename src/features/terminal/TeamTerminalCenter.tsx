import { FitAddon } from "@xterm/addon-fit";
import { Terminal } from "@xterm/xterm";
import {
  Check,
  Eye,
  Keyboard,
  LoaderCircle,
  RadioTower,
  RefreshCw,
  ShieldOff,
  UserPlus,
  Users,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { createTerminalInputQueue } from "../../lib/terminal-input";
import { useAppStore } from "../../store/app-store";
import type {
  TeamDevice,
  TeamMember,
  TeamRelayTerminalInvitation,
  TeamRelayTerminalSession,
  TeamRelayWorkspaceBinding,
  TeamTerminalParticipant,
  TeamWorkspace,
  TerminalSession,
} from "../../types";
import {
  resolveTerminalPreferences,
  terminalFontFamilies,
  terminalThemes,
} from "./terminal-preferences";

const MAX_OUTPUT_FRAMES = 512;
const MAX_OUTPUT_BYTES = 4 * 1024 * 1024;

interface OutputChunk {
  sequence: number;
  bytes: Uint8Array;
}

interface OutputBuffer {
  chunks: OutputChunk[];
  bytes: number;
}

export function TeamTerminalCenter({
  open,
  onClose,
  sessions,
  activeSessionId,
  onError,
}: {
  open: boolean;
  onClose: () => void;
  sessions: TerminalSession[];
  activeSessionId: string | null;
  onError: (message: string) => void;
}) {
  const [workspaces, setWorkspaces] = useState<TeamWorkspace[]>([]);
  const [bindings, setBindings] = useState<TeamRelayWorkspaceBinding[]>([]);
  const [relaySessions, setRelaySessions] = useState<
    TeamRelayTerminalSession[]
  >([]);
  const [invitations, setInvitations] = useState<TeamRelayTerminalInvitation[]>(
    [],
  );
  const [members, setMembers] = useState<TeamMember[]>([]);
  const [devices, setDevices] = useState<TeamDevice[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [hostSessionId, setHostSessionId] = useState("");
  const [viewMode, setViewMode] = useState<"host" | "participant">("host");
  const [selectedRoomId, setSelectedRoomId] = useState("");
  const [selectedDeviceIds, setSelectedDeviceIds] = useState<string[]>([]);
  const [pendingDeviceIdsByRoom, setPendingDeviceIdsByRoom] = useState<
    Record<string, string[]>
  >({});
  const [leaseDuration, setLeaseDuration] = useState(60);
  const [busy, setBusy] = useState<string | null>(null);
  const [localError, setLocalError] = useState<string | null>(null);
  const [outputVersion, setOutputVersion] = useState(0);
  const [now, setNow] = useState(Date.now());
  const outputBuffers = useRef(new Map<string, OutputBuffer>());
  const relaySessionsRef = useRef(relaySessions);
  const forwardQueues = useRef(new Map<string, Promise<void>>());
  const forwardingErrors = useRef(new Set<string>());
  const reportErrorRef = useRef(onError);
  reportErrorRef.current = onError;
  relaySessionsRef.current = relaySessions;

  const reportError = useCallback((reason: unknown) => {
    const message = errorMessage(reason);
    setLocalError(message);
    reportErrorRef.current(message);
  }, []);

  const refresh = useCallback(
    async (surfaceErrors: boolean) => {
      try {
        const [nextWorkspaces, nextBindings, nextSessions] = await Promise.all([
          api.listTeamWorkspaces(),
          api.listTeamRelayBindings(),
          api.listTeamRelayTerminalSessions(),
        ]);
        const invitationResults = await Promise.allSettled(
          nextBindings.map((binding) =>
            api.listTeamRelayTerminalInvitations(binding.workspaceId),
          ),
        );
        setWorkspaces(nextWorkspaces);
        setBindings(nextBindings);
        setRelaySessions(nextSessions);
        setInvitations(
          invitationResults.flatMap((result) =>
            result.status === "fulfilled" ? result.value : [],
          ),
        );
        setWorkspaceId((current) =>
          nextBindings.some((binding) => binding.workspaceId === current)
            ? current
            : (nextBindings[0]?.workspaceId ?? ""),
        );
      } catch (reason) {
        if (surfaceErrors) reportError(reason);
      }
    },
    [reportError],
  );

  useEffect(() => {
    void refresh(false);
    const timer = window.setInterval(() => void refresh(false), 15_000);
    return () => window.clearInterval(timer);
  }, [refresh]);

  useEffect(() => {
    if (open) void refresh(true);
  }, [open, refresh]);

  useEffect(() => {
    if (!workspaceId) {
      setMembers([]);
      setDevices([]);
      return;
    }
    let active = true;
    void Promise.all([
      api.listTeamMembers(workspaceId),
      api.listTeamDevices(workspaceId),
    ])
      .then(([nextMembers, nextDevices]) => {
        if (!active) return;
        setMembers(nextMembers);
        setDevices(nextDevices);
      })
      .catch(reportError);
    return () => {
      active = false;
    };
  }, [reportError, workspaceId]);

  useEffect(() => {
    const candidates = sessions.filter(
      (session) =>
        session.sessionType === "terminal" && session.status === "online",
    );
    setHostSessionId((current) =>
      candidates.some((session) => session.id === current)
        ? current
        : (candidates.find((session) => session.id === activeSessionId)?.id ??
          candidates[0]?.id ??
          ""),
    );
  }, [activeSessionId, sessions]);

  useEffect(() => {
    const candidates = relaySessions
      .filter(
        (session) =>
          session.workspaceId === workspaceId &&
          session.mode === viewMode &&
          session.status !== "closed",
      )
      .sort((left, right) => right.createdAt.localeCompare(left.createdAt));
    setSelectedRoomId((current) =>
      candidates.some((session) => session.roomId === current)
        ? current
        : (candidates[0]?.roomId ?? ""),
    );
  }, [relaySessions, viewMode, workspaceId]);

  useEffect(() => {
    if (!open) return;
    setNow(Date.now());
    const timer = window.setInterval(() => setNow(Date.now()), 1_000);
    return () => window.clearInterval(timer);
  }, [open]);

  useEffect(() => {
    const unlisten = api.onTeamRelayTerminalEvent((event) => {
      setRelaySessions((current) => upsertSession(current, event.session));
      if (event.kind === "participants") {
        const joined = new Set(
          event.session.participants.map((participant) => participant.deviceId),
        );
        setPendingDeviceIdsByRoom((current) => ({
          ...current,
          [event.roomId]: (current[event.roomId] ?? []).filter(
            (deviceId) => !joined.has(deviceId),
          ),
        }));
      }
      if (
        event.kind !== "output" ||
        event.sequence == null ||
        !event.dataBase64
      )
        return;
      try {
        const bytes = decodeBase64(event.dataBase64);
        const buffer = outputBuffers.current.get(event.roomId) ?? {
          chunks: [],
          bytes: 0,
        };
        const latest = buffer.chunks.at(-1)?.sequence ?? 0;
        if (event.sequence <= latest) return;
        buffer.chunks.push({ sequence: event.sequence, bytes });
        buffer.bytes += bytes.byteLength;
        while (
          buffer.chunks.length > MAX_OUTPUT_FRAMES ||
          buffer.bytes > MAX_OUTPUT_BYTES
        ) {
          const removed = buffer.chunks.shift();
          if (!removed) break;
          buffer.bytes -= removed.bytes.byteLength;
        }
        outputBuffers.current.set(event.roomId, buffer);
        setOutputVersion((value) => value + 1);
      } catch (reason) {
        reportError(reason);
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [reportError]);

  useEffect(() => {
    const unlisten = api.onTerminalOutput((output) => {
      const targets = relaySessionsRef.current.filter(
        (session) =>
          session.mode === "host" &&
          session.terminalSessionId === output.sessionId &&
          !["closed", "failed"].includes(session.status),
      );
      for (const room of targets) {
        const previous =
          forwardQueues.current.get(room.roomId) ?? Promise.resolve();
        const next = previous
          .catch(() => undefined)
          .then(() =>
            api.publishTeamRelayTerminalOutput(room.roomId, output.dataBase64),
          )
          .then(() => {
            forwardingErrors.current.delete(room.roomId);
          })
          .catch((reason) => {
            if (!forwardingErrors.current.has(room.roomId)) {
              forwardingErrors.current.add(room.roomId);
              reportError(reason);
            }
          });
        forwardQueues.current.set(room.roomId, next);
      }
    });
    return () => {
      void unlisten.then((stop) => stop());
    };
  }, [reportError]);

  const selectedRoom = relaySessions.find(
    (session) => session.roomId === selectedRoomId,
  );
  const workspace = workspaces.find((item) => item.id === workspaceId);
  const workspaceBindings = bindings.filter(
    (binding) => binding.workspaceId === workspaceId,
  );
  const pendingInvitations = invitations.filter(
    (item) => item.invitation.workspaceId === workspaceId,
  );
  const sshSessions = sessions.filter(
    (session) =>
      session.sessionType === "terminal" && session.status === "online",
  );
  const memberById = useMemo(
    () => new Map(members.map((member) => [member.id, member])),
    [members],
  );
  const deviceById = useMemo(
    () => new Map(devices.map((device) => [device.id, device])),
    [devices],
  );

  const run = async (key: string, action: () => Promise<void>) => {
    try {
      setBusy(key);
      setLocalError(null);
      await action();
    } catch (reason) {
      reportError(reason);
    } finally {
      setBusy(null);
    }
  };

  const startRoom = () =>
    run("start", async () => {
      const created = await api.startTeamRelayTerminalRoom(
        workspaceId,
        hostSessionId,
      );
      setRelaySessions((current) => upsertSession(current, created));
      setViewMode("host");
      setSelectedRoomId(created.roomId);
    });

  const routeInvitations = () =>
    run("invite", async () => {
      if (!selectedRoom) return;
      await api.inviteTeamRelayTerminalDevices(
        selectedRoom.roomId,
        selectedDeviceIds,
      );
      setPendingDeviceIdsByRoom((current) => ({
        ...current,
        [selectedRoom.roomId]: Array.from(
          new Set([
            ...(current[selectedRoom.roomId] ?? []),
            ...selectedDeviceIds,
          ]),
        ),
      }));
      setSelectedDeviceIds([]);
      const latest = await api.listTeamRelayTerminalSessions();
      setRelaySessions(latest);
    });

  const acceptInvitation = (item: TeamRelayTerminalInvitation) =>
    run(`accept-${item.roomId}`, async () => {
      const joined = await api.acceptTeamRelayTerminalInvitation(
        item.invitation,
      );
      setRelaySessions((current) => upsertSession(current, joined));
      setInvitations((current) =>
        current.filter((candidate) => candidate.roomId !== item.roomId),
      );
      setViewMode("participant");
      setSelectedRoomId(joined.roomId);
    });

  const grantControl = (participant: TeamTerminalParticipant) =>
    run(`grant-${participant.deviceId}`, async () => {
      if (!selectedRoom) return;
      const updated = await api.grantTeamRelayTerminalControl(
        selectedRoom.roomId,
        participant.deviceId,
        leaseDuration,
      );
      setRelaySessions((current) => upsertSession(current, updated));
    });

  const revokeControl = () =>
    run("revoke", async () => {
      if (!selectedRoom) return;
      const updated = await api.revokeTeamRelayTerminalControl(
        selectedRoom.roomId,
      );
      setRelaySessions((current) => upsertSession(current, updated));
    });

  const closeRoom = () => {
    if (!selectedRoom || !confirm("关闭这个在线团队终端房间？")) return;
    void run("close", async () => {
      const closed = await api.closeTeamRelayTerminalRoom(selectedRoom.roomId);
      setRelaySessions((current) => upsertSession(current, closed));
    });
  };

  if (!open) return null;

  return (
    <Modal title="在线团队终端" onClose={onClose} wide>
      <div className="team-terminal-center">
        <header className="team-terminal-toolbar">
          <div
            className="team-terminal-mode"
            role="tablist"
            aria-label="协作模式"
          >
            <button
              role="tab"
              aria-selected={viewMode === "host"}
              className={viewMode === "host" ? "active" : ""}
              onClick={() => setViewMode("host")}
            >
              <RadioTower size={14} />
              主持
            </button>
            <button
              role="tab"
              aria-selected={viewMode === "participant"}
              className={viewMode === "participant" ? "active" : ""}
              onClick={() => setViewMode("participant")}
            >
              <Users size={14} />
              加入
              {pendingInvitations.length > 0 && (
                <span className="team-terminal-count">
                  {pendingInvitations.length}
                </span>
              )}
            </button>
          </div>
          <label>
            <span>在线工作区</span>
            <select
              aria-label="在线工作区"
              value={workspaceId}
              onChange={(event) => setWorkspaceId(event.target.value)}
            >
              {bindings.map((binding) => (
                <option key={binding.workspaceId} value={binding.workspaceId}>
                  {workspaces.find((item) => item.id === binding.workspaceId)
                    ?.name ?? binding.profileName}
                </option>
              ))}
            </select>
          </label>
          <button
            className="mini-button"
            aria-label="刷新在线团队终端"
            title="刷新在线团队终端"
            disabled={busy != null}
            onClick={() => void refresh(true)}
          >
            <RefreshCw size={13} />
            刷新
          </button>
        </header>

        {localError && (
          <div className="inline-error" role="alert">
            {localError}
          </div>
        )}

        {!workspaceBindings.length ? (
          <div className="team-terminal-empty">
            <ShieldOff size={30} />
            <strong>当前没有已发布的在线工作区</strong>
            <span>请先在团队设置中登录 relay 并发布工作区。</span>
          </div>
        ) : (
          <div className="team-terminal-layout">
            <aside className="team-terminal-room-list" aria-label="在线房间">
              <div>
                <strong>
                  {viewMode === "host" ? "主持房间" : "已加入房间"}
                </strong>
                <small>{workspace?.name}</small>
              </div>
              {viewMode === "participant" && pendingInvitations.length > 0 && (
                <button
                  className={!selectedRoomId ? "active" : ""}
                  onClick={() => setSelectedRoomId("")}
                >
                  <span className="status-dot connecting" />
                  <span>
                    <strong>待处理邀请</strong>
                    <small>{pendingInvitations.length} 个房间</small>
                  </span>
                </button>
              )}
              {relaySessions
                .filter(
                  (session) =>
                    session.workspaceId === workspaceId &&
                    session.mode === viewMode &&
                    session.status !== "closed",
                )
                .sort((left, right) =>
                  right.createdAt.localeCompare(left.createdAt),
                )
                .map((session) => (
                  <button
                    key={session.roomId}
                    className={
                      session.roomId === selectedRoomId ? "active" : ""
                    }
                    onClick={() => setSelectedRoomId(session.roomId)}
                  >
                    <span className={`status-dot ${session.status}`} />
                    <span>
                      <strong>{roomTitle(session, sessions)}</strong>
                      <small>{terminalStatusLabel(session.status)}</small>
                    </span>
                  </button>
                ))}
              {!relaySessions.some(
                (session) =>
                  session.workspaceId === workspaceId &&
                  session.mode === viewMode &&
                  session.status !== "closed",
              ) && <p>暂无活动房间</p>}
            </aside>

            <section className="team-terminal-main">
              {viewMode === "host" ? (
                selectedRoom?.mode === "host" ? (
                  <HostRoom
                    room={selectedRoom}
                    devices={devices}
                    memberById={memberById}
                    deviceById={deviceById}
                    selectedDeviceIds={selectedDeviceIds}
                    setSelectedDeviceIds={setSelectedDeviceIds}
                    pendingDeviceIds={
                      pendingDeviceIdsByRoom[selectedRoom.roomId] ?? []
                    }
                    leaseDuration={leaseDuration}
                    setLeaseDuration={setLeaseDuration}
                    busy={busy}
                    onInvite={routeInvitations}
                    onGrant={grantControl}
                    onRevoke={revokeControl}
                    onClose={closeRoom}
                  />
                ) : (
                  <div className="team-terminal-start">
                    <div>
                      <RadioTower size={22} />
                      <span>
                        <strong>新建主持房间</strong>
                        <small>输出将通过端到端加密 relay 转发。</small>
                      </span>
                    </div>
                    <label>
                      <span>SSH 会话</span>
                      <select
                        aria-label="用于主持的 SSH 会话"
                        value={hostSessionId}
                        onChange={(event) =>
                          setHostSessionId(event.target.value)
                        }
                      >
                        {sshSessions.map((session) => (
                          <option key={session.id} value={session.id}>
                            {session.title}
                          </option>
                        ))}
                      </select>
                    </label>
                    {!sshSessions.length && <p>当前没有在线 SSH 会话。</p>}
                    <button
                      className="button primary"
                      disabled={!hostSessionId || busy != null}
                      onClick={() => void startRoom()}
                    >
                      {busy === "start" ? (
                        <LoaderCircle className="spin" size={14} />
                      ) : (
                        <RadioTower size={14} />
                      )}
                      开始共享
                    </button>
                  </div>
                )
              ) : (
                <ParticipantRoom
                  room={
                    selectedRoom?.mode === "participant" ? selectedRoom : null
                  }
                  invitations={pendingInvitations}
                  chunks={
                    selectedRoom
                      ? (outputBuffers.current.get(selectedRoom.roomId)
                          ?.chunks ?? [])
                      : []
                  }
                  outputVersion={outputVersion}
                  now={now}
                  busy={busy}
                  memberById={memberById}
                  deviceById={deviceById}
                  onAccept={acceptInvitation}
                  onClose={closeRoom}
                  onInput={(data) => {
                    if (!selectedRoom?.controlLease) return Promise.resolve();
                    return api.sendTeamRelayTerminalInput(
                      selectedRoom.roomId,
                      selectedRoom.controlLease.id,
                      selectedRoom.controlLease.generation,
                      encodeUtf8Base64(data),
                    );
                  }}
                  onError={reportError}
                />
              )}
            </section>
          </div>
        )}
      </div>
    </Modal>
  );
}

function HostRoom({
  room,
  devices,
  memberById,
  deviceById,
  selectedDeviceIds,
  setSelectedDeviceIds,
  pendingDeviceIds,
  leaseDuration,
  setLeaseDuration,
  busy,
  onInvite,
  onGrant,
  onRevoke,
  onClose,
}: {
  room: TeamRelayTerminalSession;
  devices: TeamDevice[];
  memberById: Map<string, TeamMember>;
  deviceById: Map<string, TeamDevice>;
  selectedDeviceIds: string[];
  setSelectedDeviceIds: (ids: string[]) => void;
  pendingDeviceIds: string[];
  leaseDuration: number;
  setLeaseDuration: (seconds: number) => void;
  busy: string | null;
  onInvite: () => void;
  onGrant: (participant: TeamTerminalParticipant) => void;
  onRevoke: () => void;
  onClose: () => void;
}) {
  const participantIds = new Set(
    room.participants.map((item) => item.deviceId),
  );
  const candidates = devices.filter(
    (device) =>
      device.status === "active" &&
      !device.isLocal &&
      !participantIds.has(device.id) &&
      !pendingDeviceIds.includes(device.id),
  );
  return (
    <div className="team-terminal-host-room">
      <RoomHeader room={room} onClose={onClose} />
      <section>
        <header>
          <span>
            <UserPlus size={14} />
            邀请设备
          </span>
          <button
            className="mini-button"
            disabled={!selectedDeviceIds.length || busy != null}
            onClick={onInvite}
          >
            {busy === "invite" ? (
              <LoaderCircle className="spin" size={13} />
            ) : (
              <UserPlus size={13} />
            )}
            发送邀请
          </button>
        </header>
        {candidates.length ? (
          <div className="team-terminal-device-grid">
            {candidates.map((device) => (
              <label key={device.id}>
                <input
                  type="checkbox"
                  aria-label={`邀请 ${device.name}`}
                  checked={selectedDeviceIds.includes(device.id)}
                  onChange={(event) =>
                    setSelectedDeviceIds(
                      event.target.checked
                        ? [...selectedDeviceIds, device.id]
                        : selectedDeviceIds.filter((id) => id !== device.id),
                    )
                  }
                />
                <span>
                  <strong>{device.name}</strong>
                  <small>
                    {memberById.get(device.memberId)?.displayName ??
                      device.memberId}
                  </small>
                </span>
              </label>
            ))}
          </div>
        ) : (
          <p>没有其他可邀请设备。</p>
        )}
        {pendingDeviceIds.length > 0 && (
          <p>{pendingDeviceIds.length} 台设备等待接受邀请。</p>
        )}
      </section>
      <section>
        <header>
          <span>
            <Users size={14} />
            房间成员
          </span>
          <label className="team-terminal-lease-duration">
            <span>控制时长</span>
            <select
              aria-label="控制权时长"
              value={leaseDuration}
              onChange={(event) => setLeaseDuration(Number(event.target.value))}
            >
              <option value={30}>30 秒</option>
              <option value={60}>60 秒</option>
              <option value={120}>2 分钟</option>
              <option value={300}>5 分钟</option>
            </select>
          </label>
        </header>
        <div className="team-terminal-participants">
          {room.participants.map((participant) => {
            const device = deviceById.get(participant.deviceId);
            const member = memberById.get(participant.memberId);
            const controls =
              room.controlLease?.deviceId === participant.deviceId &&
              Date.parse(room.controlLease.expiresAt) > Date.now();
            return (
              <div key={participant.deviceId}>
                <span className="team-terminal-avatar">
                  {(member?.displayName ?? device?.name ?? "?")
                    .slice(0, 1)
                    .toUpperCase()}
                </span>
                <span>
                  <strong>{member?.displayName ?? participant.memberId}</strong>
                  <small>
                    {device?.name ?? participant.deviceId} ·{" "}
                    {roleLabel(participant.role)}
                  </small>
                </span>
                {participant.deviceId === room.localDeviceId ? (
                  <small>主持端</small>
                ) : controls ? (
                  <button
                    className="mini-button danger"
                    disabled={busy != null}
                    onClick={onRevoke}
                  >
                    <ShieldOff size={13} />
                    撤销控制
                  </button>
                ) : (
                  <button
                    className="mini-button"
                    disabled={busy != null}
                    onClick={() => onGrant(participant)}
                  >
                    <Keyboard size={13} />
                    授予控制
                  </button>
                )}
              </div>
            );
          })}
        </div>
      </section>
    </div>
  );
}

function ParticipantRoom({
  room,
  invitations,
  chunks,
  outputVersion,
  now,
  busy,
  memberById,
  deviceById,
  onAccept,
  onClose,
  onInput,
  onError,
}: {
  room: TeamRelayTerminalSession | null;
  invitations: TeamRelayTerminalInvitation[];
  chunks: OutputChunk[];
  outputVersion: number;
  now: number;
  busy: string | null;
  memberById: Map<string, TeamMember>;
  deviceById: Map<string, TeamDevice>;
  onAccept: (item: TeamRelayTerminalInvitation) => void;
  onClose: () => void;
  onInput: (data: string) => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  if (!room) {
    return (
      <div className="team-terminal-invitations">
        <header>
          <span>
            <Users size={14} />
            待处理邀请
          </span>
        </header>
        {invitations.length ? (
          invitations.map((item) => (
            <div key={item.roomId}>
              <span>
                <strong>
                  {memberById.get(item.invitation.hostMemberId)?.displayName ??
                    "团队成员"}
                </strong>
                <small>
                  {deviceById.get(item.invitation.hostDeviceId)?.name ??
                    item.invitation.hostDeviceId}
                </small>
              </span>
              <time>
                {new Date(item.invitation.expiresAt).toLocaleTimeString()}
              </time>
              <button
                className="button primary"
                disabled={busy != null}
                onClick={() => onAccept(item)}
              >
                {busy === `accept-${item.roomId}` ? (
                  <LoaderCircle className="spin" size={14} />
                ) : (
                  <Check size={14} />
                )}
                接受
              </button>
            </div>
          ))
        ) : (
          <div className="team-terminal-empty compact">
            <Eye size={26} />
            <strong>没有待处理邀请</strong>
          </div>
        )}
      </div>
    );
  }
  const lease = room.controlLease;
  const canInput = Boolean(
    room.status === "online" &&
    lease &&
    lease.deviceId === room.localDeviceId &&
    Date.parse(lease.expiresAt) > now,
  );
  return (
    <div className="team-terminal-participant-room">
      <RoomHeader room={room} onClose={onClose} />
      <div
        className={`team-terminal-access ${canInput ? "control" : "readonly"}`}
        role="status"
      >
        {canInput ? <Keyboard size={14} /> : <Eye size={14} />}
        <span>
          <strong>{canInput ? "可控制" : "只读"}</strong>
          <small>
            {canInput && lease
              ? `控制权到 ${new Date(lease.expiresAt).toLocaleTimeString()}`
              : "等待主持端授予控制权"}
          </small>
        </span>
      </div>
      <SharedTerminal
        key={room.roomId}
        chunks={chunks}
        version={outputVersion}
        canInput={canInput}
        onInput={onInput}
        onError={onError}
      />
    </div>
  );
}

function RoomHeader({
  room,
  onClose,
}: {
  room: TeamRelayTerminalSession;
  onClose: () => void;
}) {
  return (
    <header className="team-terminal-room-header">
      <span className={`status-dot ${room.status}`} />
      <span>
        <strong>{room.mode === "host" ? "主持中" : "在线终端"}</strong>
        <small>
          {terminalStatusLabel(room.status)} · 序号 {room.lastOutputSequence}
        </small>
      </span>
      {room.lastError && <p>{room.lastError}</p>}
      <button className="mini-button danger" onClick={onClose}>
        <X size={13} />
        关闭房间
      </button>
    </header>
  );
}

function SharedTerminal({
  chunks,
  version,
  canInput,
  onInput,
  onError,
}: {
  chunks: OutputChunk[];
  version: number;
  canInput: boolean;
  onInput: (data: string) => Promise<void>;
  onError: (reason: unknown) => void;
}) {
  const hostRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const lastSequenceRef = useRef(0);
  const onInputRef = useRef(onInput);
  const onErrorRef = useRef(onError);
  const canInputRef = useRef(canInput);
  const settings = useAppStore((state) => state.settings);
  const preferences = resolveTerminalPreferences(
    settings,
    "team-relay-terminal",
  );
  const initialPreferencesRef = useRef(preferences);
  onInputRef.current = onInput;
  onErrorRef.current = onError;
  canInputRef.current = canInput;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;
    const initialPreferences = initialPreferencesRef.current;
    const terminal = new Terminal({
      convertEol: false,
      cursorBlink: canInputRef.current,
      disableStdin: !canInputRef.current,
      scrollback: initialPreferences.scrollback,
      fontFamily: terminalFontFamilies[initialPreferences.fontFamily],
      fontSize: initialPreferences.fontSize,
      lineHeight: initialPreferences.lineHeight,
      theme: terminalThemes[initialPreferences.colorScheme],
    });
    const fit = new FitAddon();
    terminal.loadAddon(fit);
    terminal.open(host);
    fit.fit();
    terminalRef.current = terminal;
    fitRef.current = fit;
    const queue = createTerminalInputQueue((data) => onInputRef.current(data));
    const input = terminal.onData((data) => {
      if (canInputRef.current)
        void queue(data).catch((reason) => onErrorRef.current(reason));
    });
    const resize =
      typeof ResizeObserver === "undefined"
        ? null
        : new ResizeObserver(() => fit.fit());
    resize?.observe(host);
    return () => {
      resize?.disconnect();
      input.dispose();
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      lastSequenceRef.current = 0;
    };
  }, []);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.disableStdin = !canInput;
    terminal.options.cursorBlink = canInput;
    if (canInput) terminal.focus();
  }, [canInput]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    for (const chunk of chunks) {
      if (chunk.sequence <= lastSequenceRef.current) continue;
      terminal.write(chunk.bytes);
      lastSequenceRef.current = chunk.sequence;
    }
  }, [chunks, version]);

  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = terminalFontFamilies[preferences.fontFamily];
    terminal.options.fontSize = preferences.fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.scrollback = preferences.scrollback;
    terminal.options.theme = terminalThemes[preferences.colorScheme];
    requestAnimationFrame(() => fitRef.current?.fit());
  }, [preferences]);

  return (
    <div
      className="team-terminal-xterm"
      ref={hostRef}
      aria-label="共享终端输出"
    />
  );
}

function upsertSession(
  sessions: TeamRelayTerminalSession[],
  next: TeamRelayTerminalSession,
) {
  return [
    ...sessions.filter((session) => session.roomId !== next.roomId),
    next,
  ];
}

function roomTitle(
  room: TeamRelayTerminalSession,
  sessions: TerminalSession[],
) {
  if (room.mode === "host")
    return (
      sessions.find((session) => session.id === room.terminalSessionId)
        ?.title ?? "SSH 会话"
    );
  return `房间 ${room.roomId.slice(0, 8)}`;
}

function terminalStatusLabel(status: string) {
  return (
    {
      connecting: "连接中",
      online: "在线",
      reconnecting: "重连中",
      failed: "失败",
      closed: "已关闭",
    }[status] ?? status
  );
}

function roleLabel(role: string) {
  return (
    { owner: "Owner", admin: "管理员", operator: "操作员", viewer: "只读" }[
      role
    ] ?? role
  );
}

function decodeBase64(value: string) {
  const binary = atob(value);
  return Uint8Array.from(binary, (character) => character.charCodeAt(0));
}

function encodeUtf8Base64(value: string) {
  const bytes = new TextEncoder().encode(value);
  let binary = "";
  for (let offset = 0; offset < bytes.length; offset += 0x8000)
    binary += String.fromCharCode(...bytes.subarray(offset, offset + 0x8000));
  return btoa(binary);
}
