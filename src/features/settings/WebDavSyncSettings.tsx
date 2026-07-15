import { Cloud, KeyRound, Play, Save, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { BackgroundTask, SaveWebDavProfileInput, SyncOptions, SyncResult, WebDavProfile } from "../../types";

const defaultOptions: SyncOptions = { includeHosts: true, includePrivateKeyPaths: false, includeCredentials: false };

export function WebDavSyncSettings({ onError }: { onError: (message: string) => void }) {
  const [profiles, setProfiles] = useState<WebDavProfile[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [url, setUrl] = useState("");
  const [username, setUsername] = useState("");
  const [password, setPassword] = useState("");
  const [passphrase, setPassphrase] = useState("");
  const [syncOnStartup, setSyncOnStartup] = useState(false);
  const [startupPassphrase, setStartupPassphrase] = useState("");
  const [options, setOptions] = useState(defaultOptions);
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<SyncResult | null>(null);
  const [progress, setProgress] = useState<{ phase: string; transferredBytes: number; totalBytes: number } | null>(null);

  const select = (profile: WebDavProfile) => { setSelectedId(profile.id); setName(profile.name); setUrl(profile.url); setUsername(profile.username); setPassword(""); setSyncOnStartup(profile.syncOnStartup); setStartupPassphrase(""); setOptions(profile.syncOptions); setResult(null); };
  const load = useCallback(() => void api.listWebDavProfiles().then((items) => { setProfiles(items); if (items[0] && !selectedId) select(items[0]); }).catch((error) => onError(errorMessage(error))), [onError, selectedId]);
  useEffect(() => { load(); }, [load]);
  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status)) return;
    const timer = window.setInterval(() => {
      void api.getTask(task.id).then((next) => { setTask(next); if (next.status === "completed") setResult(next.result as SyncResult); }).catch((error) => onError(errorMessage(error)));
    }, 400);
    return () => window.clearInterval(timer);
  }, [task, onError]);
  useEffect(() => {
    let active = true;
    let unlisten: (() => void) | undefined;
    void api.onWebDavSyncProgress((next) => { if (active && next.profileId === selectedId) setProgress(next); }).then((stop) => { unlisten = stop; });
    return () => { active = false; unlisten?.(); };
  }, [selectedId]);

  const reset = () => { setSelectedId(""); setName(""); setUrl(""); setUsername(""); setPassword(""); setSyncOnStartup(false); setStartupPassphrase(""); setResult(null); };
  const save = async () => {
    try {
      const input: SaveWebDavProfileInput = { id: selectedId || crypto.randomUUID(), name, url, username, password: password || null, syncOnStartup, syncOptions: options, syncPassphrase: syncOnStartup ? (startupPassphrase || null) : "" };
      const saved = await api.saveWebDavProfile(input);
      setProfiles((current) => [...current.filter((item) => item.id !== saved.id), saved]);
      select(saved);
    } catch (error) { onError(errorMessage(error)); }
  };
  const remove = async () => {
    if (!selectedId || !confirm("删除这条 WebDAV 配置及 Keychain 密码？远端同步文件不会删除。")) return;
    try { await api.deleteWebDavProfile(selectedId); setProfiles((current) => current.filter((item) => item.id !== selectedId)); reset(); } catch (error) { onError(errorMessage(error)); }
  };
  const run = async (direction: "write" | "read") => {
    if (!selectedId || passphrase.length < 8) { onError("请选择 WebDAV 配置并输入至少 8 位同步口令"); return; }
    if (options.includeCredentials && !confirm("Keychain 凭据会在本机加密后上传，服务端只看到密文。确认继续？")) return;
    try { setResult(null); setProgress(null); setTask(direction === "write" ? await api.startWebDavWrite(selectedId, passphrase, options) : await api.startWebDavRead(selectedId, passphrase)); setPassphrase(""); } catch (error) { onError(errorMessage(error)); }
  };

  return <section className="webdav-sync" aria-label="WebDAV 同步">
    <div className="section-heading"><div><h3><Cloud size={16} /> WebDAV 同步</h3></div></div>
    <div className="webdav-profile-list">{profiles.map((profile) => <button key={profile.id} className={profile.id === selectedId ? "active" : ""} onClick={() => select(profile)}><strong>{profile.name}</strong><small>{profile.url} · {profile.hasCredential ? "已保存密码" : "未保存密码"}</small></button>)}</div>
    <div className="automation-meta">
      <label><span>名称</span><input value={name} onChange={(event) => setName(event.target.value)} /></label>
      <label><span>HTTPS 地址</span><input value={url} onChange={(event) => setUrl(event.target.value)} placeholder="https://dav.example/remote.php/dav/files/user/" /></label>
      <label><span>用户名</span><input value={username} onChange={(event) => setUsername(event.target.value)} /></label>
      <label><span>密码</span><input type="password" value={password} onChange={(event) => setPassword(event.target.value)} placeholder={selectedId ? "留空保持原密码" : "输入 WebDAV 密码"} /></label>
    </div>
    <label><span>同步口令（至少 8 位；默认不保存）</span><input type="password" value={passphrase} onChange={(event) => setPassphrase(event.target.value)} autoComplete="new-password" /></label>
    <div className="webdav-startup"><label className="check-row"><input type="checkbox" checked={syncOnStartup} onChange={(event) => setSyncOnStartup(event.target.checked)} /><span>启动时自动导入（需保存独立同步口令）</span></label>{syncOnStartup && <label><span>启动同步口令</span><input type="password" value={startupPassphrase} onChange={(event) => setStartupPassphrase(event.target.value)} placeholder="留空保持已保存口令" /></label>}<small>{syncOnStartup ? "CNshell 启动后只导入远端加密包；未保存口令时不会联网。" : "默认关闭启动同步。"}</small></div>
    <div className="sync-toggles"><label className="check-row"><input type="checkbox" checked={options.includeHosts} onChange={(event) => setOptions({ ...options, includeHosts: event.target.checked })} /><span>同步主机资料</span></label><label className="check-row"><input type="checkbox" checked={options.includePrivateKeyPaths} disabled={!options.includeHosts} onChange={(event) => setOptions({ ...options, includePrivateKeyPaths: event.target.checked })} /><span>同步私钥路径</span></label><label className="check-row"><input type="checkbox" checked={options.includeCredentials} disabled={!options.includeHosts} onChange={(event) => setOptions({ ...options, includeCredentials: event.target.checked })} /><span>同步 Keychain 凭据</span></label></div>
    <div className="backup-actions"><button className="button secondary" onClick={() => void save()}><Save size={14} /> 保存配置</button><button className="button secondary" disabled={!selectedId || !!task && !["completed", "failed", "cancelled"].includes(task.status)} onClick={() => void run("write")}><Play size={14} /> 上传</button><button className="button secondary" disabled={!selectedId || !!task && !["completed", "failed", "cancelled"].includes(task.status)} onClick={() => void run("read")}><KeyRound size={14} /> 下载导入</button><IconButton icon={Trash2} label="删除 WebDAV 配置" disabled={!selectedId} onClick={() => void remove()} /></div>
    {task && <p className="muted-copy" aria-live="polite">任务状态：{task.status}{task.error ? ` · ${task.error}` : ""}</p>}
    {progress && <p className="muted-copy" aria-live="polite">{progress.phase}{progress.totalBytes ? ` · ${progress.transferredBytes}/${progress.totalBytes} bytes` : ""}</p>}
    {result && <p className="muted-copy" aria-live="polite">已同步 {result.connectionCount} 条连接：{result.path}{result.conflictCopy ? `；冲突副本：${result.conflictCopy}` : ""}</p>}
  </section>;
}
