import { open, save } from "@tauri-apps/plugin-dialog";
import { Ban, Download, FilePlus2, FileSearch, RefreshCw, ShieldAlert, Trash2 } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { PluginAuditEvent, PluginInstallRecord, PluginPermissionReport } from "../../types";

export function PluginSettings({ onError }: { onError: (message: string) => void }) {
  const [report, setReport] = useState<PluginPermissionReport | null>(null);
  const [records, setRecords] = useState<PluginInstallRecord[]>([]);
  const [audit, setAudit] = useState<PluginAuditEvent[]>([]);
  const [busy, setBusy] = useState(false);
  const refresh = useCallback(async () => { try { const [installed, events] = await Promise.all([api.listPlugins(), api.listPluginAudit()]); setRecords(installed); setAudit(events.slice(-8).reverse()); } catch (error) { onError(errorMessage(error)); } }, [onError]);
  useEffect(() => { void refresh(); }, [refresh]);
  const inspect = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Plugin manifest", extensions: ["json"] }] });
      if (path) setReport(await api.inspectPluginManifest(path));
    } catch (error) { onError(errorMessage(error)); }
  };
  const register = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Plugin manifest", extensions: ["json"] }] });
      if (!path) return;
      if (!confirm("该插件会登记为不可执行状态。未签名或未受信任插件不会被加载，是否继续登记？")) return;
      setBusy(true); await api.registerPlugin(path); await refresh();
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };
  const disable = async (id: string) => { try { await api.disablePlugin(id); await refresh(); } catch (error) { onError(errorMessage(error)); } };
  const remove = async (id: string) => { if (!confirm("移除插件登记？不会删除 manifest 文件。")) return; try { await api.removePlugin(id); await refresh(); } catch (error) { onError(errorMessage(error)); } };
  const exportAudit = async () => { try { const path = await save({ defaultPath: "cnshell-plugin-audit.json", filters: [{ name: "JSON", extensions: ["json"] }] }); if (path) await api.exportPluginAudit(path); } catch (error) { onError(errorMessage(error)); } };
  return <section className="plugin-settings" aria-label="插件权限">
    <div className="section-heading"><div><h3><ShieldAlert size={16} /> 插件权限与本地登记</h3><p>当前插件只允许检查和登记，不执行第三方代码。</p></div><button className="mini-button" onClick={() => void refresh()} disabled={busy}><RefreshCw size={13}/>刷新</button></div>
    <div className="backup-actions"><button className="button secondary" onClick={() => void inspect()}><FileSearch size={14} /> 检查 manifest</button><button className="button secondary" onClick={() => void register()} disabled={busy}><FilePlus2 size={14} /> 登记为阻断插件</button></div>
    {report && <div className="plugin-report" aria-live="polite"><strong>{report.manifest.name} {report.manifest.version}</strong><small>{report.manifest.id} · 签名：{report.signatureStatus}</small><div><span>默认授予：{report.defaultGrantedPermissions.join(", ") || "无"}</span><span>默认拒绝：{report.deniedPermissions.join(", ") || "无"}</span></div>{report.warnings.map((warning) => <p key={warning}>{warning}</p>)}</div>}
    <div className="plugin-report"><strong>已登记插件</strong>{records.length===0?<p>暂无插件登记。</p>:records.map((record)=><div className="plugin-record" key={`${record.id}-${record.digest}`}><div><strong>{record.name} {record.version}</strong><small>{record.id} · {record.signatureStatus} · SHA-256 {record.digest.replace("sha256:","").slice(0,16)}…</small><small>请求：{record.requestedPermissions.join(", ")||"无"} · 拒绝：{record.deniedPermissions.join(", ")||"无"}</small><small>{record.executable?"可执行":"阻断执行"} · {record.enabled?"已启用":"已禁用"}</small></div><div><button className="mini-button" onClick={()=>void disable(record.id)}><Ban size={13}/>禁用</button><button className="icon-button" aria-label={`移除 ${record.name}`} onClick={()=>void remove(record.id)}><Trash2 size={14}/></button></div></div>)}</div>
    <div className="plugin-report"><strong>最近审计</strong><button className="mini-button" onClick={()=>void exportAudit()} disabled={!audit.length}><Download size={13}/>导出</button>{audit.length===0?<p>暂无审计事件。</p>:audit.map((event)=><small key={event.id}>{new Date(event.createdAt).toLocaleString()} · {event.pluginId} · {event.action} · {event.detail}</small>)}</div>
  </section>;
}
