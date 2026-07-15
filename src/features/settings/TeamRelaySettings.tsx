import {
  Cloud,
  CloudUpload,
  Copy,
  Eye,
  EyeOff,
  LogIn,
  LogOut,
  Plus,
  RefreshCw,
  Send,
  Trash2,
  UserPlus,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type {
  TeamRelayInvitation,
  TeamRelayProfile,
  TeamRelayWorkspaceBinding,
  TeamWorkspace,
} from "../../types";

export function TeamRelaySettings({
  workspaces,
  workspaceId,
  binding,
  canManageMembers,
  onChanged,
  onError,
}: {
  workspaces: TeamWorkspace[];
  workspaceId: string;
  binding: TeamRelayWorkspaceBinding | null;
  canManageMembers: boolean;
  onChanged: (workspaceId?: string) => void | Promise<void>;
  onError: (message: string) => void;
}) {
  const [profiles, setProfiles] = useState<TeamRelayProfile[]>([]);
  const [profileId, setProfileId] = useState("");
  const [profileName, setProfileName] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [email, setEmail] = useState("");
  const [password, setPassword] = useState("");
  const [displayName, setDisplayName] = useState("");
  const [showPassword, setShowPassword] = useState(false);
  const [inviteEmail, setInviteEmail] = useState("");
  const [inviteRole, setInviteRole] = useState("viewer");
  const [invitation, setInvitation] = useState<TeamRelayInvitation | null>(null);
  const [acceptToken, setAcceptToken] = useState("");
  const [deviceName, setDeviceName] = useState("");
  const [busy, setBusy] = useState("");

  const refreshProfiles = useCallback(async () => {
    try {
      const values = await api.listTeamRelayProfiles();
      setProfiles(values);
      setProfileId((current) => {
        if (binding && values.some((profile) => profile.id === binding.profileId)) return binding.profileId;
        return values.some((profile) => profile.id === current) ? current : values[0]?.id ?? "";
      });
    } catch (error) {
      onError(errorMessage(error));
    }
  }, [binding, onError]);

  useEffect(() => { void refreshProfiles(); }, [refreshProfiles]);

  const selectedProfile = profiles.find((profile) => profile.id === profileId) ?? null;
  const selectedWorkspace = workspaces.find((workspace) => workspace.id === workspaceId) ?? null;

  const saveProfile = async () => {
    if (!profileName.trim() || !baseUrl.trim()) return;
    try {
      setBusy("profile");
      const saved = await api.saveTeamRelayProfile({
        id: crypto.randomUUID(),
        name: profileName,
        baseUrl,
      });
      setProfileName("");
      setBaseUrl("");
      await refreshProfiles();
      setProfileId(saved.id);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const deleteProfile = async () => {
    if (!selectedProfile || !confirm(`删除团队服务配置 ${selectedProfile.name}？`)) return;
    try {
      setBusy("profile");
      await api.deleteTeamRelayProfile(selectedProfile.id);
      await refreshProfiles();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const authenticate = async (mode: "login" | "register") => {
    if (!selectedProfile || !email.trim() || password.length < 12) return;
    if (mode === "register" && !displayName.trim()) return;
    try {
      setBusy(mode);
      const input = {
        profileId: selectedProfile.id,
        email,
        password,
        displayName: mode === "register" ? displayName : null,
      };
      if (mode === "register") await api.registerTeamRelayAccount(input);
      else await api.loginTeamRelayAccount(input);
      setPassword("");
      await refreshProfiles();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const logout = async () => {
    if (!selectedProfile || !confirm(`退出 ${selectedProfile.accountEmail ?? "当前账号"}？本机保存的在线会话将被清除。`)) return;
    try {
      setBusy("logout");
      await api.logoutTeamRelayAccount(selectedProfile.id);
      await refreshProfiles();
      await onChanged();
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const publish = async () => {
    if (!selectedProfile || !selectedWorkspace) return;
    if (!confirm(`把 ${selectedWorkspace.name} 发布到 ${selectedProfile.name}？发布后成员和设备以服务端状态为准。`)) return;
    try {
      setBusy("publish");
      await api.publishTeamRelayWorkspace(selectedWorkspace.id, selectedProfile.id);
      await onChanged(selectedWorkspace.id);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const sync = async () => {
    if (!workspaceId) return;
    try {
      setBusy("sync");
      await api.syncTeamRelayWorkspace(workspaceId);
      await onChanged(workspaceId);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const createInvitation = async () => {
    if (!workspaceId || !inviteEmail.trim()) return;
    try {
      setBusy("invite");
      const created = await api.createTeamRelayInvitation({
        workspaceId,
        email: inviteEmail,
        role: inviteRole,
      });
      setInvitation(created);
      setInviteEmail("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  const acceptInvitation = async () => {
    if (!selectedProfile || !acceptToken.trim() || !deviceName.trim()) return;
    try {
      setBusy("accept");
      const workspace = await api.acceptTeamRelayInvitation({
        profileId: selectedProfile.id,
        token: acceptToken.trim(),
        deviceName,
      });
      setAcceptToken("");
      setDeviceName("");
      await onChanged(workspace.id);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy("");
    }
  };

  return <div className="team-relay-settings">
    <div className="section-heading">
      <div><h3><Cloud size={16}/> 在线团队服务</h3></div>
      {binding && <button className="mini-button" disabled={Boolean(busy)} onClick={() => void sync()}><RefreshCw size={13}/>同步</button>}
    </div>

    <div className="plugin-report team-relay-profile">
      <strong>服务配置</strong>
      <div className="team-relay-form-row">
        <label><span>名称</span><input value={profileName} onChange={(event) => setProfileName(event.target.value)} maxLength={256}/></label>
        <label><span>HTTPS 地址</span><input type="url" value={baseUrl} onChange={(event) => setBaseUrl(event.target.value)} placeholder="https://relay.example.com" autoCapitalize="none" spellCheck={false}/></label>
        <button className="mini-button" disabled={Boolean(busy) || !profileName.trim() || !baseUrl.trim()} onClick={() => void saveProfile()}><Plus size={13}/>添加</button>
      </div>
      {profiles.length > 0 && <div className="team-relay-form-row">
        <label><span>当前服务</span><select value={profileId} disabled={Boolean(binding)} onChange={(event) => setProfileId(event.target.value)}>{profiles.map((profile) => <option key={profile.id} value={profile.id}>{profile.name}</option>)}</select></label>
        {selectedProfile && <small className="team-relay-url">{selectedProfile.baseUrl}</small>}
        <button className="icon-button" aria-label="删除当前团队服务配置" disabled={Boolean(busy) || Boolean(binding)} onClick={() => void deleteProfile()}><Trash2 size={14}/></button>
      </div>}
    </div>

    {selectedProfile && <div className="plugin-report">
      <strong>账号</strong>
      {selectedProfile.hasAccountSession ? <div className="team-relay-account-status">
        <div><strong>{selectedProfile.accountEmail}</strong><small>账号会话有效至 {selectedProfile.accountSessionExpiresAt ? new Date(selectedProfile.accountSessionExpiresAt).toLocaleString() : "未知"}</small></div>
        <button className="mini-button" disabled={Boolean(busy)} onClick={() => void logout()}><LogOut size={13}/>退出</button>
      </div> : <>
        <div className="team-relay-form-row">
          <label><span>邮箱</span><input type="email" value={email} onChange={(event) => setEmail(event.target.value)} autoComplete="username"/></label>
          <label><span>显示名称</span><input value={displayName} onChange={(event) => setDisplayName(event.target.value)} maxLength={256}/></label>
          <label className="team-relay-password"><span>密码</span><div><input type={showPassword ? "text" : "password"} value={password} onChange={(event) => setPassword(event.target.value)} minLength={12} maxLength={1024} autoComplete="current-password"/><button type="button" className="icon-button" aria-label={showPassword ? "隐藏密码" : "显示密码"} onClick={() => setShowPassword((value) => !value)}>{showPassword ? <EyeOff size={14}/> : <Eye size={14}/>}</button></div></label>
        </div>
        <div className="backup-actions">
          <button className="mini-button" disabled={Boolean(busy) || !email.trim() || password.length < 12} onClick={() => void authenticate("login")}><LogIn size={13}/>登录</button>
          <button className="mini-button" disabled={Boolean(busy) || !email.trim() || password.length < 12 || !displayName.trim()} onClick={() => void authenticate("register")}><UserPlus size={13}/>注册</button>
        </div>
      </>}
    </div>}

    {selectedProfile?.hasAccountSession && <div className="plugin-report">
      <strong>工作区接入</strong>
      {binding ? <div className="team-relay-account-status"><div><strong>{selectedWorkspace?.name}</strong><small>{binding.profileName} · 最近同步 {binding.lastSyncedAt ? new Date(binding.lastSyncedAt).toLocaleString() : "尚未完成"}</small></div><button className="mini-button" disabled={Boolean(busy)} onClick={() => void sync()}><RefreshCw size={13}/>立即同步</button></div> : <button className="mini-button" disabled={Boolean(busy) || !selectedWorkspace} onClick={() => void publish()}><CloudUpload size={13}/>发布当前工作区</button>}
      <div className="team-relay-form-row">
        <label><span>邀请令牌</span><input value={acceptToken} onChange={(event) => setAcceptToken(event.target.value)} maxLength={128} autoCapitalize="none" spellCheck={false}/></label>
        <label><span>本机设备名称</span><input value={deviceName} onChange={(event) => setDeviceName(event.target.value)} maxLength={256}/></label>
        <button className="mini-button" disabled={Boolean(busy) || !acceptToken.trim() || !deviceName.trim()} onClick={() => void acceptInvitation()}><Send size={13}/>接受邀请</button>
      </div>
    </div>}

    {binding && canManageMembers && <div className="plugin-report">
      <strong>在线邀请</strong>
      <div className="team-relay-form-row">
        <label><span>成员邮箱</span><input type="email" value={inviteEmail} onChange={(event) => setInviteEmail(event.target.value)}/></label>
        <label><span>角色</span><select value={inviteRole} onChange={(event) => setInviteRole(event.target.value)}><option value="viewer">Viewer</option><option value="operator">Operator</option><option value="admin">Admin</option></select></label>
        <button className="mini-button" disabled={Boolean(busy) || !inviteEmail.trim()} onClick={() => void createInvitation()}><UserPlus size={13}/>创建邀请</button>
      </div>
      {invitation && <div className="team-relay-token" aria-live="polite"><div><small>{invitation.email} · {invitation.role} · 有效至 {new Date(invitation.expiresAt).toLocaleString()}</small><code>{invitation.token}</code></div><button className="mini-button" onClick={() => void navigator.clipboard.writeText(invitation.token)}><Copy size={13}/>复制令牌</button></div>}
    </div>}
  </div>;
}
