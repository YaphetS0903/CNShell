import {
  AlertTriangle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  LoaderCircle,
  Play,
  RefreshCw,
  Search,
  Send,
  Square,
  TerminalSquare,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { useAppStore } from "../../store/app-store";
import type { BatchExecution, ConnectionProfile, Folder } from "../../types";
import { isHighRiskCommand } from "./smart-command";

type Mode = "batch" | "sync";

interface Props {
  connections: ConnectionProfile[];
  connect: (profile: ConnectionProfile) => Promise<void>;
  onClose: () => void;
  onError: (message: string) => void;
}

export function BatchExecutionDialog({
  connections,
  connect,
  onClose,
  onError,
}: Props) {
  const sessions = useAppStore((state) => state.sessions);
  const [folders, setFolders] = useState<Folder[]>([]);
  const [mode, setMode] = useState<Mode>("batch");
  const [selected, setSelected] = useState<Set<string>>(new Set());
  const [query, setQuery] = useState("");
  const [folderFilter, setFolderFilter] = useState("all");
  const [tagFilter, setTagFilter] = useState("all");
  const [command, setCommand] = useState("");
  const [concurrency, setConcurrency] = useState(4);
  const [preview, setPreview] = useState(false);
  const [execution, setExecution] = useState<BatchExecution | null>(null);
  const [expanded, setExpanded] = useState<Set<string>>(new Set());
  const [syncSessions, setSyncSessions] = useState<Map<string, string>>(
    new Map(),
  );
  const [busy, setBusy] = useState(false);

  const sshConnections = useMemo(
    () => connections.filter((connection) => connection.protocol === "ssh"),
    [connections],
  );
  const tags = useMemo(
    () =>
      [...new Set(sshConnections.flatMap((connection) => connection.tags))]
        .filter(Boolean)
        .sort((a, b) => a.localeCompare(b, "zh-CN")),
    [sshConnections],
  );
  const onlineConnectionIds = useMemo(
    () =>
      new Set(
        sessions
          .filter(
            (session) =>
              session.status === "online" && session.sessionType !== "rdp",
          )
          .map((session) => session.connectionId),
      ),
    [sessions],
  );
  const filtered = useMemo(() => {
    const normalizedQuery = query.trim().toLocaleLowerCase("zh-CN");
    return sshConnections.filter((connection) => {
      const matchesQuery = [
        connection.name,
        connection.host,
        connection.username,
        ...connection.tags,
      ]
        .join(" ")
        .toLocaleLowerCase("zh-CN")
        .includes(normalizedQuery);
      const matchesFolder =
        folderFilter === "all" ||
        (folderFilter === "ungrouped"
          ? !connection.folderId
          : connection.folderId === folderFilter);
      const matchesTag =
        tagFilter === "all" || connection.tags.includes(tagFilter);
      return matchesQuery && matchesFolder && matchesTag;
    });
  }, [folderFilter, query, sshConnections, tagFilter]);
  const selectedProfiles = useMemo(
    () => sshConnections.filter((item) => selected.has(item.id)),
    [selected, sshConnections],
  );
  const duplicateEndpoints = useMemo(
    () => findDuplicateEndpoints(selectedProfiles),
    [selectedProfiles],
  );
  const allFilteredSelected =
    filtered.length > 0 && filtered.every((item) => selected.has(item.id));
  const isFiltering = Boolean(
    query.trim() || folderFilter !== "all" || tagFilter !== "all",
  );

  useEffect(() => {
    void api
      .listFolders()
      .then(setFolders)
      .catch((error) => onError(errorMessage(error)));
  }, [onError]);

  useEffect(() => {
    const listener = api.onBatchExecution((next) =>
      setExecution((current) => (current?.id === next.id ? next : current)),
    );
    return () => {
      void listener.then((unlisten) => unlisten());
    };
  }, []);

  const toggle = (id: string) =>
    setSelected((current) => {
      const next = new Set(current);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const toggleFiltered = (checked: boolean) =>
    setSelected((current) => {
      const next = new Set(current);
      for (const connection of filtered) {
        if (checked) next.add(connection.id);
        else next.delete(connection.id);
      }
      return next;
    });

  const run = async (ids = [...selected]) => {
    setBusy(true);
    try {
      const next = await api.startBatch(ids, command.trim(), concurrency);
      setExecution(next);
      setPreview(false);
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const retryFailed = () => {
    const failed =
      execution?.targets
        .filter((target) => target.status === "failed")
        .map((target) => target.connectionId) ?? [];
    if (failed.length) void run(failed);
  };

  const cancel = async () => {
    if (!execution) return;
    try {
      setExecution(await api.cancelBatch(execution.id));
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  const prepareSync = async () => {
    setBusy(true);
    const mapped = new Map<string, string>();
    try {
      for (const id of selected) {
        const profile = sshConnections.find((item) => item.id === id);
        if (!profile) continue;
        let session = [...useAppStore.getState().sessions]
          .reverse()
          .find(
            (item) =>
              item.connectionId === id &&
              item.sessionType !== "rdp" &&
              item.status === "online",
          );
        if (!session) {
          const before = new Set(
            useAppStore.getState().sessions.map((item) => item.id),
          );
          await connect(profile);
          session = useAppStore
            .getState()
            .sessions.find(
              (item) => !before.has(item.id) && item.connectionId === id,
            );
        }
        if (session) mapped.set(id, session.id);
      }
      setSyncSessions(mapped);
      if (mapped.size !== selected.size)
        onError("部分主机未建立会话；首次连接请先核对主机指纹后重试");
    } catch (error) {
      setSyncSessions(mapped);
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const sendSync = async () => {
    const value = command.trim();
    if (!value) return;
    if (isHighRiskCommand(value)) {
      onError("同步输入模式禁止直接发送明显破坏性命令，请逐台执行");
      return;
    }
    setBusy(true);
    try {
      await Promise.all(
        [...syncSessions.values()].map((sessionId) =>
          api.terminalInput(sessionId, value + "\r"),
        ),
      );
      setCommand("");
    } catch (error) {
      onError(errorMessage(error));
    } finally {
      setBusy(false);
    }
  };

  const running = execution && ["queued", "running"].includes(execution.status);

  return (
    <Modal title="多主机执行" onClose={onClose} wide>
      <div className="batch-dialog">
        <div className="segmented-control" role="tablist" aria-label="执行模式">
          <button
            role="tab"
            aria-selected={mode === "batch"}
            className={mode === "batch" ? "active" : ""}
            onClick={() => setMode("batch")}
          >
            <TerminalSquare size={14} />
            批量命令
          </button>
          <button
            role="tab"
            aria-selected={mode === "sync"}
            className={mode === "sync" ? "active" : ""}
            onClick={() => setMode("sync")}
          >
            <Send size={14} />
            同步输入
          </button>
        </div>
        <div className="batch-layout">
          <section className="batch-targets" aria-label="目标连接">
            <label className="batch-search">
              <Search size={13} />
              <input
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索名称、地址或标签"
                aria-label="筛选 SSH 连接"
              />
            </label>
            <div className="batch-target-filters">
              <select
                aria-label="按文件夹筛选目标"
                value={folderFilter}
                onChange={(event) => setFolderFilter(event.target.value)}
              >
                <option value="all">全部文件夹</option>
                <option value="ungrouped">未分组</option>
                {folders.map((folder) => (
                  <option key={folder.id} value={folder.id}>
                    {folder.name}
                  </option>
                ))}
              </select>
              <select
                aria-label="按标签筛选目标"
                value={tagFilter}
                onChange={(event) => setTagFilter(event.target.value)}
              >
                <option value="all">全部标签</option>
                {tags.map((tag) => (
                  <option key={tag} value={tag}>
                    {tag}
                  </option>
                ))}
              </select>
            </div>
            <div className="batch-select-all">
              <label>
                <input
                  type="checkbox"
                  checked={allFilteredSelected}
                  onChange={(event) => toggleFiltered(event.target.checked)}
                />
                <span>{isFiltering ? "选择筛选结果" : "选择全部"}</span>
              </label>
              <small>
                {selected.size}/{sshConnections.length}
              </small>
            </div>
            <div className="batch-connection-list">
              {filtered.map((connection) => {
                const online = onlineConnectionIds.has(connection.id);
                return (
                  <label key={connection.id}>
                    <input
                      type="checkbox"
                      checked={selected.has(connection.id)}
                      onChange={() => toggle(connection.id)}
                    />
                    <span>
                      <strong>{connection.name}</strong>
                      <small>
                        {connection.username}@{connection.host}:
                        {connection.port}
                      </small>
                      <small className="batch-connection-state">
                        <i className={online ? "online" : ""} />
                        {online ? "已连接" : "未连接"}
                        {connection.tags.length > 0 &&
                          " · " + connection.tags.join(" / ")}
                      </small>
                    </span>
                    {syncSessions.has(connection.id) && (
                      <CheckCircle2 size={13} aria-label="已加入同步" />
                    )}
                  </label>
                );
              })}
              {!filtered.length && (
                <p className="batch-no-targets">没有符合筛选条件的连接</p>
              )}
            </div>
          </section>
          <section className="batch-main">
            {duplicateEndpoints.length > 0 && (
              <div className="batch-target-warning" role="status">
                <AlertTriangle size={14} />
                <span>
                  所选目标中有重复地址：{duplicateEndpoints.join("、")}
                  。请核对是否确实需要重复执行。
                </span>
              </div>
            )}
            {mode === "batch" ? (
              !execution ? (
                preview ? (
                  <div className="batch-preview">
                    <p>
                      <AlertTriangle size={15} />
                      批量命令会在以下 {selectedProfiles.length}
                      台主机执行，请核对目标与命令。
                    </p>
                    <div>
                      {selectedProfiles.map((profile) => (
                        <span key={profile.id}>
                          {profile.name}
                          <small>
                            {profile.username}@{profile.host}:{profile.port}
                          </small>
                        </span>
                      ))}
                    </div>
                    <code>{command}</code>
                    <footer className="form-actions">
                      <button
                        className="button secondary"
                        onClick={() => setPreview(false)}
                      >
                        返回修改
                      </button>
                      <button
                        className="button primary"
                        onClick={() => void run()}
                        disabled={busy}
                      >
                        <Play size={14} />
                        确认执行
                      </button>
                    </footer>
                  </div>
                ) : (
                  <div className="batch-compose">
                    <label>
                      <span>命令</span>
                      <textarea
                        value={command}
                        onChange={(event) => setCommand(event.target.value)}
                        placeholder="输入要在所有目标执行的命令"
                        spellCheck={false}
                      />
                    </label>
                    <label className="batch-concurrency">
                      <span>并发上限</span>
                      <input
                        type="number"
                        min={1}
                        max={10}
                        value={concurrency}
                        onChange={(event) =>
                          setConcurrency(
                            Math.min(
                              10,
                              Math.max(1, Number(event.target.value) || 1),
                            ),
                          )
                        }
                      />
                    </label>
                    <p>
                      明显破坏性命令会被前后端共同拒绝。单台失败不会中止其他目标。
                    </p>
                    <footer className="form-actions">
                      <button
                        className="button primary"
                        disabled={!selected.size || !command.trim() || busy}
                        onClick={() => {
                          if (isHighRiskCommand(command)) {
                            onError("批量模式禁止明显破坏性命令，请逐台执行");
                            return;
                          }
                          setPreview(true);
                        }}
                      >
                        <Play size={14} />
                        预览执行
                      </button>
                    </footer>
                  </div>
                )
              ) : (
                <BatchResults
                  execution={execution}
                  expanded={expanded}
                  setExpanded={setExpanded}
                  onCancel={() => void cancel()}
                  onRetry={retryFailed}
                />
              )
            ) : (
              <div className="sync-input">
                <p>
                  <AlertTriangle size={15} />
                  同步输入会把同一行发送到所有已连接目标。请始终核对主机范围。
                </p>
                {!syncSessions.size ? (
                  <button
                    className="button primary"
                    disabled={!selected.size || busy}
                    onClick={() => void prepareSync()}
                  >
                    {busy ? (
                      <LoaderCircle className="spin" size={14} />
                    ) : (
                      <Play size={14} />
                    )}
                    {selected.size
                      ? "建立 " + selected.size + " 个同步会话"
                      : "选择目标后建立会话"}
                  </button>
                ) : (
                  <>
                    <div className="sync-target-summary">
                      <strong>{syncSessions.size} 台主机已就绪</strong>
                      <button
                        className="mini-button"
                        onClick={() => setSyncSessions(new Map())}
                      >
                        停止同步
                      </button>
                    </div>
                    <form
                      onSubmit={(event) => {
                        event.preventDefault();
                        void sendSync();
                      }}
                    >
                      <input
                        value={command}
                        onChange={(event) => setCommand(event.target.value)}
                        placeholder="输入一行并同步发送"
                        aria-label="同步命令"
                      />
                      <button
                        className="button primary"
                        disabled={!command.trim() || busy}
                      >
                        <Send size={14} />
                        发送
                      </button>
                    </form>
                    <div className="sync-controls">
                      <button
                        className="button secondary"
                        onClick={() =>
                          void Promise.all(
                            [...syncSessions.values()].map((id) =>
                              api.terminalInput(id, "\u0003"),
                            ),
                          )
                        }
                      >
                        发送 Ctrl+C
                      </button>
                      <button
                        className="button secondary"
                        onClick={() =>
                          void Promise.all(
                            [...syncSessions.values()].map((id) =>
                              api.terminalInput(id, "\u000c"),
                            ),
                          )
                        }
                      >
                        发送 Ctrl+L
                      </button>
                    </div>
                  </>
                )}
              </div>
            )}
          </section>
        </div>
        <footer className="form-actions">
          <button
            className="button secondary"
            onClick={onClose}
            disabled={Boolean(running)}
          >
            关闭
          </button>
        </footer>
      </div>
    </Modal>
  );
}

interface BatchResultsProps {
  execution: BatchExecution;
  expanded: Set<string>;
  setExpanded: (value: Set<string>) => void;
  onCancel: () => void;
  onRetry: () => void;
}

function BatchResults({
  execution,
  expanded,
  setExpanded,
  onCancel,
  onRetry,
}: BatchResultsProps) {
  const running = ["queued", "running"].includes(execution.status);
  const failed = execution.targets.filter(
    (target) => target.status === "failed",
  ).length;
  const completed = execution.targets.filter(
    (target) => target.status === "completed",
  ).length;
  return (
    <div className="batch-results">
      <header>
        <div>
          <strong>{statusLabel(execution.status)}</strong>
          <span>
            {completed}/{execution.targets.length} 成功
          </span>
        </div>
        {running ? (
          <button className="button secondary" onClick={onCancel}>
            <Square size={13} />
            取消
          </button>
        ) : (
          failed > 0 && (
            <button className="button secondary" onClick={onRetry}>
              <RefreshCw size={13} />
              仅重试失败项
            </button>
          )
        )}
      </header>
      {execution.targets.map((target) => {
        const open = expanded.has(target.connectionId);
        return (
          <div
            className={"batch-result " + target.status}
            key={target.connectionId}
          >
            <button
              onClick={() => {
                const next = new Set(expanded);
                if (open) next.delete(target.connectionId);
                else next.add(target.connectionId);
                setExpanded(next);
              }}
            >
              {open ? <ChevronDown size={13} /> : <ChevronRight size={13} />}
              <i />
              <strong>{target.name}</strong>
              <span>{statusLabel(target.status)}</span>
              {target.durationMs != null && (
                <small>{target.durationMs} ms</small>
              )}
              {target.exitCode != null && <code>exit {target.exitCode}</code>}
            </button>
            {open && (
              <div>
                <pre>
                  {target.stdout || target.stderr || target.error || "暂无输出"}
                </pre>
                {target.stderr && target.stdout && (
                  <pre className="stderr">{target.stderr}</pre>
                )}
                {target.error && <p>{target.error}</p>}
              </div>
            )}
          </div>
        );
      })}
    </div>
  );
}

const statusLabel = (status: string) =>
  (
    ({
      queued: "等待中",
      running: "执行中",
      completed: "已完成",
      failed: "失败",
      cancelled: "已取消",
    }) as Record<string, string>
  )[status] ?? status;

function findDuplicateEndpoints(connections: ConnectionProfile[]) {
  const endpoints = new Map<string, number>();
  for (const connection of connections) {
    const endpoint =
      connection.host.trim().toLocaleLowerCase("zh-CN") + ":" + connection.port;
    endpoints.set(endpoint, (endpoints.get(endpoint) ?? 0) + 1);
  }
  return [...endpoints.entries()]
    .filter(([, count]) => count > 1)
    .map(([endpoint]) => endpoint);
}
