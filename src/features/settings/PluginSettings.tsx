import { open, save } from "@tauri-apps/plugin-dialog";
import {
  Ban,
  Download,
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
import { errorMessage } from "../../lib/format";
import type {
  PluginAuditEvent,
  PluginInstallRecord,
  PluginPermissionReport,
  PluginPublisherRoot,
  PluginRunResult,
} from "../../types";

export function PluginSettings({ onError }: { onError: (message: string) => void }) {
  const [report, setReport] = useState<PluginPermissionReport | null>(null);
  const [records, setRecords] = useState<PluginInstallRecord[]>([]);
  const [publishers, setPublishers] = useState<PluginPublisherRoot[]>([]);
  const [audit, setAudit] = useState<PluginAuditEvent[]>([]);
  const [lastRun, setLastRun] = useState<PluginRunResult | null>(null);
  const [busy, setBusy] = useState(false);

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
      if (!confirm("CNshell 会固定 manifest 与 WASM 摘要。只有受信任签名且不请求未开放权限的插件才能启用，是否继续登记？")) return;
      setBusy(true);
      await api.registerPlugin(path);
      await refresh();
    } catch (error) { onError(errorMessage(error)); } finally { setBusy(false); }
  };

  const enable = async (record: PluginInstallRecord) => {
    const permissions = record.requestedPermissions.join(", ") || "无宿主权限";
    if (!confirm(`启用 ${record.name}？\n\n权限：${permissions}\n运行时：无 WASI、32 MB 内存、有限燃料`)) return;
    try { setBusy(true); await api.enablePlugin(record.id); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
  };

  const disable = async (id: string) => {
    try { await api.disablePlugin(id); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
  };

  const run = async (id: string) => {
    try { setBusy(true); setLastRun(await api.runPlugin(id)); await refresh(); }
    catch (error) { onError(errorMessage(error)); }
    finally { setBusy(false); }
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
      <div><h3><ShieldAlert size={16} /> 插件信任与沙箱</h3><p>插件只在签名、摘要和权限通过后进入无 WASI 的限额 WebAssembly 沙箱。</p></div>
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
      {records.length === 0 ? <p>暂无插件登记。</p> : records.map((record) => <div className="plugin-record" key={`${record.id}-${record.digest}`}>
        <div>
          <strong>{record.name} {record.version}</strong>
          <small>{record.id} · {record.signatureStatus} · manifest {record.digest.replace("sha256:", "").slice(0, 12)}…</small>
          <small>请求：{record.requestedPermissions.join(", ") || "无"} · 未开放：{record.deniedPermissions.join(", ") || "无"}</small>
          <small>{record.executable ? "签名可执行" : "阻断执行"} · {record.enabled ? "已启用" : "已禁用"}</small>
        </div>
        <div>
          {record.executable && !record.enabled && <button className="mini-button" onClick={() => void enable(record)} disabled={busy}><ShieldCheck size={13}/>启用</button>}
          {record.enabled && <button className="mini-button" onClick={() => void run(record.id)} disabled={busy}><Play size={13}/>运行</button>}
          {record.enabled && <button className="mini-button" onClick={() => void disable(record.id)}><Ban size={13}/>禁用</button>}
          <button className="icon-button" aria-label={`移除 ${record.name}`} onClick={() => void remove(record.id)}><Trash2 size={14}/></button>
        </div>
      </div>)}
      {lastRun && <small aria-live="polite">最近运行：{lastRun.pluginId} · 状态码 {lastRun.statusCode} · 燃料 {lastRun.fuelConsumed} · {lastRun.durationMs} ms</small>}
    </div>
    <div className="plugin-report">
      <strong>最近审计</strong>
      <button className="mini-button" onClick={() => void exportAudit()} disabled={!audit.length}><Download size={13}/>导出</button>
      {audit.length === 0 ? <p>暂无审计事件。</p> : audit.map((event) => <small key={event.id}>{new Date(event.createdAt).toLocaleString()} · {event.pluginId} · {event.action} · {event.detail}</small>)}
    </div>
  </section>;
}
