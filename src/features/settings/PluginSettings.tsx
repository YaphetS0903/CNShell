import { open } from "@tauri-apps/plugin-dialog";
import { FileSearch, ShieldAlert } from "lucide-react";
import { useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { PluginPermissionReport } from "../../types";

export function PluginSettings({ onError }: { onError: (message: string) => void }) {
  const [report, setReport] = useState<PluginPermissionReport | null>(null);
  const inspect = async () => {
    try {
      const path = await open({ multiple: false, directory: false, filters: [{ name: "Plugin manifest", extensions: ["json"] }] });
      if (path) setReport(await api.inspectPluginManifest(path));
    } catch (error) { onError(errorMessage(error)); }
  };
  return <section className="plugin-settings" aria-label="插件权限">
    <div className="section-heading"><div><h3><ShieldAlert size={16} /> 插件权限</h3></div></div>
    <button className="button secondary" onClick={() => void inspect()}><FileSearch size={14} /> 检查 manifest</button>
    {report && <div className="plugin-report" aria-live="polite"><strong>{report.manifest.name} {report.manifest.version}</strong><small>{report.manifest.id} · 签名：{report.signatureStatus}</small><div><span>默认授予：{report.defaultGrantedPermissions.join(", ") || "无"}</span><span>默认拒绝：{report.deniedPermissions.join(", ") || "无"}</span></div>{report.warnings.map((warning) => <p key={warning}>{warning}</p>)}</div>}
  </section>;
}
