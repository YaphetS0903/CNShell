import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Ban,
  Download,
  FolderOpen,
  FilePlus2,
  FileSearch,
  KeyRound,
  Play,
  RefreshCw,
  ShieldAlert,
  ShieldCheck,
  Trash2,
} from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { waitForTask } from "../../lib/background-task";
import { errorMessage } from "../../lib/format";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import { useAppStore } from "../../store/app-store";
import type {
  ConnectionProfile,
  PluginAuditEvent,
  PluginInstallRecord,
  PluginPermissionReport,
  PluginPublisherRoot,
  PluginRunResult,
} from "../../types";

export function PluginSettings({ connections, onError }: { connections: ConnectionProfile[]; onError: (message: string) => void }) {
  const [report, setReport] = useState<PluginPermissionReport | null>(null);
  const [records, setRecords] = useState<PluginInstallRecord[]>([]);
  const [publishers, setPublishers] = useState<PluginPublisherRoot[]>([]);
  const [audit, setAudit] = useState<PluginAuditEvent[]>([]);
  const [lastRun, setLastRun] = useState<PluginRunResult | null>(null);
  const [busy, setBusy] = useState(false);
  const [connectionId, setConnectionId] = useState("");
  const [networkUrl, setNetworkUrl] = useState("");
  const [directoryPath, setDirectoryPath] = useState("");
  const [directoryRelativePath, setDirectoryRelativePath] = useState("");
  const [proxyStatus, setProxyStatus] = useState("");
  const activeSessionId = useAppStore((state) => state.activeSessionId);
  const activeSession = useAppStore((state) => state.sessions.find((session) => session.id === state.activeSessionId) ?? null);

  const refresh = useCallback(async () => {
    try {
      const [installed, roots, events] = await Promise.all([
        api.listPlugins(),
        api.listPluginPublishers(),
        api.listPluginAudit(),
      ]);
      setRecords(installed);
      setPublishers(roots);
      setAudit(events.slice(-8).reverse());
    } catch (error) {
      onError(errorMessage(error));
    }
  }, [onError]);

  useEffect(() => { void refresh(); }, [refresh]);

  const inspect = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Plugin manifest", extensions: ["json"] }] });
      if (path) setReport(await api.inspectPluginManifest(path));
    } catch (error) { onError(errorMessage(error)); }
  };

  const importPublisher = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Publisher key", extensions: ["json"] }] });
      if (!path) return;
      if (!confirm("信任发布者后，其有效签名插件可进入启用流程。请先通过独立渠道核对公钥指纹，是否继续？")) return;
      setBusy(true);
      await api.importPluginPublisher(path);
      await refresh();
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const register = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Plugin manifest", extensions: ["json"] }] });
      if (!path) return;
      if (!confirm("CNshell 会固定 manifest 与 WASM 摘要。只有受信任签名插件才能进入启用流程，敏感权限默认不授予，是否继续登记？")) return;
      setBusy(true);
      await api.registerPlugin(path);
      await refresh();
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const enable = async (record: PluginInstallRecord) => {
    const permissions = record.requestedPermissions.join(", ") || "无宿主权限";
    if (!confirm(`启用 ${record.name}？\n\n先授予低风险权限：ui（如已声明）。\n高风险权限将逐项再次确认。\n声明权限：${permissions}\n运行时：无 WASI、32 MB 内存、有限燃料`)) return;
    const granted: string[] = record.requestedPermissions.filter((permission) => permission === "ui");
    for (const permission of record.requestedPermissions.filter((value) => value !== "ui")) {
      if (confirm(`授予插件权限 ${permission}？\n\n该权限只对本机沙箱生效，并且每次运行仍有独立的数据范围或用户确认。`)) granted.push(permission);
    }
    try { setBusy(true); await api.enablePlugin(record.id, granted); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const disable = async (id: string) => {
    try { await api.disablePlugin(id); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const run = async (record: PluginInstallRecord) => {
    const needsConnection = record.grantedPermissions.some((permission) => permission === "connectionMetadata" || permission === "credentialProxy");
    const selectedText = workspaceRuntime.terminalSelectionBySession.get(activeSessionId ?? "") ?? "";
    if (needsConnection && !connectionId) { onError("请先为插件本次运行选择一个连接"); return; }
    if (record.grantedPermissions.includes("terminalRead") && !selectedText) { onError("请先在当前终端中选中文本；插件只能读取这次明确选中的内容"); return; }
    if (record.grantedPermissions.includes("network") && !networkUrl.trim()) { onError("请为插件本次运行填写 manifest 允许域名内的 HTTPS URL"); return; }
    if (record.grantedPermissions.includes("directory") && !directoryPath) { onError("请为插件本次运行选择一个本地目录"); return; }
    if (record.grantedPermissions.includes("terminalInput") && !activeSessionId) { onError("请先打开并选中一个终端会话"); return; }
    if (record.grantedPermissions.includes("network") && !confirm(`允许 ${record.name} 本次读取以下 HTTPS URL 的最多 64 KB 响应？\n\n${networkUrl.trim()}\n\n允许域名：${record.networkDomains.join(", ") || "manifest 未声明"}`)) return;
    try { setBusy(true); setProxyStatus(""); setLastRun(await api.runPlugin({id:record.id,connectionId:connectionId||null,selectedText:record.grantedPermissions.includes("terminalRead")?selectedText:null,networkUrl:record.grantedPermissions.includes("network")?networkUrl.trim():null,directoryPath:record.grantedPermissions.includes("directory")?directoryPath:null,directoryRelativePath:record.grantedPermissions.includes("directory")&&directoryRelativePath.trim()?directoryRelativePath.trim():null,terminalSessionId:record.grantedPermissions.includes("terminalInput")?activeSessionId:null})); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const approveProxy = async () => {
    const request = lastRun?.credentialProxyRequest;
    if (!request || !confirm(`${request.pluginName} 请求由 CNshell 对 ${request.connectionName} 执行一次连接诊断。插件不会获得密码、私钥或诊断明文，是否允许？`)) return;
    try { setBusy(true); const task = await api.approvePluginCredentialProxy(request.requestId); await waitForTask(task); setProxyStatus("一次性连接诊断已完成；凭据始终由 CNshell 后端持有。"); setLastRun({...lastRun,credentialProxyRequest:null}); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const rejectProxy = async () => {
    const request = lastRun?.credentialProxyRequest;
    if (!request) return;
    try { await api.rejectPluginCredentialProxy(request.requestId); setProxyStatus("已拒绝凭据代理请求，未读取或使用凭据。"); setLastRun({...lastRun,credentialProxyRequest:null}); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const approveTerminalInput = async () => {
    const request = lastRun?.terminalInputRequest;
    if (!request || !confirm(`${request.pluginName} 请求向当前会话发送以下完整内容：\n\n${JSON.stringify(request.data)}\n\n确认发送一次？`)) return;
    try { setBusy(true); await api.approvePluginTerminalInput(request.requestId); setProxyStatus("已向终端发送一次经确认的插件输入。"); setLastRun({...lastRun,terminalInputRequest:null}); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const rejectTerminalInput = async () => {
    const request = lastRun?.terminalInputRequest;
    if (!request) return;
    try { await api.rejectPluginTerminalInput(request.requestId); setProxyStatus("已拒绝插件终端输入，未发送任何数据。"); setLastRun({...lastRun,terminalInputRequest:null}); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const chooseDirectory = async () => {
    try { const path = await open({multiple:false,directory:true}); if (typeof path === "string") { setDirectoryPath(path); setDirectoryRelativePath(""); } }
    catch (error) { onError(errorMessage(error)); }
  };

  const revokePublisher = async (root: PluginPublisherRoot) => {
    if (!confirm(`撤销发布者 ${root.name}？其全部插件会立即禁用。`)) return;
    try { await api.revokePluginPublisher(root.id); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const remove = async (id: string) => {
    if (!confirm("移除插件登记？不会删除 manifest 或 WASM 文件。")) return;
    try { await api.removePlugin(id); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const exportAudit = async () => {
    try {
      const path = await save({ defaultPath: "cnshell-plugin-audit.json", filters: [{ name: "JSON", extensions: ["json"] }] });
      if (path) await api.exportPluginAudit(path);
    } catch (error) { onError(errorMessage(error)); }
  };

  return <section className="plugin-settings" aria-label="插件权限">
    <div className="section-heading">
      <div><h3><ShieldAlert size={16} /> 插件信任与沙箱</h3><p>插件只在签名和摘要通过后进入无 WASI 的限额 WebAssembly 沙箱；敏感权限默认不授予。</p></div>
      <button className="mini-button" onClick={() => void refresh()} disabled={busy}><RefreshCw size={13}/>刷新</button>
    </div>
    <div className="backup-actions">
      <button className="button secondary" onClick={() => void importPublisher()} disabled={busy}><KeyRound size={14} /> 导入发布者根</button>
      <button className="button secondary" onClick={() => void inspect()}><FileSearch size={14} /> 检查 manifest</button>
      <button className="button secondary" onClick={() => void register()} disabled={busy}><FilePlus2 size={14} /> 登记插件</button>
    </div>
    {report && <div className="plugin-report" aria-live="polite">
      <strong>{report.manifest.name} {report.manifest.version}</strong>
      <small>{report.manifest.id} · 签名：{report.signatureStatus}</small>
      <div><span>可授予：{report.defaultGrantedPermissions.join(", ") || "无"}</span><span>当前未开放：{report.deniedPermissions.join(", ") || "无"}</span></div>
      {report.warnings.map((warning) => <p key={warning}>{warning}</p>)}
    </div>}
    <div className="plugin-report">
      <strong>受信任发布者</strong>
      {publishers.length === 0 ? <p>暂无发布者根，第三方插件不能执行。</p> : publishers.map((root) => <div className="plugin-record" key={root.id}>
        <div><strong>{root.name}</strong><small>{root.id} · {root.enabled ? "已信任" : "已撤销"}</small><small>SHA-256 {root.fingerprint.replace("sha256:", "").slice(0, 24)}…</small></div>
        {root.enabled && <button className="mini-button" onClick={() => void revokePublisher(root)}><Ban size={13}/>撤销</button>}
      </div>)}
    </div>
    <div className="plugin-report">
      <strong>已登记插件</strong>
      <label><span>本次运行使用的连接（仅按插件权限提供有界元数据或代办诊断）</span><select value={connectionId} onChange={(event)=>setConnectionId(event.target.value)}><option value="">不提供连接</option>{connections.map((connection)=><option key={connection.id} value={connection.id}>{connection.name} · {connection.protocol.toUpperCase()}</option>)}</select></label>
      <label><span>本次 HTTPS GET（只允许插件 manifest 中的精确域名、443 端口、无重定向）</span><input value={networkUrl} onChange={(event)=>setNetworkUrl(event.target.value)} placeholder="https://api.example.com/status" /></label>
      <label><span>本次只读目录</span><div className="backup-actions"><button className="button secondary" onClick={()=>void chooseDirectory()}><FolderOpen size={14}/>选择目录</button><code>{directoryPath||"尚未选择"}</code></div></label>
      <label><span>目录内单个文件（可选，相对路径，最大 64 KB）</span><input value={directoryRelativePath} onChange={(event)=>setDirectoryRelativePath(event.target.value)} disabled={!directoryPath} placeholder="status.json" /></label>
      <small>终端只读只使用当前选中文本；终端输入目标为当前会话：{activeSession?.title??"未打开终端"}，插件提出内容后仍需再次确认。</small>
      {records.length === 0 ? <p>暂无插件登记。</p> : records.map((record) => <div className="plugin-record" key={`${record.id}-${record.digest}`}>
        <div>
          <strong>{record.name} {record.version}</strong>
          <small>{record.id} · {record.signatureStatus} · manifest {record.digest.replace("sha256:", "").slice(0, 12)}…</small>
          <small>请求：{record.requestedPermissions.join(", ") || "无"} · 未开放：{record.deniedPermissions.join(", ") || "无"}</small>
          {record.networkDomains.length>0&&<small>网络域名：{record.networkDomains.join(", ")}</small>}
          <small>{record.executable ? "签名可执行" : "阻断执行"} · {record.enabled ? "已启用" : "已禁用"}</small>
        </div>
        <div>
          {record.executable && !record.enabled && <button className="mini-button" onClick={() => void enable(record)} disabled={busy}><ShieldCheck size={13}/>启用</button>}
          {record.enabled && <button className="mini-button" onClick={() => void run(record)} disabled={busy}><Play size={13}/>运行</button>}
          {record.enabled && <button className="mini-button" onClick={() => void disable(record.id)}><Ban size={13}/>禁用</button>}
          <button className="icon-button" aria-label={`移除 ${record.name}`} onClick={() => void remove(record.id)}><Trash2 size={14}/></button>
        </div>
      </div>)}
      {lastRun && <div aria-live="polite"><small>最近运行：{lastRun.pluginId} · 状态码 {lastRun.statusCode} · 燃料 {lastRun.fuelConsumed} · {lastRun.durationMs} ms</small>{lastRun.logs.map((message,index)=><code key={`${index}-${message}`}>{message}</code>)}{lastRun.credentialProxyRequest&&<div><small>{lastRun.credentialProxyRequest.pluginName} 请求一次性 connectionTest，{new Date(lastRun.credentialProxyRequest.expiresAt).toLocaleTimeString()} 前有效。</small><button className="mini-button" onClick={()=>void approveProxy()} disabled={busy}><ShieldCheck size={13}/>允许一次</button><button className="mini-button" onClick={()=>void rejectProxy()} disabled={busy}><Ban size={13}/>拒绝</button></div>}{lastRun.terminalInputRequest&&<div><small>{lastRun.terminalInputRequest.pluginName} 请求向当前会话发送以下内容，{new Date(lastRun.terminalInputRequest.expiresAt).toLocaleTimeString()} 前有效：</small><code>{JSON.stringify(lastRun.terminalInputRequest.data)}</code><button className="mini-button" onClick={()=>void approveTerminalInput()} disabled={busy}><ShieldCheck size={13}/>确认发送一次</button><button className="mini-button" onClick={()=>void rejectTerminalInput()} disabled={busy}><Ban size={13}/>拒绝</button></div>}{proxyStatus&&<small>{proxyStatus}</small>}</div>}
    </div>
    <div className="plugin-report">
      <strong>最近审计</strong>
      <button className="mini-button" onClick={() => void exportAudit()} disabled={!audit.length}><Download size={13}/>导出</button>
      {audit.length === 0 ? <p>暂无审计事件。</p> : audit.map((event) => <small key={event.id}>{new Date(event.createdAt).toLocaleString()} · {event.pluginId} · {event.action} · {event.detail}</small>)}
    </div>
  </section>;
}
