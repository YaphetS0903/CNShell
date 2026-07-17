import { useEffect, useMemo, useState } from "react";
import { AlertTriangle, CheckCircle2, Eye, EyeOff, FolderOpen, HardDrive, KeyRound, LoaderCircle, Monitor, RefreshCw, ShieldCheck, TerminalSquare, Usb, Volume2, X } from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import type { AuthType, Fido2Identity, Folder, Protocol, ProxyProfile, RdpConnectionOptions, RdpDisplay, SaveConnectionInput, SerialConnectionOptions, SerialDeviceInfo, SshCertificateInfo } from "../../types";
import { Modal } from "../../components/Modal";
import { errorMessage } from "../../lib/format";
import "./ConnectionEditor.css";
import { TerminalPreferencesFields } from "../settings/TerminalPreferencesFields";
import { usePlatformCapabilities } from "../../lib/platform";

export function ConnectionEditor() {
  const platform=usePlatformCapabilities();
  const { connectionEditorOpen, editingConnection, closeConnectionEditor, refreshConnections, setError, settings, saveSettings } = useAppStore();
  const initial = useMemo<SaveConnectionInput>(() => editingConnection ? { ...editingConnection, credential: "" } : {
    id: crypto.randomUUID(), folderId: null, protocol: "ssh", name: "", host: "", port: 22, username: "root", authType: "password", privateKeyPath: null, certificatePath: null, hostKeyPolicy: "strict", note: "", tags: [], encoding: "UTF-8", startupCommand: null, proxyId: null, environment: {}, credential: ""
  }, [editingConnection]);
  const [form, setForm] = useState(initial);
  const [showSecret, setShowSecret] = useState(false);
  const [saving, setSaving] = useState(false);
  const [proxies, setProxies] = useState<ProxyProfile[]>([]);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [certificateInfo, setCertificateInfo] = useState<SshCertificateInfo | null>(null);
  const [fido2Identities, setFido2Identities] = useState<Fido2Identity[] | null>(null);
  const [fido2Busy, setFido2Busy] = useState(false);
  const [rdpOptions,setRdpOptions]=useState<RdpConnectionOptions>(()=>defaultRdpOptions(initial.id));
  const [rdpDisplays,setRdpDisplays]=useState<RdpDisplay[]>([]);
  const [serialOptions,setSerialOptions]=useState<SerialConnectionOptions>(()=>defaultSerialOptions(initial.id));
  const [serialDevices,setSerialDevices]=useState<SerialDeviceInfo[]>([]);
  const [overrideEnabled,setOverrideEnabled]=useState(false);
  const [terminalOverride,setTerminalOverride]=useState(settings.terminal);
  useEffect(() => { void Promise.all([api.listProxies().then(setProxies),api.listFolders().then(setFolders)]); }, []);
  useEffect(() => {
    if (!connectionEditorOpen) return;
    setForm(initial);
    setShowSecret(false);
    setCertificateInfo(null);
    setFido2Identities(null);
    setRdpOptions(defaultRdpOptions(initial.id));
    setRdpDisplays([]);
    setSerialOptions(defaultSerialOptions(initial.id));
    setSerialDevices([]);
    if(editingConnection?.protocol==="rdp"){
      void api.getRdpOptions(initial.id).then(setRdpOptions).catch((error)=>setError(errorMessage(error)));
      void api.rdpDisplays().then(setRdpDisplays).catch((error)=>setError(errorMessage(error)));
    }
    if(editingConnection?.protocol==="serial"){
      void api.getSerialOptions(initial.id).then(setSerialOptions).catch((error)=>setError(errorMessage(error)));
      void api.serialDevices().then(setSerialDevices).catch((error)=>setError(errorMessage(error)));
    }
    const override=settings.terminalOverrides[initial.id];
    setOverrideEnabled(Boolean(override));
    setTerminalOverride(override??settings.terminal);
  }, [connectionEditorOpen, editingConnection?.protocol, initial, setError, settings]);
  if (!connectionEditorOpen) return null;
  const change = <K extends keyof SaveConnectionInput>(key: K, value: SaveConnectionInput[K]) => setForm((current) => ({ ...current, [key]: value }));
  const submit = async (event: React.FormEvent) => {
    event.preventDefault(); setSaving(true);
    try { await api.saveConnection(form);if(form.protocol==="rdp")await api.saveRdpOptions({...rdpOptions,connectionId:form.id});if(form.protocol==="serial")await api.saveSerialOptions({...serialOptions,connectionId:form.id});const terminalOverrides={...settings.terminalOverrides};if(form.protocol==="ssh"&&overrideEnabled)terminalOverrides[form.id]=terminalOverride;else delete terminalOverrides[form.id];await saveSettings({...settings,terminalOverrides});await refreshConnections(); closeConnectionEditor(); }
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
  const chooseCertificate = async () => {
    if (!api.isDesktop()) {
      setError("选择 SSH Certificate 需要运行 CNshell 桌面版");
      return;
    }
    try {
      const selected = await open({ multiple: false, directory: false });
      if (!selected) return;
      const info = await api.inspectSshCertificate(selected);
      change("certificatePath", selected);
      setCertificateInfo(info);
    } catch (error) {
      setError(errorMessage(error));
    }
  };
  const detectFido2 = async () => {
    setFido2Busy(true);
    try {
      setFido2Identities(await api.listFido2Identities());
    } catch (error) {
      setFido2Identities([]);
      setError(errorMessage(error));
    } finally {
      setFido2Busy(false);
    }
  };
  const chooseRdpDrive = async () => {
    if(!confirm("映射后，远端 Windows 会获得所选本地文件夹的读写权限。仅选择当前主机确实需要的目录。"))return;
    try {
      const selected=await open({multiple:false,directory:true,title:"选择允许远程桌面读写的文件夹"});
      if(selected)setRdpOptions((current)=>({...current,drivePath:selected}));
    } catch(error){setError(errorMessage(error));}
  };
  const refreshSerialDevices = async () => {
    try { const devices=await api.serialDevices();setSerialDevices(devices);setForm((current)=>current.protocol==="serial"&&!current.host&&devices[0]?{...current,host:devices[0].path}:current); }
    catch(error){setError(errorMessage(error));}
  };
  return <Modal title={editingConnection ? "编辑连接" : "新建连接"} onClose={closeConnectionEditor} wide>
    <form onSubmit={submit} className="connection-form">
      <div className="form-section-title"><span><ShieldCheck size={17}/>常规</span><small>连接资料仅保存在这台电脑</small></div>
      <div className="form-grid">
        <label><span>协议</span><select value={form.protocol} onChange={(event) => { const protocol = event.target.value as Protocol;setForm((current)=>protocol==="rdp"?{...current,protocol,port:3389,authType:"password",credential:"",privateKeyPath:null,certificatePath:null,proxyId:null,startupCommand:null,environment:{}}:protocol==="local"?{...current,protocol,host:"localhost",port:0,username:current.username||"本机用户",authType:"none",credential:"",privateKeyPath:null,certificatePath:null,proxyId:null,hostKeyPolicy:"strict",environment:current.protocol==="serial"?{}:current.environment}:protocol==="telnet"?{...current,protocol,port:23,username:"anonymous",authType:"none",credential:"",privateKeyPath:null,certificatePath:null,proxyId:null,hostKeyPolicy:"strict",startupCommand:null,environment:{}}:protocol==="serial"?{...current,protocol,host:"",port:115200,username:"serial",authType:"none",credential:"",privateKeyPath:null,certificatePath:null,proxyId:null,hostKeyPolicy:"strict",startupCommand:null,environment:{}}:{...current,protocol,port:22,authType:current.authType==="none"?"password":current.authType,environment:current.protocol==="serial"?{}:current.environment});setCertificateInfo(null);if(protocol==="rdp"){setRdpOptions(defaultRdpOptions(form.id));void api.rdpDisplays().then(setRdpDisplays).catch(()=>setRdpDisplays([]));}if(protocol==="serial"){setSerialOptions(defaultSerialOptions(form.id));void refreshSerialDevices();} }}><option value="ssh">SSH / Linux</option><option value="rdp">远程桌面 / Windows</option><option value="local">本地 Shell</option><option value="telnet">Telnet（未加密）</option><option value="serial">Serial 串口</option></select></label>
        <label><span>名称</span><input required autoFocus value={form.name} onChange={(event) => change("name", event.target.value)} placeholder="生产服务器" /></label>
        {form.protocol === "serial" ? <><label className="span-2"><span>串口设备</span><div className="private-key-field"><select aria-label="串口设备" value={serialDevices.some((item)=>item.path===form.host)?form.host:""} onChange={(event)=>change("host",event.target.value)}><option value="">手动输入设备路径</option>{serialDevices.map((device)=><option key={device.path} value={device.path}>{device.label} · {device.path}</option>)}</select><button type="button" className="button secondary" onClick={()=>void refreshSerialDevices()}><RefreshCw size={14}/>刷新</button></div></label><label><span>设备路径</span><input required value={form.host} onChange={(event)=>change("host",event.target.value)} placeholder={platform.operatingSystem==="windows"?"COM3":"/dev/cu.usbserial-..."} aria-label="设备路径"/></label><label><span>波特率</span><select value={form.port} onChange={(event)=>change("port",Number(event.target.value))}>{[300,1200,2400,4800,9600,19200,38400,57600,115200,230400,460800,921600].map((rate)=><option key={rate} value={rate}>{rate}</option>)}</select></label></> : form.protocol !== "local" && <div className="span-2 field-group"><label htmlFor="connection-host">主机</label><div className="joined-fields"><input id="connection-host" required value={form.host} onChange={(event) => change("host", event.target.value)} placeholder="server.example.com"/><input className="port-input" required type="number" min="1" max="65535" value={form.port} onChange={(event) => change("port", Number(event.target.value))} aria-label="端口"/></div></div>}
        {form.protocol === "local" ? <label><span>本机用户</span><input value={form.username} onChange={(event) => change("username", event.target.value)} placeholder={`当前 ${platform.displayName} 用户`} /></label> : form.protocol !== "serial" && <label><span>用户名</span><input required value={form.username} onChange={(event) => change("username", event.target.value)} autoComplete="username" /></label>}
        <label><span>标签</span><input value={form.tags.join(", ")} onChange={(event) => change("tags", event.target.value.split(",").map((value) => value.trim()).filter(Boolean))} placeholder="生产, 香港" /></label>
        <label className="span-2"><span>文件夹</span><select value={form.folderId??""} onChange={(event)=>change("folderId",event.target.value||null)}><option value="">未分组</option>{folderOptions(folders).map((folder)=><option key={folder.id} value={folder.id}>{folder.label}</option>)}</select></label>
      </div>
      {form.protocol === "ssh" && <>
        <div className="form-section-title"><span><KeyRound size={17}/>认证</span><small>密码与私钥口令写入{platform.credentialStoreName}</small></div>
        <div className="form-grid">
          <label><span>认证方式</span><select value={form.authType} onChange={(event) => {const authType=event.target.value as AuthType;const usesKey=authType==="privateKey"||authType==="sshCertificate";setForm((current)=>({...current,authType,credential:"",privateKeyPath:usesKey?current.privateKeyPath:null,certificatePath:authType==="sshCertificate"?current.certificatePath:null}));if(authType!=="sshCertificate")setCertificateInfo(null);if(authType==="fido2Agent"&&fido2Identities===null)void detectFido2();}}><option value="password">密码</option><option value="privateKey">私钥</option><option value="sshCertificate">SSH Certificate</option><option value="sshAgent">SSH Agent</option><option value="fido2Agent">FIDO2 硬件密钥</option></select></label>
          {!(["sshAgent","fido2Agent"] as AuthType[]).includes(form.authType) && <label><span>{form.authType === "password" ? "密码" : "私钥口令"}</span><div className="password-field"><input type={showSecret ? "text" : "password"} value={form.credential ?? ""} onChange={(event) => change("credential", event.target.value)} placeholder={editingConnection?.hasCredential ? "留空以保留已保存凭据" : ""} autoComplete="new-password"/><button type="button" onClick={() => setShowSecret(!showSecret)} aria-label={showSecret ? "隐藏密码" : "显示密码"}>{showSecret ? <EyeOff size={16}/> : <Eye size={16}/>}</button></div></label>}
          {form.authType === "fido2Agent" && <div className="span-2 field-group"><div className="fido2-heading"><label>Agent 中的 FIDO2 身份</label><button type="button" className="button secondary" onClick={()=>void detectFido2()} disabled={fido2Busy}><RefreshCw size={14} className={fido2Busy?"spin":undefined}/>{fido2Busy?"检测中…":"重新检测"}</button></div><small>仅尝试 sk-ssh-ed25519 / sk-ecdsa 硬件身份；普通 Agent 密钥不会被使用。连接时由系统 OpenSSH Agent 显示触摸或 PIN 提示。</small>{fido2Identities!==null&&<div className="fido2-identities">{fido2Identities.length===0?<span className="warning">未检测到硬件身份</span>:fido2Identities.map((identity)=><div key={`${identity.keyType}-${identity.fingerprint}`}><strong>{identity.comment||"未命名硬件密钥"}</strong><code>{identity.keyType}</code><small>{identity.fingerprint}</small></div>)}</div>}</div>}
          {(form.authType === "privateKey" || form.authType === "sshCertificate") && <div className="span-2 field-group"><label htmlFor="private-key-path">私钥路径</label><div className="private-key-field"><input id="private-key-path" required value={form.privateKeyPath ?? ""} onChange={(event) => change("privateKeyPath", event.target.value)} placeholder={platform.operatingSystem==="windows"?"C:\\Users\\me\\.ssh\\id_ed25519":"/Users/me/.ssh/id_ed25519"} /><button type="button" className="button secondary" aria-label="选择私钥" onClick={() => void choosePrivateKey()}><FolderOpen size={15}/>选择…</button></div><small>可选择 OpenSSH 私钥，也可直接输入绝对路径。</small></div>}
          {form.authType === "sshCertificate" && <div className="span-2 field-group"><label htmlFor="certificate-path">证书路径</label><div className="private-key-field"><input id="certificate-path" required value={form.certificatePath ?? ""} onChange={(event) => {change("certificatePath",event.target.value);setCertificateInfo(null);}} placeholder={platform.operatingSystem==="windows"?"C:\\Users\\me\\.ssh\\id_ed25519-cert.pub":"/Users/me/.ssh/id_ed25519-cert.pub"} /><button type="button" className="button secondary" aria-label="选择证书" onClick={() => void chooseCertificate()}><FolderOpen size={15}/>选择…</button></div><small>只接受 OpenSSH 用户证书；连接时会再次检查有效期。</small>{certificateInfo&&<dl className={`certificate-info ${certificateInfo.validNow?"":"warning"}`}><div><dt>Key ID</dt><dd>{certificateInfo.keyId||"未设置"}</dd></div><div><dt>序列号</dt><dd>{certificateInfo.serial}</dd></div><div><dt>签发 CA</dt><dd>{certificateInfo.signingCa}</dd></div><div><dt>有效期</dt><dd>{certificateInfo.validFrom} 至 {certificateInfo.validTo}</dd></div><div><dt>主体</dt><dd>{certificateInfo.principals.length?certificateInfo.principals.join(", "):"未限制"}</dd></div><div><dt>状态</dt><dd>{certificateInfo.status==="valid"?"有效":certificateInfo.status==="expired"?"已过期":"尚未生效"}</dd></div></dl>}</div>}
          <label><span>主机密钥策略</span><select value={form.hostKeyPolicy} onChange={(event) => change("hostKeyPolicy", event.target.value as "strict"|"acceptNew")}><option value="strict">首次人工核对（推荐）</option><option value="acceptNew">自动信任首次密钥（有风险）</option></select></label>
          <label><span>终端编码</span><select value={form.encoding} onChange={(event) => change("encoding", event.target.value)}><option>UTF-8</option></select></label>
          <label><span>代理 / 跳板机</span><select value={form.proxyId ?? ""} onChange={(event) => change("proxyId", event.target.value || null)}><option value="">直接连接</option>{proxies.map((proxy) => <option key={proxy.id} value={proxy.id}>{proxy.name} · {proxy.type}</option>)}</select></label>
          <label className="span-2"><span>启动命令</span><input value={form.startupCommand ?? ""} onChange={(event) => change("startupCommand", event.target.value || null)} placeholder="例如：tmux attach || tmux" /></label>
          <label className="span-2"><span>环境变量</span><input value={Object.entries(form.environment).map(([key,value])=>`${key}=${value}`).join("; ")} onChange={(event)=>change("environment",parseEnvironment(event.target.value))} placeholder="LANG=zh_CN.UTF-8; APP_ENV=production"/></label>
        </div>
      </>}
      {form.protocol === "local" && <>
        <div className="form-section-title"><span><TerminalSquare size={17}/>本地 Shell</span><small>{platform.operatingSystem==="windows"?"使用 PowerShell 或 cmd，通过 ConPTY 运行":"使用当前系统默认 Shell，通过独立 PTY 运行"}</small></div>
        <div className="form-grid"><label className="span-2"><span>启动命令（可选）</span><input value={form.startupCommand ?? ""} onChange={(event) => change("startupCommand", event.target.value || null)} placeholder="例如：tmux attach || tmux" /></label><label className="span-2"><span>环境变量</span><input value={Object.entries(form.environment).map(([key,value])=>`${key}=${value}`).join("; ")} onChange={(event)=>change("environment",parseEnvironment(event.target.value))} placeholder="LANG=zh_CN.UTF-8; APP_ENV=development" /></label></div>
      </>}
      {form.protocol === "telnet" && <div className="inline-warning"><AlertTriangle size={15}/><span>Telnet 未加密，用户名、命令和服务器输出都可能被网络中的其他人读取。CNshell 不保存 Telnet 密码；请只在受控内网或遗留设备维护场景使用。</span></div>}
      {form.protocol === "serial" && <><div className="form-section-title"><span><Usb size={17}/>串口参数</span><small>独占设备；拔出后自动等待同一路径重新接入</small></div><div className="form-grid"><label><span>数据位</span><select value={serialOptions.dataBits} onChange={(event)=>setSerialOptions({...serialOptions,dataBits:Number(event.target.value)})}>{[5,6,7,8].map((bits)=><option key={bits} value={bits}>{bits}</option>)}</select></label><label><span>校验位</span><select value={serialOptions.parity} onChange={(event)=>setSerialOptions({...serialOptions,parity:event.target.value})}><option value="none">无</option><option value="odd">奇校验</option><option value="even">偶校验</option></select></label><label><span>停止位</span><select value={serialOptions.stopBits} onChange={(event)=>setSerialOptions({...serialOptions,stopBits:Number(event.target.value)})}><option value={1}>1</option><option value={2}>2</option></select></label><label><span>流控</span><select value={serialOptions.flowControl} onChange={(event)=>setSerialOptions({...serialOptions,flowControl:event.target.value})}><option value="none">无</option><option value="software">软件 XON/XOFF</option><option value="hardware">硬件 RTS/CTS</option></select></label><label className="check-row"><input type="checkbox" checked={serialOptions.dtr} onChange={(event)=>setSerialOptions({...serialOptions,dtr:event.target.checked})}/><span>连接时启用 DTR</span></label><label className="check-row"><input type="checkbox" checked={serialOptions.rts} onChange={(event)=>setSerialOptions({...serialOptions,rts:event.target.checked})}/><span>连接时启用 RTS</span></label></div></>}
      {form.protocol === "rdp" && <>
        <div className="form-section-title"><span><KeyRound size={17}/>远程桌面认证</span><small>密码通过 stdin 安全传递给 FreeRDP</small></div>
        <div className="form-grid"><label className="span-2"><span>Windows 密码</span><div className="password-field"><input required={!editingConnection?.hasCredential} type={showSecret?"text":"password"} value={form.credential??""} onChange={(event)=>change("credential",event.target.value)} placeholder={editingConnection?.hasCredential?"留空以保留已保存凭据":""} autoComplete="new-password"/><button type="button" onClick={()=>setShowSecret(!showSecret)} aria-label={showSecret?"隐藏密码":"显示密码"}>{showSecret?<EyeOff size={16}/>:<Eye size={16}/>}</button></div></label></div>
        <div className="form-section-title"><span><Monitor size={17}/>显示与性能</span><small>由内置 FreeRDP 原生窗口渲染</small></div>
        <div className="form-grid">
          <label><span>窗口模式</span><select value={rdpOptions.displayMode} onChange={(event)=>setRdpOptions({...rdpOptions,displayMode:event.target.value as RdpConnectionOptions["displayMode"]})}><option value="window">独立窗口</option><option value="fullscreen">全屏</option></select></label>
          <label><span>本机显示器</span><select value={rdpOptions.displayId??""} disabled={rdpOptions.displayMode!=="fullscreen"} onChange={(event)=>setRdpOptions({...rdpOptions,displayId:event.target.value?Number(event.target.value):null})}><option value="">系统主显示器</option>{rdpDisplays.map((display)=><option key={display.id} value={display.id}>{display.name} · {display.width}×{display.height}{display.primary?" · 主屏":""}</option>)}</select></label>
          <label><span>缩放</span><select value={rdpOptions.scaleMode} onChange={(event)=>setRdpOptions({...rdpOptions,scaleMode:event.target.value as RdpConnectionOptions["scaleMode"]})}><option value="dynamic">动态分辨率</option><option value="fit">缩放适应窗口</option><option value="native">远端原始尺寸</option></select></label>
          <label><span>画质 / 带宽</span><select value={rdpOptions.quality} onChange={(event)=>setRdpOptions({...rdpOptions,quality:event.target.value as RdpConnectionOptions["quality"]})}><option value="auto">自动检测</option><option value="lowBandwidth">低带宽</option><option value="balanced">均衡</option><option value="highQuality">高画质 / 局域网</option></select></label>
        </div>
        <div className="form-section-title"><span><Volume2 size={17}/>重定向权限</span><small>默认只开启文本剪贴板</small></div>
        <div className="rdp-permissions">
          <label className="check-row"><input type="checkbox" checked={rdpOptions.clipboard} onChange={(event)=>setRdpOptions({...rdpOptions,clipboard:event.target.checked})}/><span>允许双向剪贴板</span></label>
          <label><span>远端声音</span><select value={rdpOptions.audioMode} onChange={(event)=>setRdpOptions({...rdpOptions,audioMode:event.target.value as RdpConnectionOptions["audioMode"]})}><option value="off">关闭</option><option value="local">在这台电脑播放</option><option value="remote">留在远端播放</option></select></label>
          <label className="check-row"><input type="checkbox" checked={rdpOptions.microphone} onChange={(event)=>{if(event.target.checked&&!confirm("允许远端 Windows 使用这台电脑的麦克风？"))return;setRdpOptions({...rdpOptions,microphone:event.target.checked});}}/><span>允许麦克风重定向</span></label>
          <div className="span-2 field-group"><label>本地目录映射</label><div className="private-key-field"><input readOnly value={rdpOptions.drivePath??""} placeholder="默认关闭" aria-label="RDP 映射目录"/><button type="button" className="button secondary" onClick={()=>void chooseRdpDrive()}><HardDrive size={14}/>选择目录</button>{rdpOptions.drivePath&&<button type="button" className="icon-button" aria-label="移除 RDP 映射目录" onClick={()=>setRdpOptions({...rdpOptions,drivePath:null})}><X size={14}/></button>}</div><small>映射目录会向远端提供读写权限；CNshell 只授权你选择的这一个文件夹。</small></div>
        </div>
      </>}
      {form.protocol === "ssh" && <><div className="form-section-title"><span><TerminalSquare size={17}/>终端偏好</span><small>默认跟随全局设置</small></div><label className="check-row"><input type="checkbox" checked={overrideEnabled} onChange={(event)=>setOverrideEnabled(event.target.checked)}/><span>为此连接覆盖全局终端偏好</span></label>{overrideEnabled&&<TerminalPreferencesFields idPrefix={`connection-terminal-${form.id}`} value={terminalOverride} onChange={setTerminalOverride}/>}</>}
      <div className="form-section-title"><span><CheckCircle2 size={17}/>备注</span></div>
      <label><textarea rows={3} value={form.note} onChange={(event) => change("note", event.target.value)} placeholder="用途、负责人或环境说明" /></label>
      <footer className="form-actions"><button type="button" className="button secondary" onClick={closeConnectionEditor}>取消</button><button className="button primary" disabled={saving}>{saving && <LoaderCircle className="spin" size={16}/>}保存连接</button></footer>
    </form>
  </Modal>;
}

function parseEnvironment(value:string):Record<string,string>{const result:Record<string,string>={};for(const pair of value.split(";")){const[key,...rest]=pair.trim().split("=");if(key&&/^[A-Za-z_][A-Za-z0-9_]*$/.test(key))result[key]=rest.join("=");}return result;}
function defaultRdpOptions(connectionId:string):RdpConnectionOptions{return{connectionId,displayMode:"window",displayId:null,scaleMode:"dynamic",quality:"auto",clipboard:true,audioMode:"off",microphone:false,drivePath:null};}
function defaultSerialOptions(connectionId:string):SerialConnectionOptions{return{connectionId,dataBits:8,parity:"none",stopBits:1,flowControl:"none",dtr:true,rts:true};}
function folderOptions(folders:Folder[]){const byId=new Map(folders.map((folder)=>[folder.id,folder]));const label=(folder:Folder,seen=new Set<string>()):string=>{if(seen.has(folder.id))return folder.name;seen.add(folder.id);const parent=folder.parentId?byId.get(folder.parentId):undefined;return parent?`${label(parent,seen)} / ${folder.name}`:folder.name;};return folders.map((folder)=>({...folder,label:label(folder)})).sort((left,right)=>left.label.localeCompare(right.label,"zh-CN"));}
