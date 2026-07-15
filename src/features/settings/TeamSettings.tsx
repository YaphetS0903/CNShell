import { save } from "@tauri-apps/plugin-dialog";
import { Download, Plus, ShieldCheck, Trash2, Users } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { TeamAuditEvent, TeamMember, TeamPermissionReport, TeamWorkspace } from "../../types";

const roleLabel: Record<string, string> = {
  owner: "Owner",
  admin: "Admin",
  operator: "Operator",
  viewer: "Viewer",
};

export function TeamSettings({ onError }: { onError: (message: string) => void }) {
  const [workspaces, setWorkspaces] = useState<TeamWorkspace[]>([]);
  const [workspaceId, setWorkspaceId] = useState("");
  const [members, setMembers] = useState<TeamMember[]>([]);
  const [permissions, setPermissions] = useState<TeamPermissionReport | null>(null);
  const [audit, setAudit] = useState<TeamAuditEvent[]>([]);
  const [teamName, setTeamName] = useState("");
  const [ownerName, setOwnerName] = useState("");
  const [memberName, setMemberName] = useState("");
  const [memberRole, setMemberRole] = useState("viewer");
  const [busy, setBusy] = useState(false);

  const refreshWorkspaces = useCallback(async () => {
    try {
      const values = await api.listTeamWorkspaces();
      setWorkspaces(values);
      setWorkspaceId((current) => values.some((item) => item.id === current) ? current : values[0]?.id ?? "");
    } catch (error) { onError(errorMessage(error)); }
  }, [onError]);

  const refreshWorkspace = useCallback(async (id: string) => {
    if (!id) { setMembers([]); setPermissions(null); setAudit([]); return; }
    try {
      const report = await api.getTeamPermissions(id);
      const [memberValues, auditValues] = await Promise.all([
        api.listTeamMembers(id),
        report.permissions.includes("auditRead") ? api.listTeamAudit(id) : Promise.resolve([]),
      ]);
      setPermissions(report);
      setMembers(memberValues);
      setAudit(auditValues.slice(0, 8));
    } catch (error) { onError(errorMessage(error)); }
  }, [onError]);

  useEffect(() => { void refreshWorkspaces(); }, [refreshWorkspaces]);
  useEffect(() => { void refreshWorkspace(workspaceId); }, [refreshWorkspace, workspaceId]);

  const createWorkspace = async () => {
    if (!teamName.trim() || !ownerName.trim()) return;
    try {
      setBusy(true);
      const created = await api.createTeamWorkspace({ name: teamName, ownerName });
      setTeamName(""); setOwnerName("");
      await refreshWorkspaces();
      setWorkspaceId(created.id);
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const saveMember = async (member?: TeamMember, role = memberRole) => {
    const displayName = member?.displayName ?? memberName;
    if (!workspaceId || !displayName.trim()) return;
    try {
      setBusy(true);
      await api.saveTeamMember({ workspaceId, memberId: member?.id ?? null, displayName, role });
      if (!member) setMemberName("");
      await Promise.all([refreshWorkspace(workspaceId), refreshWorkspaces()]);
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const removeMember = async (member: TeamMember) => {
    if (!confirm(`移除成员 ${member.displayName}？后续共享会切换到新的密钥 epoch。`)) return;
    try {
      await api.removeTeamMember(workspaceId, member.id);
      await Promise.all([refreshWorkspace(workspaceId), refreshWorkspaces()]);
    } catch (error) { onError(errorMessage(error)); }
  };

  const exportAudit = async () => {
    try {
      const path = await save({ defaultPath: "cnshell-team-audit.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (path) await api.exportTeamAudit(workspaceId, path);
    } catch (error) { onError(errorMessage(error)); }
  };

  const workspace = workspaces.find((item) => item.id === workspaceId);
  const canManageMembers = permissions?.permissions.includes("memberManage") ?? false;
  const canManageOwners = permissions?.permissions.includes("ownerManage") ?? false;

  return <section className="plugin-settings" aria-label="团队工作区">
    <div className="section-heading"><div><h3><Users size={16}/> 团队工作区与 RBAC</h3><p>本机权限和审计已启用；在线邀请、同步与多人会话仍需正式团队服务。</p></div></div>
    <div className="plugin-report">
      <strong>创建本地工作区</strong>
      <div className="backup-actions">
        <input aria-label="团队名称" placeholder="团队名称" value={teamName} onChange={(event) => setTeamName(event.target.value)} maxLength={256}/>
        <input aria-label="Owner 名称" placeholder="Owner 名称" value={ownerName} onChange={(event) => setOwnerName(event.target.value)} maxLength={256}/>
        <button className="mini-button" disabled={busy || !teamName.trim() || !ownerName.trim()} onClick={() => void createWorkspace()}><Plus size={13}/>创建</button>
      </div>
    </div>
    {workspaces.length > 0 && <>
      <label><span>工作区</span><select value={workspaceId} onChange={(event) => setWorkspaceId(event.target.value)}>{workspaces.map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
      {workspace && <div className="plugin-report"><strong>{workspace.name}</strong><small>本机角色：{roleLabel[workspace.localRole]} · 密钥 epoch {workspace.keyEpoch}</small><small>权限：{permissions?.permissions.join(", ") || "无"}</small></div>}
      {canManageMembers && <div className="plugin-report">
        <strong>添加成员</strong>
        <div className="backup-actions">
          <input aria-label="成员名称" placeholder="成员名称" value={memberName} onChange={(event) => setMemberName(event.target.value)} maxLength={256}/>
          <select aria-label="成员角色" value={memberRole} onChange={(event) => setMemberRole(event.target.value)}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="admin">Admin</option>{canManageOwners && <option value="owner">Owner</option>}</select>
          <button className="mini-button" disabled={busy || !memberName.trim()} onClick={() => void saveMember()}><Plus size={13}/>添加</button>
        </div>
      </div>}
      <div className="plugin-report"><strong>成员</strong>{members.map((member) => <div className="plugin-record" key={member.id}>
        <div><strong>{member.displayName}</strong><small>{member.id === workspace?.localMemberId ? "本机成员 · " : ""}{roleLabel[member.role]} · {member.status === "active" ? "活动" : "已移除"}</small></div>
        {member.status === "active" && canManageMembers && member.id !== workspace?.localMemberId && <div>
          <select aria-label={`修改 ${member.displayName} 角色`} value={member.role} onChange={(event) => void saveMember(member, event.target.value)} disabled={busy || (member.role === "owner" && !canManageOwners)}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="admin">Admin</option>{canManageOwners && <option value="owner">Owner</option>}</select>
          <button className="icon-button" aria-label={`移除 ${member.displayName}`} onClick={() => void removeMember(member)}><Trash2 size={14}/></button>
        </div>}
      </div>)}</div>
      {(permissions?.permissions.includes("auditRead") ?? false) && <div className="plugin-report"><strong><ShieldCheck size={14}/> 最近团队审计</strong><button className="mini-button" disabled={!audit.length} onClick={() => void exportAudit()}><Download size={13}/>导出</button>{audit.map((event) => <small key={event.id}>{new Date(event.createdAt).toLocaleString()} · {event.actorMemberId} · {event.action} · {event.targetType}:{event.targetId}</small>)}</div>}
    </>}
  </section>;
}
