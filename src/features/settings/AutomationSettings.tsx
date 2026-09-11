import { open } from "@tauri-apps/plugin-dialog";
import {
  Braces,
  Clock3,
  History,
  Pencil,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type {
  AutomationPlan,
  AutomationRun,
  AutomationRunRecord,
  AutomationSchedule,
  AutomationStep,
  BackgroundTask,
  ConnectionProfile,
} from "../../types";
import { PythonAutomationSettings } from "./PythonAutomationSettings";
import { RecordingSettings } from "./RecordingSettings";
import {
  formatAutomationSchedule,
  formatScheduleTime,
} from "./automation-format";

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

const scheduleDefaults: Record<string, string> = {
  interval: "3600",
  daily: "09:00",
  weekly: "mon@09:00",
  cron: "0 0 9 * * *",
};

const scheduleDefault = (type: string) =>
  type === "once"
    ? new Date(Date.now() + 60 * 60 * 1000).toISOString()
    : (scheduleDefaults[type] ?? "");

const defaultTimeZone =
  Intl.DateTimeFormat().resolvedOptions().timeZone || "UTC";

const runStatusLabel = (status: string) =>
  ({ completed: "成功", failed: "失败", cancelled: "已取消" })[status] ??
  status;

const runSourceLabel = (source: string) =>
  ({
    manual: "手动运行",
    scheduled: "定时触发",
    scheduleManual: "手动触发任务",
    python: "Python",
  })[source] ?? source;

const runDuration = (run: AutomationRunRecord) => {
  const duration =
    new Date(run.finishedAt).getTime() - new Date(run.startedAt).getTime();
  if (!Number.isFinite(duration) || duration < 0) return "—";
  if (duration < 1000) return `${duration} ms`;
  if (duration < 60_000) return `${(duration / 1000).toFixed(1)} 秒`;
  return `${Math.floor(duration / 60_000)} 分 ${Math.round((duration % 60_000) / 1000)} 秒`;
};

export function AutomationSettings({
  connections,
  onError,
  onDirtyChange,
}: {
  connections: ConnectionProfile[];
  onError: (message: string) => void;
  onDirtyChange?: (dirty: boolean) => void;
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
  const [runs, setRuns] = useState<AutomationRunRecord[]>([]);
  const [runStatusFilter, setRunStatusFilter] = useState("all");
  const [editingScheduleId, setEditingScheduleId] = useState<string | null>(
    null,
  );
  const [scheduleType, setScheduleType] = useState("interval");
  const [scheduleExpression, setScheduleExpression] = useState("3600");
  const [misfirePolicy, setMisfirePolicy] = useState("skip");
  const [timeZone, setTimeZone] = useState(defaultTimeZone);
  const [activeTool, setActiveTool] = useState<
    "workflow" | "history" | "python" | "recording"
  >("workflow");
  const scheduleEditorRef = useRef<HTMLElement>(null);
  const currentDraft = useMemo(
    () =>
      JSON.stringify({
        plan,
        scheduleType,
        scheduleExpression,
        misfirePolicy,
        timeZone,
      }),
    [misfirePolicy, plan, scheduleExpression, scheduleType, timeZone],
  );
  const [savedDraft, setSavedDraft] = useState(currentDraft);
  const dirty = currentDraft !== savedDraft;
  useEffect(() => {
    onDirtyChange?.(dirty);
  }, [dirty, onDirtyChange]);
  useEffect(
    () => () => {
      onDirtyChange?.(false);
    },
    [onDirtyChange],
  );
  const loadRuns = useCallback(() => {
    void api
      .listAutomationRuns()
      .then(setRuns)
      .catch((error) => onError(errorMessage(error)));
  }, [onError]);
  useEffect(() => {
    void api
      .listAutomationSchedules()
      .then(setSchedules)
      .catch((error) => onError(errorMessage(error)));
    loadRuns();
  }, [loadRuns, onError]);
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
              loadRuns();
            } else if (
              next.status === "failed" ||
              next.status === "cancelled"
            ) {
              setRunning(false);
              loadRuns();
            }
          })
          .catch((error) => {
            setRunning(false);
            onError(errorMessage(error));
          }),
      300,
    );
    return () => window.clearInterval(timer);
  }, [loadRuns, task, onError]);
  const visibleRuns = useMemo(
    () =>
      runStatusFilter === "all"
        ? runs
        : runs.filter((item) => item.status === runStatusFilter),
    [runStatusFilter, runs],
  );
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
    window.setTimeout(loadRuns, 500);
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
        id: editingScheduleId ?? crypto.randomUUID(),
        plan,
        scheduleType,
        expression: scheduleExpression.trim(),
        enabled: true,
        misfirePolicy,
        timeZone,
        nextRunAt: null,
        lastRunAt: null,
        lastOccurrenceKey: null,
      });
      setSchedules((current) =>
        editingScheduleId
          ? current.map((item) =>
              item.id === editingScheduleId ? schedule : item,
            )
          : [...current, schedule],
      );
      setEditingScheduleId(null);
      setSavedDraft(currentDraft);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const runSchedule = async (schedule: AutomationSchedule) => {
    try {
      const next = await api.runAutomationScheduleNow(schedule.id);
      setTask(next);
      setResult(null);
      setRunning(true);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const editSchedule = (schedule: AutomationSchedule) => {
    const nextDraft = JSON.stringify({
      plan: schedule.plan,
      scheduleType: schedule.scheduleType,
      scheduleExpression: schedule.expression,
      misfirePolicy: schedule.misfirePolicy,
      timeZone: schedule.timeZone,
    });
    setPlan(schedule.plan);
    setScheduleType(schedule.scheduleType);
    setScheduleExpression(schedule.expression);
    setMisfirePolicy(schedule.misfirePolicy);
    setTimeZone(schedule.timeZone);
    setEditingScheduleId(schedule.id);
    setSavedDraft(nextDraft);
    scheduleEditorRef.current?.scrollIntoView({ block: "start" });
  };
  const deleteSchedule = async (id: string) => {
    const schedule = schedules.find((item) => item.id === id);
    if (
      !confirm(
        `删除定时任务“${schedule?.plan.name || "未命名计划"}”？此操作无法撤销。`,
      )
    )
      return;
    try {
      await api.deleteAutomationSchedule(id);
      setSchedules((current) => current.filter((item) => item.id !== id));
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const clearRuns = async () => {
    if (!confirm("清空全部自动化运行记录？此操作无法撤销。")) return;
    try {
      await api.clearAutomationRuns();
      setRuns([]);
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  return (
    <section className="automation-settings" aria-busy={running}>
      <div
        className="automation-tool-tabs"
        role="tablist"
        aria-label="自动化工具"
        onKeyDown={(event) => {
          if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key))
            return;
          event.preventDefault();
          const tabs = Array.from(
            event.currentTarget.querySelectorAll<HTMLButtonElement>(
              '[role="tab"]',
            ),
          );
          const current = Math.max(
            0,
            tabs.indexOf(document.activeElement as HTMLButtonElement),
          );
          const next =
            event.key === "Home"
              ? 0
              : event.key === "End"
                ? tabs.length - 1
                : (current +
                    (event.key === "ArrowRight" ? 1 : -1) +
                    tabs.length) %
                  tabs.length;
          tabs[next]?.focus();
          tabs[next]?.click();
        }}
      >
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "workflow"}
          className={activeTool === "workflow" ? "active" : ""}
          data-modal-initial-focus
          onClick={() => setActiveTool("workflow")}
        >
          任务编排
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "history"}
          className={activeTool === "history" ? "active" : ""}
          onClick={() => {
            setActiveTool("history");
            loadRuns();
          }}
        >
          运行记录
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "python"}
          className={activeTool === "python" ? "active" : ""}
          onClick={() => setActiveTool("python")}
        >
          Python
        </button>
        <button
          type="button"
          role="tab"
          aria-selected={activeTool === "recording"}
          className={activeTool === "recording" ? "active" : ""}
          onClick={() => setActiveTool("recording")}
        >
          录制
        </button>
      </div>
      {activeTool === "workflow" && (
        <>
          <div className="section-heading">
            <div>
              <h3>
                <Braces size={16} />
                受限任务编排
              </h3>
              <p>只支持命令、匹配、条件和文件传输；不执行未授权系统代码。</p>
            </div>
          </div>
          <div className="automation-meta">
            <label>
              <span>计划名称</span>
              <input
                value={plan.name}
                onChange={(event) =>
                  setPlan({ ...plan, name: event.target.value })
                }
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
                      <option value="continueIfMatch">
                        匹配才继续，否则失败
                      </option>
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
                      update(index, {
                        timeoutSeconds: Number(event.target.value),
                      })
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
          <section
            className="automation-schedules"
            aria-label="定时任务"
            ref={scheduleEditorRef}
          >
            <div className="section-heading">
              <div>
                <h3>
                  <Clock3 size={16} /> 定时任务
                </h3>
                <p>只调度当前受限步骤；应用退出期间不会在后台执行。</p>
              </div>
            </div>
            <div className="automation-meta">
              <label>
                <span>类型</span>
                <select
                  value={scheduleType}
                  onChange={(event) => {
                    const value = event.target.value;
                    setScheduleType(value);
                    setScheduleExpression(scheduleDefault(value));
                  }}
                >
                  <option value="interval">固定间隔</option>
                  <option value="once">一次执行</option>
                  <option value="daily">每日</option>
                  <option value="weekly">每周</option>
                  <option value="cron">Cron</option>
                </select>
              </label>
              <label>
                <span>
                  {scheduleType === "interval"
                    ? "间隔秒数"
                    : scheduleType === "once"
                      ? "RFC3339 时间"
                      : scheduleType === "daily"
                        ? "每日时间"
                        : scheduleType === "weekly"
                          ? "星期与时间"
                          : "Cron 表达式"}
                </span>
                <input
                  value={scheduleExpression}
                  onChange={(event) =>
                    setScheduleExpression(event.target.value)
                  }
                  placeholder={scheduleDefault(scheduleType)}
                />
              </label>
              <label>
                <span>IANA 时区</span>
                <input
                  value={timeZone}
                  onChange={(event) => setTimeZone(event.target.value)}
                  placeholder="Asia/Shanghai"
                />
              </label>
              <label>
                <span>错过执行</span>
                <select
                  value={misfirePolicy}
                  onChange={(event) => setMisfirePolicy(event.target.value)}
                >
                  <option value="skip">跳过</option>
                  <option value="runOnce">恢复后执行一次</option>
                </select>
              </label>
              <button
                className="button secondary"
                onClick={() => void saveSchedule()}
              >
                <Clock3 size={14} />
                {editingScheduleId ? "更新定时任务" : "保存定时任务"}
              </button>
            </div>
            <div className="automation-schedule-list-heading">
              <div>
                <strong>已保存任务</strong>
                <small>{schedules.length} 个</small>
              </div>
              <small>运行状态会显示在任务列表下方。</small>
            </div>
            {schedules.length > 0 ? (
              <div className="automation-schedule-list" aria-live="polite">
                {schedules.map((schedule) => (
                  <article
                    key={schedule.id}
                    className="automation-schedule-card"
                  >
                    <div className="automation-schedule-copy">
                      <div>
                        <strong>{schedule.plan.name || "未命名计划"}</strong>
                        <span
                          className={`automation-schedule-status ${schedule.enabled ? "enabled" : "paused"}`}
                        >
                          {schedule.enabled ? "已启用" : "已暂停"}
                        </span>
                      </div>
                      <p>{formatAutomationSchedule(schedule)}</p>
                      <small>
                        {connections.find(
                          (item) => item.id === schedule.plan.connectionId,
                        )?.name ?? "目标连接不可用"}
                        {` · ${schedule.plan.steps.length} 个步骤 · ${schedule.timeZone}`}
                      </small>
                      <dl>
                        <div>
                          <dt>下次运行</dt>
                          <dd>
                            {formatScheduleTime(
                              schedule.nextRunAt,
                              schedule.timeZone,
                            ) ?? "暂无"}
                          </dd>
                        </div>
                        <div>
                          <dt>最近运行</dt>
                          <dd>
                            {formatScheduleTime(
                              schedule.lastRunAt,
                              schedule.timeZone,
                            ) ?? "尚未运行"}
                          </dd>
                        </div>
                      </dl>
                    </div>
                    <div className="automation-actions">
                      <button
                        className="button secondary"
                        disabled={running}
                        onClick={() => void runSchedule(schedule)}
                      >
                        <Play size={14} /> 立即运行
                      </button>
                      <IconButton
                        icon={Pencil}
                        label="编辑定时任务"
                        disabled={running}
                        onClick={() => editSchedule(schedule)}
                      />
                      <IconButton
                        icon={Trash2}
                        label="删除定时任务"
                        disabled={running}
                        onClick={() => void deleteSchedule(schedule.id)}
                      />
                    </div>
                  </article>
                ))}
              </div>
            ) : (
              <div className="automation-schedule-empty" role="status">
                <Clock3 size={18} />
                <span>
                  <strong>还没有定时任务</strong>
                  <small>完成上方计划后保存，任务会集中显示在这里。</small>
                </span>
              </div>
            )}
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
        </>
      )}
      {activeTool === "history" && (
        <section className="automation-run-history" aria-label="自动化运行记录">
          <div className="section-heading">
            <div>
              <h3>
                <History size={16} /> 运行记录
              </h3>
              <p>保留最近 100 次运行，可展开查看每一步的结果和错误。</p>
            </div>
            <div className="automation-history-actions">
              <IconButton
                icon={RefreshCw}
                label="刷新运行记录"
                onClick={loadRuns}
              />
              <IconButton
                icon={Trash2}
                label="清空运行记录"
                disabled={!runs.length || running}
                onClick={() => void clearRuns()}
              />
            </div>
          </div>
          <div className="automation-history-filter">
            <label>
              <span>状态</span>
              <select
                value={runStatusFilter}
                onChange={(event) => setRunStatusFilter(event.target.value)}
              >
                <option value="all">全部（{runs.length}）</option>
                <option value="completed">
                  成功（
                  {runs.filter((item) => item.status === "completed").length}）
                </option>
                <option value="failed">
                  失败（{runs.filter((item) => item.status === "failed").length}
                  ）
                </option>
                <option value="cancelled">
                  已取消（
                  {runs.filter((item) => item.status === "cancelled").length}）
                </option>
              </select>
            </label>
            <span>{visibleRuns.length} 条记录</span>
          </div>
          {visibleRuns.length ? (
            <div className="automation-run-list" aria-live="polite">
              {visibleRuns.map((run) => (
                <article
                  key={run.id}
                  className={`automation-run-card ${run.status}`}
                >
                  <header>
                    <div>
                      <strong>{run.planName || "未命名计划"}</strong>
                      <span className={`automation-run-status ${run.status}`}>
                        {runStatusLabel(run.status)}
                      </span>
                    </div>
                    <time dateTime={run.startedAt}>
                      {formatScheduleTime(run.startedAt, defaultTimeZone)}
                    </time>
                  </header>
                  <p>
                    {connections.find((item) => item.id === run.connectionId)
                      ?.name ?? "目标连接已删除"}
                    {` · ${runSourceLabel(run.source)} · ${runDuration(run)}`}
                  </p>
                  <details>
                    <summary>
                      {run.results.length
                        ? `查看 ${run.results.length} 个步骤`
                        : "查看错误详情"}
                    </summary>
                    {run.error && (
                      <p className="automation-run-error">{run.error}</p>
                    )}
                    {run.results.map((item, index) => (
                      <section
                        key={`${item.stepId}-${index}`}
                        className={item.status}
                      >
                        <strong>
                          步骤 {index + 1} · {item.kind} ·{" "}
                          {runStatusLabel(item.status)} · {item.durationMs} ms
                        </strong>
                        {item.output && <pre>{item.output}</pre>}
                        {item.error && <p>{item.error}</p>}
                      </section>
                    ))}
                  </details>
                </article>
              ))}
            </div>
          ) : (
            <div className="automation-schedule-empty" role="status">
              <History size={18} />
              <span>
                <strong>
                  {runs.length ? "当前筛选没有记录" : "还没有运行记录"}
                </strong>
                <small>
                  {runs.length
                    ? "切换状态筛选查看其他结果。"
                    : "运行任务后，结果会自动保存在这里。"}
                </small>
              </span>
            </div>
          )}
        </section>
      )}
      {activeTool === "python" && (
        <PythonAutomationSettings connections={connections} onError={onError} />
      )}
      {activeTool === "recording" && (
        <RecordingSettings connections={connections} onError={onError} />
      )}
    </section>
  );
}
