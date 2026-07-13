import { useEffect, useMemo, useState } from "react";
import { CheckCircle2, Eye, EyeOff, FolderOpen, KeyRound, LoaderCircle, ShieldCheck, TerminalSquare } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type { AuthType, Folder, ProxyProfile, SaveConnectionInput } from "../../types";
import { Modal } from "../../components/Modal";
import { errorMessage } from "../../lib/format";
import "./ConnectionEditor.css";
import { TerminalPreferencesFields } from "../settings/TerminalPreferencesFields";

export function ConnectionEditor() {
  const { connectionEditorOpen, editingConnection, closeConnectionEditor, refreshConnections, setError, settings, saveSettings } = useAppStore();
  const initial = useMemo<SaveConnectionInput>(() => editingConnection ? { ...editingConnection, credential: "" } : {
    id: crypto.randomUUID(), folderId: null, protocol: "ssh", name: "", host: "", port: 22, username: "root", authType: "password", privateKeyPath: null, hostKeyPolicy: "strict", note: "", tags: [], encoding: "UTF-8", startupCommand: null, proxyId: null, environment: {}, credential: ""
  }, [editingConnection]);
  const [form, setForm] = useState(initial);
  const [showSecret, setShowSecret] = useState(false);
  const [saving, setSaving] = useState(false);
  const [proxies, setProxies] = useState<ProxyProfile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [overrideEnabled,setOverrideEnabled]=useState(false);
  const [terminalOverride,setTerminalOverride]=useState(settings.terminal);
  useEffect(() => { void Promise.all([api.listProxies().then(setProxies),api.listFolders().then(setFolders)]); }, []);
  useEffect(() => {
    if (!connectionEditorOpen) return;
    setForm(initial);
    setShowSecret(false);
    const override=settings.terminalOverrides[initial.id];
    setOverrideEnabled(Boolean(override));
    setTerminalOverride(override??settings.terminal);
  }, [connectionEditorOpen, initial, settings]);
  if (!connectionEditorOpen) return null;
  const change = <K extends keyof SaveConnectionInput>(key: K, value: SaveConnectionInput[K]) => setForm((current) => ({ ...current, [key]: value }));
  const submit = async (event: React.FormEvent) => {
    event.preventDefault(); setSaving(true);
    try { await api.saveConnection(form);const terminalOverrides={...settings.terminalOverrides};if(form.protocol==="ssh"&&overrideEnabled)terminalOverrides[form.id]=terminalOverride;else delete terminalOverrides[form.id];await saveSettings({...settings,terminalOverrides});await refreshConnections(); closeConnectionEditor(); }
    catch (error) { setError(errorMessage(error)); } finally { setSaving(false); }
  };
  const choosePrivateKey = async () => {
    if (!api.isDesktop()) {
      setError("选择私钥需要运行 CNshell 桌面版");
      return;
    }
    try {
      const selected = await open({ multiple: false, directory: false });
      if (selected) change("privateKeyPath", selected);
    } catch (error) {
      setError(errorMessage(error));
    }
  };
  return <Modal title={editingConnection ? "编辑连接" : "新建连接"} onClose={closeConnectionEditor} wide>
    <form onSubmit={submit} className="connection-form">
      <div className="form-section-title"><span><ShieldCheck size={17}/>常规</span><small>连接资料仅保存在这台 Mac</small></div>
      <div className="form-grid">
        <label><span>协议</span><select value={form.protocol} onChange={(event) => { const protocol = event.target.value as "ssh"|"rdp";setForm((current)=>protocol==="rdp"?{...current,protocol,port:3389,authType:"password",credential:"",privateKeyPath:null,proxyId:null,startupCommand:null,environment:{}}:{...current,protocol,port:22}); }}><option value="ssh">SSH / Linux</option><option value="rdp">远程桌面 / Windows</option></select></label>
        <label><span>名称</span><input required autoFocus value={form.name} onChange={(event) => change("name", event.target.value)} placeholder="生产服务器" /></label>
        <div className="span-2 field-group"><label htmlFor="connection-host">主机</label><div className="joined-fields"><input id="connection-host" required value={form.host} onChange={(event) => change("host", event.target.value)} placeholder="server.example.com"/><input className="port-input" required type="number" min="1" max="65535" value={form.port} onChange={(event) => change("port", Number(event.target.value))} aria-label="端口"/></div></div>
        <label><span>用户名</span><input required value={form.username} onChange={(event) => change("username", event.target.value)} autoComplete="username" /></label>
        <label><span>标签</span><input value={form.tags.join(", ")} onChange={(event) => change("tags", event.target.value.split(",").map((value) => value.trim()).filter(Boolean))} placeholder="生产, 香港" /></label>
        <label className="span-2"><span>文件夹</span><select value={form.folderId??""} onChange={(event)=>change("folderId",event.target.value||null)}><option value="">未分组</option>{folderOptions(folders).map((folder)=><option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>
      </div>
      {form.protocol === "ssh" && <>
        <div className="form-section-title"><span><KeyRound size={17}/>认证</span><small>密码与私钥口令写入 macOS Keychain</small></div>
        <div className="form-grid">
          <label><span>认证方式</span><select value={form.authType} onChange={(event) => {const authType=event.target.value as AuthType;setForm((current)=>({...current,authType,credential:"",privateKeyPath:authType==="privateKey"?current.privateKeyPath:null}));}}><option value="password">密码</option><option value="privateKey">私钥</option><option value="sshAgent">SSH Agent</option></select></label>
          {form.authType !== "sshAgent" && <label><span>{form.authType === "password" ? "密码" : "私钥口令"}</span><div className="password-field"><input type={showSecret ? "text" : "password"} value={form.credential ?? ""} onChange={(event) => change("credential", event.target.value)} placeholder={editingConnection?.hasCredential ? "留空以保留已保存凭据" : ""} autoComplete="new-password"/><button type="button" onClick={() => setShowSecret(!showSecret)} aria-label={showSecret ? "隐藏密码" : "显示密码"}>{showSecret ? <EyeOff size={16}/> : <Eye size={16}/>}</button></div></label>}
          {form.authType === "privateKey" && <div className="span-2 field-group"><label htmlFor="private-key-path">私钥路径</label><div className="private-key-field"><input id="private-key-path" required value={form.privateKeyPath ?? ""} onChange={(event) => change("privateKeyPath", event.target.value)} placeholder="/Users/me/.ssh/id_ed25519" /><button type="button" className="button secondary" onClick={() => void choosePrivateKey()}><FolderOpen size={15}/>选择…</button></div><small>可选择 OpenSSH 私钥，也可直接输入绝对路径。</small></div>}
          <label><span>主机密钥策略</span><select value={form.hostKeyPolicy} onChange={(event) => change("hostKeyPolicy", event.target.value as "strict"|"acceptNew")}><option value="strict">首次人工核对（推荐）</option><option value="acceptNew">自动信任首次密钥（有风险）</option></select></label>
          <label><span>终端编码</span><select value={form.encoding} onChange={(event) => change("encoding", event.target.value)}><option>UTF-8</option></select></label>
          <label><span>代理 / 跳板机</span><select value={form.proxyId ?? ""} onChange={(event) => change("proxyId", event.target.value || null)}><option value="">直接连接</option>{proxies.map((proxy) => <option key={proxy.id} value={proxy.id}>{proxy.name} · {proxy.type}</option>)}</select></label>
          <label className="span-2"><span>启动命令</span><input value={form.startupCommand ?? ""} onChange={(event) => change("startupCommand", event.target.value || null)} placeholder="例如：tmux attach || tmux" /></label>
          <label className="span-2"><span>环境变量</span><input value={Object.entries(form.environment).map(([key,value])=>`${key}=${value}`).join("; ")} onChange={(event)=>change("environment",parseEnvironment(event.target.value))} placeholder="LANG=zh_CN.UTF-8; APP_ENV=production"/></label>
        </div>
      </>}
      {form.protocol === "rdp" && <>
        <div className="form-section-title"><span><KeyRound size={17}/>远程桌面认证</span><small>密码通过 stdin 安全传递给 FreeRDP</small></div>
        <div className="form-grid"><label className="span-2"><span>Windows 密码</span><div className="password-field"><input required={!editingConnection?.hasCredential} type={showSecret?"text":"password"} value={form.credential??""} onChange={(event)=>change("credential",event.target.value)} placeholder={editingConnection?.hasCredential?"留空以保留已保存凭据":""} autoComplete="new-password"/><button type="button" onClick={()=>setShowSecret(!showSecret)} aria-label={showSecret?"隐藏密码":"显示密码"}>{showSecret?<EyeOff size={16}/>:<Eye size={16}/>}</button></div></label></div>
      </>}
      {form.protocol === "ssh" && <><div className="form-section-title"><span><TerminalSquare size={17}/>终端偏好</span><small>默认跟随全局设置</small></div><label className="check-row"><input type="checkbox" checked={overrideEnabled} onChange={(event)=>setOverrideEnabled(event.target.checked)}/><span>为此连接覆盖全局终端偏好</span></label>{overrideEnabled&&<TerminalPreferencesFields idPrefix={`connection-terminal-${form.id}`} value={terminalOverride} onChange={setTerminalOverride}/>}</>}
      <div className="form-section-title"><span><CheckCircle2 size={17}/>备注</span></div>
      <label><textarea rows={3} value={form.note} onChange={(event) => change("note", event.target.value)} placeholder="用途、负责人或环境说明" /></label>
      <footer className="form-actions"><button type="button" className="button secondary" onClick={closeConnectionEditor}>取消</button><button className="button primary" disabled={saving}>{saving && <LoaderCircle className="spin" size={16}/>}保存连接</button></footer>
    </form>
  </Modal>;
}

function parseEnvironment(value:string):Record<string,string>{const result:Record<string,string>={};for(const pair of value.split(";")){const[key,...rest]=pair.trim().split("=");if(key&&/^[A-Za-z_][A-Za-z0-9_]*$/.test(key))result[key]=rest.join("=");}return result;}
function folderOptions(folders:Folder[]){const byId=new Map(folders.map((folder)=>[folder.id,folder]));const label=(folder:Folder,seen=new Set<string>()):string=>{if(seen.has(folder.id))return folder.name;seen.add(folder.id);const parent=folder.parentId?byId.get(folder.parentId):undefined;return parent?`${label(parent,seen)} / ${folder.name}`:folder.name;};return folders.map((folder)=>({...folder,label:label(folder)})).sort((left,right)=>left.label.localeCompare(right.label,"zh-CN"));}
