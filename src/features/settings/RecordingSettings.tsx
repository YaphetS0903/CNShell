import { CircleStop, Play, Radio, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { compileRecordedActions, listenRecordableAction, type RecordableAutomationAction } from "../../lib/automation-recorder";
import { errorMessage } from "../../lib/format";
import type { AutomationPlan, BackgroundTask, ConnectionProfile } from "../../types";

export function RecordingSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const [name, setName] = useState("");
  const [connectionId, setConnectionId] = useState("");
  const [recording, setRecording] = useState(false);
  const [actions, setActions] = useState<RecordableAutomationAction[]>([]);
  const [plan, setPlan] = useState<AutomationPlan | null>(null);
  const [task, setTask] = useState<BackgroundTask | null>(null);

  useEffect(() => {
    if (!recording) return;
    return listenRecordableAction((action) => {
      if (action.connectionId !== connectionId) return;
      setActions((current) => current.length >= 50 ? current : [...current, action]);
    });
  }, [recording, connectionId]);

  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status)) return;
    const timer = window.setInterval(() => {
      void api.getTask(task.id).then(setTask).catch((error) => onError(errorMessage(error)));
    }, 400);
    return () => window.clearInterval(timer);
  }, [task, onError]);

  const begin = () => {
    if (!connectionId) {
      onError("请选择要录制的 SSH 连接");
      return;
    }
    setActions([]);
    setPlan(null);
    setRecording(true);
  };

  const stop = () => {
    setRecording(false);
    setPlan(compileRecordedActions(name, connectionId, actions));
  };

  const run = async () => {
    const next = plan ?? compileRecordedActions(name, connectionId, actions);
    if (!next) {
      onError("录制内容为空，或计划名称未填写");
      return;
    }
    try {
      await api.validateAutomation(next);
      if (!confirm(`将按录制顺序在目标连接执行 ${next.steps.length} 个命令，确认继续？`)) return;
      setTask(await api.startAutomation(next));
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  return (
    <section className="automation-recording" aria-label="操作录制">
      <div className="section-heading">
        <div><h3><Radio size={16} /> 操作录制</h3></div>
      </div>
      <div className="automation-meta">
        <label><span>录制名称</span><input value={name} onChange={(event) => { setName(event.target.value); setPlan(null); }} /></label>
        <label><span>录制连接</span><select value={connectionId} onChange={(event) => { setConnectionId(event.target.value); setPlan(null); }}><option value="">选择 SSH 连接</option>{connections.filter((item) => item.protocol === "ssh").map((item) => <option key={item.id} value={item.id}>{item.name}</option>)}</select></label>
      </div>
      <div className="automation-actions">
        {recording ? <button className="button secondary danger" onClick={stop}><CircleStop size={14} /> 停止录制</button> : <button className="button secondary" onClick={begin}><Radio size={14} /> 开始录制</button>}
        {plan && <button className="button primary" onClick={() => void run()}><Play size={14} /> 预览并运行</button>}
        <IconButton icon={Trash2} label="清空录制" disabled={recording || !actions.length} onClick={() => { setActions([]); setPlan(null); }} />
      </div>
      <div className="recording-status" aria-live="polite">{recording ? "录制中" : `${actions.length} 个结构化动作`}{task ? ` · 任务 ${task.status}` : ""}</div>
      {actions.length > 0 && <ol className="recording-actions">{actions.map((action, index) => <li key={`${action.recordedAt}-${index}`}><code>{action.command}</code></li>)}</ol>}
    </section>
  );
}
