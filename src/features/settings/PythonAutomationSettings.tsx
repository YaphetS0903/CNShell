import { open } from "@tauri-apps/plugin-dialog";
import { Eye, FilePlus2, Play, ShieldCheck, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type {
  BackgroundTask,
  ConnectionProfile,
  PythonAutomationPreview,
  PythonAutomationRequest,
} from "../../types";

const permissionLabels: Record<string, string> = {
  executeCommand: "执行远端命令",
  readResults: "读取步骤结果",
  transferUpload: "上传授权文件",
  transferDownload: "下载到授权路径",
};

export function PythonAutomationSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [connectionId, setConnectionId] = useState("");
  const [source, setSource] = useState(
    'cnshell.command("uname -a", timeout=30)\ncnshell.require("Linux")',
  );
  const [permissions, setPermissions] = useState([
    "executeCommand",
    "readResults",
  ]);
  const [allowedLocalPaths, setAllowedLocalPaths] = useState<string[]>([]);
  const [preview, setPreview] = useState<PythonAutomationPreview | null>(null);
  const [task, setTask] = useState<BackgroundTask | null>(null);

  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status)) return;
    const timer = window.setInterval(() => {
      void api.getTask(task.id).then(setTask).catch((error) => onError(errorMessage(error)));
    }, 400);
    return () => window.clearInterval(timer);
  }, [task, onError]);

  const request = useMemo<PythonAutomationRequest>(
    () => ({
      id: crypto.randomUUID(),
      name,
      source,
      manifest: { connectionId, permissions, allowedLocalPaths },
    }),
    [name, source, connectionId, permissions, allowedLocalPaths],
  );

  const togglePermission = (permission: string, checked: boolean) => {
    setPermissions((current) =>
      checked
        ? [...new Set([...current, permission])]
        : current.filter((item) => item !== permission),
    );
    setPreview(null);
  };

  const authorizePath = async () => {
    const selected = await open({ multiple: true, directory: false });
    if (!selected) return;
    const paths = Array.isArray(selected) ? selected : [selected];
    setAllowedLocalPaths((current) => [...new Set([...current, ...paths])]);
    setPreview(null);
  };

  const inspect = async () => {
    try {
      const value = await api.previewPythonAutomation(request);
      setPreview(value);
      return value;
    } catch (error) {
      onError(errorMessage(error));
      return null;
    }
  };

  const run = async () => {
    const inspected = preview ?? (await inspect());
    if (!inspected) return;
    const target = connections.find((item) => item.id === connectionId)?.name ?? connectionId;
    const warning = inspected.warnings.length
      ? `\n\n警告：\n${inspected.warnings.join("\n")}`
      : "";
    if (
      !confirm(
        `目标：${target}\n脚本：${inspected.scriptHash}\n权限：${inspected.permissions.join(", ") || "无"}\n步骤：${inspected.steps.length}${warning}\n\n确认运行？`,
      )
    )
      return;
    try {
      setTask(await api.startPythonAutomation(request));
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  const cancel = async () => {
    if (!task) return;
    await api.cancelTask(task.id);
    setTask({ ...task, status: "cancelled" });
  };

  return (
    <section className="python-automation" aria-label="受限 Python 自动化">
      <div className="section-heading">
        <div>
          <h3><ShieldCheck size={16} /> 受限 Python</h3>
        </div>
      </div>
      <div className="automation-meta">
        <label>
          <span>脚本名称</span>
          <input value={name} onChange={(event) => { setName(event.target.value); setPreview(null); }} />
        </label>
        <label>
          <span>Python 目标连接</span>
          <select value={connectionId} onChange={(event) => { setConnectionId(event.target.value); setPreview(null); }}>
            <option value="">选择 SSH 连接</option>
            {connections.filter((item) => item.protocol === "ssh").map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}
          </select>
        </label>
      </div>
      <label className="python-source-field">
        <span>脚本</span>
        <textarea aria-label="受限 Python 脚本" spellCheck={false} value={source} onChange={(event) => { setSource(event.target.value); setPreview(null); }} />
      </label>
      <fieldset className="python-permissions">
        <legend>权限清单</legend>
        {Object.entries(permissionLabels).map(([permission, label]) => (
          <label className="check-row" key={permission}>
            <input type="checkbox" checked={permissions.includes(permission)} onChange={(event) => togglePermission(permission, event.target.checked)} />
            <span>{label}</span>
          </label>
        ))}
      </fieldset>
      <div className="python-paths">
        <button className="button secondary" onClick={() => void authorizePath()}><FilePlus2 size={14} /> 授权本地文件</button>
        {allowedLocalPaths.map((path) => <span key={path} className="path-chip" title={path}>{path}</span>)}
      </div>
      <div className="automation-actions">
        <button className="button secondary" onClick={() => void inspect()}><Eye size={14} /> 生成预览</button>
        {task && !["completed", "failed", "cancelled"].includes(task.status) ? (
          <button className="button secondary danger" onClick={() => void cancel()}><X size={14} /> 取消运行</button>
        ) : (
          <button className="button primary" onClick={() => void run()}><Play size={14} /> 运行 Python</button>
        )}
      </div>
      {preview && (
        <div className="python-preview" aria-live="polite">
          <strong>{preview.scriptHash}</strong>
          <span>{preview.steps.length} 个受限步骤</span>
          {preview.warnings.map((warning) => <p key={warning}>{warning}</p>)}
        </div>
      )}
      {task && <p className="muted-copy" aria-live="polite">任务状态：{task.status}{task.error ? ` · ${task.error}` : ""}</p>}
    </section>
  );
}
