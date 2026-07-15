import { open, save } from "@tauri-apps/plugin-dialog";
import { Download, KeyRound, Plus, Share2, ShieldCheck, Trash2, Upload, Users } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { ConnectionProfile, TeamAuditEvent, TeamDevice, TeamMember, TeamPermissionReport, TeamRelayWorkspaceBinding, TeamSharePreview, TeamWorkspace } from "../../types";
import { TeamRelaySettings } from "./TeamRelaySettings";

const roleLabel: Record<string, string> = { owner: "Owner", admin: "Admin", operator: "Operator", viewer: "Viewer" };

export function TeamSettings({
  onError,
  connections = [],
  onConnectionImported,
}: {
  onError: (message: string) => void;
  connections?: ConnectionProfile[];
  onConnectionImported?: () => void | Promise<void>;
}) {
  const [workspaces, setWorkspaces] = useState<TeamWorkspace[]>([]);
  const [relayBindings, setRelayBindings] = useState<TeamRelayWorkspaceBinding[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [members, setMembers] = useState<TeamMember[]>([]);
  const [devices, setDevices] = useState<TeamDevice[]>([]);
  const [permissions, setPermissions] = useState<TeamPermissionReport | null>(null);
  const [audit, setAudit] = useState<TeamAuditEvent[]>([]);
  const [teamName, setTeamName] = useState("");
  const [ownerName, setOwnerName] = useState("");
  const [memberName, setMemberName] = useState("");
  const [memberRole, setMemberRole] = useState("viewer");
  const [deviceName, setDeviceName] = useState("");
  const [connectionId, setConnectionId] = useState("");
  const [recipientIds, setRecipientIds] = useState<Set<string>>(new Set());
  const [includeCredential, setIncludeCredential] = useState(false);
  const [sharePreview, setSharePreview] = useState<TeamSharePreview | null>(null);
  const [busy, setBusy] = useState(false);

  const shareableConnections = connections.filter((connection) => connection.protocol === "ssh" || connection.protocol === "rdp");
  const relayBinding = relayBindings.find((binding) => binding.workspaceId === workspaceId) ?? null;

  const refreshWorkspaces = useCallback(async () => {
    try {
      const [values, bindings] = await Promise.all([api.listTeamWorkspaces(), api.listTeamRelayBindings()]);
      setWorkspaces(values);
      setRelayBindings(bindings);
      setWorkspaceId((current) => values.some((item) => item.id === current) ? current : values[0]?.id ?? "");
    } catch (error) { onError(errorMessage(error)); }
  }, [onError]);

  const refreshWorkspace = useCallback(async (id: string) => {
    if (!id) { setMembers([]); setDevices([]); setPermissions(null); setAudit([]); return; }
    try {
      const report = await api.getTeamPermissions(id);
      const [memberValues, deviceValues, auditValues] = await Promise.all([
        api.listTeamMembers(id),
        api.listTeamDevices(id),
        report.permissions.includes("auditRead") ? api.listTeamAudit(id) : Promise.resolve([]),
      ]);
      setPermissions(report);
      setMembers(memberValues);
      setDevices(deviceValues);
      setRecipientIds((current) => new Set([...current].filter((value) => deviceValues.some((device) => device.id === value && device.status === "active"))));
      setAudit(auditValues.slice(0, 8));
    } catch (error) { onError(errorMessage(error)); }
  }, [onError]);

  useEffect(() => { void refreshWorkspaces(); }, [refreshWorkspaces]);
  useEffect(() => { void refreshWorkspace(workspaceId); }, [refreshWorkspace, workspaceId]);
  useEffect(() => {
    if (!connectionId && shareableConnections[0]) setConnectionId(shareableConnections[0].id);
  }, [connectionId, shareableConnections]);

  const createWorkspace = async () => {
    if (!teamName.trim() || !ownerName.trim()) return;
    try { setBusy(true); const created = await api.createTeamWorkspace({ name: teamName, ownerName }); setTeamName(""); setOwnerName(""); await refreshWorkspaces(); setWorkspaceId(created.id); }
    catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const saveMember = async (member?: TeamMember, role = memberRole) => {
    const displayName = member?.displayName ?? memberName;
    if (!workspaceId || !displayName.trim()) return;
    try { setBusy(true); if (relayBinding && member) await api.updateTeamRelayMember({ workspaceId, memberId: member.id, role, status: "active" }); else await api.saveTeamMember({ workspaceId, memberId: member?.id ?? null, displayName, role }); if (!member) setMemberName(""); await Promise.all([refreshWorkspace(workspaceId), refreshWorkspaces()]); }
    catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const removeMember = async (member: TeamMember) => {
    if (!confirm(`移除成员 ${member.displayName}？后续共享会切换到新的密钥 epoch。`)) return;
    try { if (relayBinding) await api.updateTeamRelayMember({ workspaceId, memberId: member.id, role: member.role, status: "removed" }); else await api.removeTeamMember(workspaceId, member.id); await Promise.all([refreshWorkspace(workspaceId), refreshWorkspaces()]); }
    catch (error) { onError(errorMessage(error)); }
  };

  const ensureDevice = async () => {
    if (!deviceName.trim()) return;
    try { setBusy(true); await api.ensureTeamDevice(workspaceId, deviceName); setDeviceName(""); await refreshWorkspace(workspaceId); }
    catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const exportDevice = async () => {
    try {
      const path = await save({ defaultPath: "cnshell-device.cnshelldevice", filters: [{ name: "CNshell Device", extensions: ["cnshelldevice"] }] });
      if (path) await api.exportTeamDevice(workspaceId, path);
    } catch (error) { onError(errorMessage(error)); }
  };

  const importDevice = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "CNshell Device", extensions: ["cnshelldevice"] }] });
      if (!path) return;
      if (!confirm("导入前应通过独立渠道核对设备公钥指纹。确认把该设备加入当前工作区？")) return;
      await api.importTeamDevice(workspaceId, path);
      await refreshWorkspace(workspaceId);
    } catch (error) { onError(errorMessage(error)); }
  };

  const revokeDevice = async (device: TeamDevice) => {
    if (!confirm(`撤销设备 ${device.name}？工作区会切换到新的密钥 epoch。`)) return;
    try { if (relayBinding) await api.revokeTeamRelayDevice(workspaceId, device.id); else await api.revokeTeamDevice(workspaceId, device.id); await Promise.all([refreshWorkspace(workspaceId), refreshWorkspaces()]); }
    catch (error) { onError(errorMessage(error)); }
  };

  const exportShare = async () => {
    if (!connectionId || recipientIds.size === 0) return;
    try {
      const path = await save({ defaultPath: "connection.cnshellshare", filters: [{ name: "CNshell Secure Share", extensions: ["cnshellshare"] }] });
      if (!path) return;
      if (includeCredential && !confirm("分享文件将包含端到端加密的连接凭据。只有所选设备可解密，是否继续？")) return;
      setBusy(true);
      await api.exportTeamShare({ workspaceId, connectionId, recipientDeviceIds: [...recipientIds], includeCredential, outputPath: path });
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const previewShare = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "CNshell Secure Share", extensions: ["cnshellshare"] }] });
      if (path) setSharePreview(await api.previewTeamShare(path));
    } catch (error) { onError(errorMessage(error)); }
  };

  const applyShare = async () => {
    if (!sharePreview || !confirm(`导入连接 ${sharePreview.connectionName}（${sharePreview.host}）？`)) return;
    try { setBusy(true); await api.applyTeamShare(sharePreview.requestId); setSharePreview(null); await onConnectionImported?.(); }
    catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const exportAudit = async () => {
    try { const path = await save({ defaultPath: "cnshell-team-audit.json", filters: [{ name: "JSON", extensions: ["json"] }] }); if (path) await api.exportTeamAudit(workspaceId, path); }
    catch (error) { onError(errorMessage(error)); }
  };

  const exportWorkspace = async () => {
    try {
      const path = await save({ defaultPath: "cnshell-team-workspace.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (!path) return;
      setBusy(true);
      await api.exportTeamWorkspace(workspaceId, path);
      await refreshWorkspace(workspaceId);
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const workspace = workspaces.find((item) => item.id === workspaceId);
  const canManageMembers = permissions?.permissions.includes("memberManage") ?? false;
  const canManageOwners = permissions?.permissions.includes("ownerManage") ?? false;
  const canManageShares = permissions?.permissions.includes("shareManage") ?? false;
  const canExportWorkspace = permissions?.permissions.includes("workspaceExport") ?? false;
  const localDevice = devices.find((device) => device.isLocal && device.status === "active");
  const relayChanged = async (targetWorkspaceId?: string) => {
    await refreshWorkspaces();
    const target = targetWorkspaceId ?? workspaceId;
    if (targetWorkspaceId) setWorkspaceId(targetWorkspaceId);
    if (target) await refreshWorkspace(target);
  };

  return <section className="plugin-settings" aria-label="团队工作区">
    <div className="section-heading"><div><h3><Users size={16}/> 团队工作区与 RBAC</h3></div></div>
    <TeamRelaySettings workspaces={workspaces} workspaceId={workspaceId} binding={relayBinding} canManageMembers={permissions?.permissions.includes("memberManage") ?? false} onChanged={relayChanged} onError={onError}/>
    <div className="plugin-report"><strong>创建本地工作区</strong><div className="backup-actions"><input aria-label="团队名称" placeholder="团队名称" value={teamName} onChange={(event) => setTeamName(event.target.value)} maxLength={256}/><input aria-label="Owner 名称" placeholder="Owner 名称" value={ownerName} onChange={(event) => setOwnerName(event.target.value)} maxLength={256}/><button className="mini-button" disabled={busy || !teamName.trim() || !ownerName.trim()} onClick={() => void createWorkspace()}><Plus size={13}/>创建</button></div></div>
    {workspaces.length > 0 && <>
      <label><span>工作区</span><select value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)}>{workspaces.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
      {workspace && <div className="plugin-report"><strong>{workspace.name}</strong><small>本机角色：{roleLabel[workspace.localRole]} · 密钥 epoch {workspace.keyEpoch}</small><small>权限：{permissions?.permissions.join(", ") || "无"}</small>{canExportWorkspace && <button className="mini-button" disabled={busy} onClick={() => void exportWorkspace()}><Download size={13}/>导出组织目录</button>}</div>}
      {canManageMembers && !relayBinding && <div className="plugin-report"><strong>添加成员</strong><div className="backup-actions"><input aria-label="成员名称" placeholder="成员名称" value={memberName} onChange={(event) => setMemberName(event.target.value)} maxLength={256}/><select aria-label="成员角色" value={memberRole} onChange={(event) => setMemberRole(event.target.value)}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="admin">Admin</option>{canManageOwners && <option value="owner">Owner</option>}</select><button className="mini-button" disabled={busy || !memberName.trim()} onClick={() => void saveMember()}><Plus size={13}/>添加</button></div></div>}
      <div className="plugin-report"><strong>成员</strong>{members.map((member) => <div className="plugin-record" key={member.id}><div><strong>{member.displayName}</strong><small>{member.id === workspace?.localMemberId ? "本机成员 · " : ""}{roleLabel[member.role]} · {member.status === "active" ? "活动" : "已移除"}</small></div>{member.status === "active" && canManageMembers && member.id !== workspace?.localMemberId && <div><select aria-label={`修改 ${member.displayName} 角色`} value={member.role} onChange={(event) => void saveMember(member, event.target.value)} disabled={busy || (member.role === "owner" && !canManageOwners)}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="admin">Admin</option>{canManageOwners && <option value="owner">Owner</option>}</select><button className="icon-button" aria-label={`移除 ${member.displayName}`} onClick={() => void removeMember(member)}><Trash2 size={14}/></button></div>}</div>)}</div>
      <div className="plugin-report"><strong><KeyRound size={14}/> 设备密钥</strong>{!localDevice && <div className="backup-actions"><input aria-label="本机设备名称" placeholder="例如：Chen 的 Mac" value={deviceName} onChange={(event) => setDeviceName(event.target.value)} maxLength={256}/><button className="mini-button" disabled={busy || !deviceName.trim()} onClick={() => void ensureDevice()}><KeyRound size={13}/>创建设备身份</button></div>}{localDevice && <div className="backup-actions"><button className="mini-button" onClick={() => void exportDevice()}><Download size={13}/>导出本机公钥</button>{canManageShares && !relayBinding && <button className="mini-button" onClick={() => void importDevice()}><Upload size={13}/>导入设备公钥</button>}</div>}{devices.map((device) => <div className="plugin-record" key={device.id}><div><strong>{device.name}</strong><small>{members.find((member) => member.id === device.memberId)?.displayName ?? device.memberId} · {device.isLocal ? "本机" : "远端"} · {device.status === "active" ? "活动" : "已撤销"}</small><small>SHA-256 {device.fingerprint.replace("sha256:", "")}</small></div>{device.status === "active" && canManageShares && (!relayBinding || !device.isLocal) && <button className="icon-button" aria-label={`撤销设备 ${device.name}`} onClick={() => void revokeDevice(device)}><Trash2 size={14}/></button>}</div>)}</div>
      {localDevice && permissions?.permissions.includes("shareCreate") && <div className="plugin-report"><strong><Share2 size={14}/> 端到端连接分享</strong><label><span>连接</span><select value={connectionId} onChange={(event) => setConnectionId(event.target.value)}>{shareableConnections.map((connection) => <option key={connection.id} value={connection.id}>{connection.name}</option>)}</select></label><div>{devices.filter((device) => device.status === "active").map((device) => <label className="check-row" key={device.id}><input type="checkbox" checked={recipientIds.has(device.id)} onChange={(event) => setRecipientIds((current) => { const next = new Set(current); if (event.target.checked) next.add(device.id); else next.delete(device.id); return next; })}/><span>{device.name}</span></label>)}</div>{canManageShares && <label className="check-row"><input type="checkbox" checked={includeCredential} onChange={(event) => setIncludeCredential(event.target.checked)}/><span>包含加密凭据</span></label>}<div className="backup-actions"><button className="mini-button" disabled={busy || !connectionId || recipientIds.size === 0} onClick={() => void exportShare()}><Download size={13}/>导出分享</button><button className="mini-button" onClick={() => void previewShare()}><Upload size={13}/>打开分享</button></div>{sharePreview && <div aria-live="polite"><small>{sharePreview.connectionName} · {sharePreview.protocol.toUpperCase()} · {sharePreview.host} · epoch {sharePreview.keyEpoch} · {sharePreview.hasCredential ? "含凭据" : "无凭据"}</small><button className="mini-button" disabled={busy} onClick={() => void applyShare()}><Plus size={13}/>导入连接</button></div>}</div>}
      {(permissions?.permissions.includes("auditRead") ?? false) && <div className="plugin-report"><strong><ShieldCheck size={14}/> 最近团队审计</strong><button className="mini-button" disabled={!audit.length} onClick={() => void exportAudit()}><Download size={13}/>导出</button>{audit.map((event) => <small key={event.id}>{new Date(event.createdAt).toLocaleString()} · {event.actorMemberId} · {event.action} · {event.targetType}:{event.targetId}</small>)}</div>}
    </>}
  </section>;
}
