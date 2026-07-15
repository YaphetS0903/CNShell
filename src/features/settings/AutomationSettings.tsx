import { open } from "@tauri-apps/plugin-dialog";
import { Braces, Clock3, Play, Plus, Trash2, X } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type {
  AutomationPlan,
  AutomationRun,
  AutomationSchedule,
  AutomationStep,
  BackgroundTask,
  ConnectionProfile,
} from "../../types";

const blankStep = (
  kind: AutomationStep["kind"] = "command",
): AutomationStep => ({
  id: crypto.randomUUID(),
  kind,
  command: kind === "command" ? "" : null,
  pattern: ["waitForMatch", "condition"].includes(kind) ? "" : null,
  timeoutSeconds: 30,
  action: kind === "condition" ? "continueIfMatch" : null,
  direction: kind === "transfer" ? "upload" : null,
  localPath: kind === "transfer" ? "" : null,
  remotePath: kind === "transfer" ? "" : null,
});

export function AutomationSettings({
  connections,
  onError,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
}) {
  const [plan, setPlan] = useState<AutomationPlan>({
    id: crypto.randomUUID(),
    name: "",
    connectionId: "",
    steps: [blankStep()],
  });
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [result, setResult] = useState<AutomationRun | null>(null);
  const [running, setRunning] = useState(false);
  const [schedules, setSchedules] = useState<AutomationSchedule[]>([]);
  const [scheduleType, setScheduleType] = useState("interval");
  const [scheduleExpression, setScheduleExpression] = useState("3600");
  const [misfirePolicy, setMisfirePolicy] = useState("skip");
  useEffect(() => {
    void api.listAutomationSchedules().then(setSchedules).catch((error) => onError(errorMessage(error)));
  }, [onError]);
  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status))
      return;
    const timer = window.setInterval(
      () =>
        void api
          .getTask(task.id)
          .then((next) => {
            setTask(next);
            if (next.status === "completed") {
              setRunning(false);
              setResult(next.result as AutomationRun);
            } else if (next.status === "failed" || next.status === "cancelled")
              setRunning(false);
          })
          .catch((error) => {
            setRunning(false);
            onError(errorMessage(error));
          }),
      300,
    );
    return () => window.clearInterval(timer);
  }, [task, onError]);
  const preview = useMemo(
    () =>
      plan.steps
        .map(
          (step, index) =>
            `${index + 1}. ${step.kind === "command" ? `执行 ${step.command || "<空命令>"}` : step.kind === "waitForMatch" ? `等待此前输出匹配 /${step.pattern}/` : step.kind === "condition" ? `条件 /${step.pattern}/ → ${step.action}` : `${step.direction} ${step.localPath} ⇄ ${step.remotePath}`}（${step.timeoutSeconds ?? 30}s）`,
        )
        .join("\n"),
    [plan.steps],
  );
  const update = (index: number, patch: Partial<AutomationStep>) =>
    setPlan((current) => ({
      ...current,
      steps: current.steps.map((step, itemIndex) =>
        itemIndex === index ? { ...step, ...patch } : step,
      ),
    }));
  const chooseLocal = async (index: number) => {
    const path = await open({ multiple: false, directory: false });
    if (path) update(index, { localPath: path });
  };
  const start = async () => {
    try {
      await api.validateAutomation(plan);
      if (
        !confirm(
          `即将在 ${connections.find((item) => item.id === plan.connectionId)?.name ?? "目标主机"} 执行：\n\n${preview}\n\n失败时立即停止，确认继续？`,
        )
      )
        return;
      const next = await api.startAutomation(plan);
      setTask(next);
      setResult(null);
      setRunning(true);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const cancel = async () => {
    if (!task) return;
    await api.cancelTask(task.id);
    setRunning(false);
    setTask({ ...task, status: "cancelled" });
  };
  const retry = async () => {
    setTask(null);
    setResult(null);
    await start();
  };
  const saveSchedule = async () => {
    try {
      await api.validateAutomation(plan);
      const schedule = await api.saveAutomationSchedule({
        id: crypto.randomUUID(),
        plan,
        scheduleType,
        expression: scheduleExpression.trim(),
        enabled: true,
        misfirePolicy,
        nextRunAt: null,
        lastRunAt: null,
      });
      setSchedules((current) => [...current, schedule]);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const runSchedule = async (schedule: AutomationSchedule) => {
    try {
      await api.runAutomationScheduleNow(schedule.id);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const deleteSchedule = async (id: string) => {
    try {
      await api.deleteAutomationSchedule(id);
      setSchedules((current) => current.filter((item) => item.id !== id));
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  return (
    <section className="automation-settings" aria-busy={running}>
      <div className="section-heading">
        <div>
          <h3>
            <Braces size={16} />
            受限任务编排
          </h3>
          <p>
            只支持命令、匹配、条件和文件传输；不执行任意 Python 或插件代码。
          </p>
        </div>
      </div>
      <div className="automation-meta">
        <label>
          <span>计划名称</span>
          <input
            value={plan.name}
            onChange={(event) => setPlan({ ...plan, name: event.target.value })}
            placeholder="例如：发布前检查"
          />
        </label>
        <label>
          <span>目标连接</span>
          <select
            value={plan.connectionId}
            onChange={(event) =>
              setPlan({ ...plan, connectionId: event.target.value })
            }
          >
            <option value="">选择 SSH 连接</option>
            {connections
              .filter((item) => item.protocol === "ssh")
              .map((item) => (
                <option key={item.id} value={item.id}>
                  {item.name}
                </option>
              ))}
          </select>
        </label>
      </div>
      <div className="automation-steps">
        {plan.steps.map((step, index) => (
          <article key={step.id}>
            <header>
              <b>步骤 {index + 1}</b>
              <select
                value={step.kind}
                onChange={(event) =>
                  update(index, {
                    ...blankStep(event.target.value),
                    id: step.id,
                  })
                }
              >
                <option value="command">执行命令</option>
                <option value="waitForMatch">等待匹配</option>
                <option value="condition">条件分支</option>
                <option value="transfer">文件传输</option>
              </select>
              <IconButton
                icon={Trash2}
                label={`删除步骤 ${index + 1}`}
                disabled={plan.steps.length === 1}
                onClick={() =>
                  setPlan({
                    ...plan,
                    steps: plan.steps.filter(
                      (_, itemIndex) => itemIndex !== index,
                    ),
                  })
                }
              />
            </header>
            {step.kind === "command" && (
              <label>
                <span>命令</span>
                <input
                  value={step.command ?? ""}
                  onChange={(event) =>
                    update(index, { command: event.target.value })
                  }
                />
              </label>
            )}
            {["waitForMatch", "condition"].includes(step.kind) && (
              <label>
                <span>正则表达式</span>
                <input
                  value={step.pattern ?? ""}
                  onChange={(event) =>
                    update(index, { pattern: event.target.value })
                  }
                />
              </label>
            )}
            {step.kind === "condition" && (
              <label>
                <span>动作</span>
                <select
                  aria-label={`步骤 ${index + 1} 类型`}
                  value={step.action ?? "continueIfMatch"}
                  onChange={(event) =>
                    update(index, { action: event.target.value })
                  }
                >
                  <option value="continueIfMatch">匹配才继续，否则失败</option>
                  <option value="stopIfMatch">匹配时正常结束</option>
                  <option value="stopIfMissing">未匹配时正常结束</option>
                </select>
              </label>
            )}
            {step.kind === "transfer" && (
              <>
                <label>
                  <span>方向</span>
                  <select
                    value={step.direction ?? "upload"}
                    onChange={(event) =>
                      update(index, { direction: event.target.value })
                    }
                  >
                    <option value="upload">上传</option>
                    <option value="download">下载</option>
                  </select>
                </label>
                <label>
                  <span>本地文件</span>
                  <div className="path-picker">
                    <input value={step.localPath ?? ""} readOnly />
                    <button
                      className="mini-button"
                      onClick={() => void chooseLocal(index)}
                    >
                      选择
                    </button>
                  </div>
                </label>
                <label>
                  <span>远端绝对路径</span>
                  <input
                    value={step.remotePath ?? ""}
                    onChange={(event) =>
                      update(index, { remotePath: event.target.value })
                    }
                  />
                </label>
              </>
            )}
            <label>
              <span>超时（秒）</span>
              <input
                type="number"
                min={1}
                max={3600}
                value={step.timeoutSeconds ?? 30}
                onChange={(event) =>
                  update(index, { timeoutSeconds: Number(event.target.value) })
                }
              />
            </label>
          </article>
        ))}
      </div>
      <div className="automation-actions">
        <button
          className="button secondary"
          disabled={plan.steps.length >= 50 || running}
          onClick={() =>
            setPlan({ ...plan, steps: [...plan.steps, blankStep()] })
          }
        >
          <Plus size={14} />
          添加步骤
        </button>
        {running ? (
          <button
            className="button secondary danger"
            onClick={() => void cancel()}
          >
            <X size={14} />
            取消运行
          </button>
        ) : (
          <button className="button primary" onClick={() => void start()}>
            <Play size={14} />
            预览并运行
          </button>
        )}
      </div>
      <pre className="automation-preview" aria-label="自动化预览">
        {preview}
      </pre>
      <section className="automation-schedules" aria-label="定时任务">
        <div className="section-heading">
          <div>
            <h3><Clock3 size={16} /> 定时任务</h3>
            <p>只调度当前受限步骤；应用退出期间不会在后台执行。</p>
          </div>
        </div>
        <div className="automation-meta">
          <label><span>类型</span><select value={scheduleType} onChange={(event) => setScheduleType(event.target.value)}><option value="interval">固定间隔</option><option value="once">一次执行</option><option value="cron">Cron</option></select></label>
          <label><span>{scheduleType === "interval" ? "间隔秒数" : scheduleType === "once" ? "RFC3339 时间" : "Cron 表达式"}</span><input value={scheduleExpression} onChange={(event) => setScheduleExpression(event.target.value)} placeholder={scheduleType === "cron" ? "0 0 * * * *" : undefined} /></label>
          <label><span>错过执行</span><select value={misfirePolicy} onChange={(event) => setMisfirePolicy(event.target.value)}><option value="skip">跳过</option><option value="runOnce">恢复后执行一次</option></select></label>
          <button className="button secondary" onClick={() => void saveSchedule()}><Clock3 size={14} /> 保存定时任务</button>
        </div>
        {schedules.length > 0 && <div className="automation-results" aria-live="polite">{schedules.map((schedule) => <article key={schedule.id}><p><strong>{schedule.plan.name || "未命名计划"}</strong> · {schedule.scheduleType} · {schedule.expression}</p><div className="automation-actions"><button className="button secondary" onClick={() => void runSchedule(schedule)}><Play size={14} /> 立即运行</button><IconButton icon={Trash2} label="删除定时任务" onClick={() => void deleteSchedule(schedule.id)} /></div></article>)}</div>}
      </section>
      {task && (
        <p className="muted-copy" aria-live="polite">
          任务状态：{task.status}
          {task.error ? ` · ${task.error}` : ""}
          {!running && ["failed", "cancelled"].includes(task.status) && (
            <button className="mini-button" onClick={() => void retry()}>
              重新运行
            </button>
          )}
        </p>
      )}
      {result && (
        <div className="automation-results" aria-live="polite">
          {result.results.map((item) => (
            <article key={item.stepId} className={item.status}>
              <strong>
                {item.kind} · {item.status} · {item.durationMs} ms
              </strong>
              {item.output && <pre>{item.output}</pre>}
              {item.error && <p>{item.error}</p>}
            </article>
          ))}
        </div>
      )}
    </section>
  );
}
