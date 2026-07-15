import {
  Activity,
  ClipboardList,
  Clock3,
  Columns2,
  Command,
  Copy,
  FileClock,
  Files,
  Highlighter,
  History,
  MoreHorizontal,
  RefreshCw,
  Rows2,
  Search,
  ServerCog,
  Users,
  X,
} from "lucide-react";
import { createRef, useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import { IconButton } from "../../components/IconButton";
import { TerminalView, type TerminalActions } from "./TerminalView";
import { FileManager } from "../files/FileManager";
import { TransferQueue } from "../files/TransferQueue";
import { SystemInfoPanel } from "../monitor/SystemInfoPanel";
import type { ConnectionProfile, TerminalSession } from "../../types";

const isInteractiveTerminal = (
  session: TerminalSession | undefined,
): session is TerminalSession => Boolean(session && session.sessionType !== "rdp");
import { clampPanelSize, resizeFromKeyboard } from "../../lib/layout";
import { RdpWorkspace } from "../rdp/RdpWorkspace";
import "./TerminalWorkspace.css";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import { CommandPanel } from "./CommandPanel";
import { SessionLogDialog } from "./SessionLogDialog";
import { TriggerRulesDialog } from "./TriggerRulesDialog";
import { BatchExecutionDialog } from "./BatchExecutionDialog";
import { TerminalSplitLayout } from "./TerminalSplitLayout";
import {
  layoutRects,
  layoutSessions,
  leaf,
  removeLeaf,
  splitLeaf,
  updateRatio,
  validateLayout,
  type SplitDirection,
  type TerminalLayout,
} from "./terminal-layout";
import { GlobalTerminalSearch } from "./GlobalTerminalSearch";
import { PasteHistoryDialog, PasteSafetyDialog } from "./PasteSafetyDialog";
import { pasteRisk } from "./terminal-safety";
import { errorMessage } from "../../lib/format";
import { resolveTerminalPreferences, withTerminalFontSize } from "./terminal-preferences";

export default function TerminalWorkspace({
  connect,
}: {
  connect: (profile: ConnectionProfile) => Promise<void>;
}) {
  const {
    sessions,
    connections,
    activeSessionId,
    activePanel,
    setActiveSession,
    updateSession,
    removeSession,
    setPanel,
    settings,
    saveSettings,
    setError,
  } = useAppStore();
  const [findOpen, setFindOpen] = useState(false);
  const [query, setQuery] = useState("");
  const [bottomOpen, setBottomOpen] = useState(true);
  const [bottomHeight, setBottomHeight] = useState(() =>
    clampPanelSize(
      Number(localStorage.getItem("cnshell-bottom-height")) || 260,
      210,
      520,
    ),
  );
  const stackRef = useRef<HTMLDivElement>(null);
  const [logDialogOpen, setLogDialogOpen] = useState(false);
  const [triggerDialogOpen, setTriggerDialogOpen] = useState(false);
  const [batchDialogOpen, setBatchDialogOpen] = useState(false);
  const [globalSearchOpen, setGlobalSearchOpen] = useState(false);
  const [pasteHistoryOpen, setPasteHistoryOpen] = useState(false);
  const [pasteRequest, setPasteRequest] = useState<{
    sessionId: string;
    text: string;
  } | null>(null);
  const [copyMode, setCopyMode] = useState(false);
  const [showTimestamps, setShowTimestamps] = useState(false);
  const [activityVersion, setActivityVersion] = useState(0);
  const [terminalLayout, setTerminalLayout] = useState<TerminalLayout | null>(
    null,
  );
  const [tabMenu, setTabMenu] = useState<string | null>(null);
  const [refs] = useState(
    () => new Map<string, React.RefObject<TerminalActions | null>>(),
  );
  sessions.forEach((session) => {
    if (isInteractiveTerminal(session) && !refs.has(session.id))
      refs.set(session.id, createRef<TerminalActions>());
  });
  const close = useCallback(
    async (id: string) => {
      const session = useAppStore
        .getState()
        .sessions.find((item) => item.id === id);
      setTerminalLayout((current) =>
        current ? removeLeaf(current, id) : null,
      );
      try {
        if (session?.sessionType === "rdp") await api.rdpClose(id);
        else await api.closeTerminal(id);
      } catch {
        /* already disconnected */
      }
      removeSession(id);
      refs.delete(id);
      workspaceRuntime.remoteFileBrowserBySession.delete(id);
    },
    [refs, removeSession],
  );
  const profileFor = (session: TerminalSession) =>
    connections.find((item) => item.id === session.connectionId);
  const duplicate = async (session: TerminalSession) => {
    const profile = profileFor(session);
    if (profile) await connect(profile);
  };
  const reconnect = async (session: TerminalSession) => {
    const profile = profileFor(session);
    if (!profile) return;
    await close(session.id);
    await connect(profile);
  };
  const selectSession = useCallback(
    (id: string) => {
      const session = useAppStore
        .getState()
        .sessions.find((item) => item.id === id);
      if (isInteractiveTerminal(session)) {
        setTerminalLayout((current) =>
          current && layoutSessions(current).includes(id) ? current : leaf(id),
        );
        workspaceRuntime.terminalActivity.delete(id);
        setActivityVersion((value) => value + 1);
      }
      if(session?.sessionType==="rdp"&&(session.status==="online"||session.status==="reconnecting"))void api.rdpFocus(id).catch((reason)=>setError(errorMessage(reason)));
      setActiveSession(id);
    },
    [setActiveSession,setError],
  );
  const split = async (session: TerminalSession, direction: SplitDirection) => {
    if (session.sessionType === "rdp") return;
    const profile = profileFor(session);
    if (!profile) return;
    const before = new Set(
      useAppStore.getState().sessions.map((item) => item.id),
    );
    await connect(profile);
    const created = useAppStore
      .getState()
      .sessions.find((item) => !before.has(item.id));
    if (created) {
      setTerminalLayout((current) =>
        splitLeaf(
          current ?? leaf(session.id),
          session.id,
          created.id,
          direction,
        ),
      );
      setActiveSession(session.id);
    }
  };
  const removeFromLayout = (id: string) => {
    setTerminalLayout((current) => (current ? removeLeaf(current, id) : null));
    const remaining = terminalLayout
      ? layoutSessions(terminalLayout).filter((item) => item !== id)
      : [];
    if (remaining[0]) setActiveSession(remaining[0]);
  };
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (
        event.metaKey &&
        !event.shiftKey &&
        event.key.toLowerCase() === "f" &&
        sessions.find((item) => item.id === activeSessionId)?.sessionType ===
          "terminal"
      ) {
        event.preventDefault();
        setFindOpen(true);
      }
      if (event.metaKey && event.shiftKey && event.key.toLowerCase() === "f") {
        event.preventDefault();
        setGlobalSearchOpen(true);
      }
      if (
        event.metaKey &&
        event.shiftKey &&
        event.key.toLowerCase() === "c" &&
        activeSessionId
      ) {
        event.preventDefault();
        setCopyMode(true);
        refs.get(activeSessionId)?.current?.copyMode("start");
      }
      if (copyMode && activeSessionId) {
        if (event.key === "ArrowUp" || event.key === "ArrowDown") {
          event.preventDefault();
          refs
            .get(activeSessionId)
            ?.current?.copyMode(event.key === "ArrowUp" ? "up" : "down");
        }
        if (event.key === "Enter") {
          event.preventDefault();
          refs.get(activeSessionId)?.current?.copyMode("copy");
        }
        if (event.key === "Escape") {
          event.preventDefault();
          refs.get(activeSessionId)?.current?.copyMode("exit");
          setCopyMode(false);
        }
      }
      if (event.metaKey && event.key.toLowerCase() === "j") {
        event.preventDefault();
        setBottomOpen((value) => !value);
      }
      if (event.metaKey && event.key.toLowerCase() === "k" && activeSessionId) {
        event.preventDefault();
        refs.get(activeSessionId)?.current?.clear();
      }
      if(event.metaKey&&activeSessionId&&["=","+","-","0"].includes(event.key)){const session=sessions.find((item)=>item.id===activeSessionId);if(isInteractiveTerminal(session)){event.preventDefault();const current=resolveTerminalPreferences(settings,session.connectionId).fontSize;const next=event.key==="0"?13:current+(event.key==="-"?-1:1);void saveSettings(withTerminalFontSize(settings,session.connectionId,next)).catch((reason)=>setError(errorMessage(reason)));}}
      if (event.metaKey && event.key.toLowerCase() === "w" && activeSessionId) {
        const session = sessions.find((item) => item.id === activeSessionId);
        if (
          session &&
          (!settings.confirmCloseActiveSession ||
            confirm(`关闭“${session.title}”会话？`))
        ) {
          event.preventDefault();
          void close(session.id);
        }
      }
      if (event.metaKey && /^[1-9]$/.test(event.key)) {
        const session = sessions[Number(event.key) - 1];
        if (session) {
          event.preventDefault();
          selectSession(session.id);
        }
      }
    };
    window.addEventListener("keydown", handler);
    return () => window.removeEventListener("keydown", handler);
  }, [
    activeSessionId,
    close,
    copyMode,
    refs,
    selectSession,
    sessions,
    settings.confirmCloseActiveSession,
    settings,
    saveSettings,
    setError,
  ]);
  useEffect(() => {
    const paste = (event: Event) => {
      const detail = (event as CustomEvent<{ sessionId: string; text: string }>)
        .detail;
      if (!detail.text) return;
      workspaceRuntime.pasteHistory = [
        detail.text,
        ...workspaceRuntime.pasteHistory.filter((item) => item !== detail.text),
      ].slice(0, 20);
      const risk = pasteRisk(detail.text);
      if (risk.multiline || risk.highRisk) setPasteRequest(detail);
      else refs.get(detail.sessionId)?.current?.paste(detail.text);
    };
    const activity = () => setActivityVersion((value) => value + 1);
    window.addEventListener("cnshell-paste-request", paste);
    window.addEventListener("cnshell-terminal-activity", activity);
    return () => {
      window.removeEventListener("cnshell-paste-request", paste);
      window.removeEventListener("cnshell-terminal-activity", activity);
    };
  }, [refs]);
  useEffect(() => {
    const handler = () => {
      const session = useAppStore
        .getState()
        .sessions.find(
          (item) => item.id === useAppStore.getState().activeSessionId,
        );
      if (
        session &&
        (!useAppStore.getState().settings.confirmCloseActiveSession ||
          confirm(`关闭“${session.title}”会话？`))
      )
        void close(session.id);
    };
    window.addEventListener("cnshell-close-session", handler);
    return () => window.removeEventListener("cnshell-close-session", handler);
  }, [close]);
  useEffect(() => {
    localStorage.setItem("cnshell-bottom-height", String(bottomHeight));
  }, [bottomHeight]);
  useEffect(() => {
    workspaceRuntime.terminalLayout = terminalLayout;
    workspaceRuntime.bottomOpen = bottomOpen;
    workspaceRuntime.bottomHeight = bottomHeight;
  }, [terminalLayout, bottomOpen, bottomHeight]);
  useEffect(() => {
    const restore = (event: Event) => {
      const detail = (
        event as CustomEvent<{
          terminalLayout?: unknown;
          splitSessionId?: string | null;
          bottomOpen: boolean;
          bottomHeight: number;
        }>
      ).detail;
      const available = new Set(
        useAppStore
          .getState()
          .sessions.filter((item) => isInteractiveTerminal(item))
          .map((item) => item.id),
      );
      const restored =
        validateLayout(detail.terminalLayout, available) ??
        (detail.splitSessionId && available.has(detail.splitSessionId)
          ? leaf(detail.splitSessionId)
          : null);
      setTerminalLayout(restored);
      setBottomOpen(detail.bottomOpen);
      setBottomHeight(clampPanelSize(detail.bottomHeight, 210, 520));
    };
    window.addEventListener("cnshell-restore-layout", restore);
    return () => window.removeEventListener("cnshell-restore-layout", restore);
  }, []);
  useEffect(() => {
    const listener = api.onTerminalStatus((status) =>
      updateSession(status.sessionId, {
        status: status.status,
        lastError: status.lastError,
      }),
    );
    return () => {
      void listener.then((unlisten) => unlisten());
    };
  }, [updateSession]);
  useEffect(() => {
    const active = useAppStore
      .getState()
      .sessions.find(
        (item) => item.id === useAppStore.getState().activeSessionId,
      );
    if (isInteractiveTerminal(active))
      setTerminalLayout((current) => current ?? leaf(active.id));
  }, [activeSessionId]);
  if (!sessions.length)
    return (
      <main className="welcome-workspace">
        <div className="welcome-mark">
          <Command size={42} />
        </div>
        <span className="eyebrow">欢迎使用 CNshell</span>
        <h2>从左侧选择一台服务器</h2>
        <p>
          双击连接即可打开安全的 SSH
          终端。文件、传输与监控会自动绑定到当前会话。
        </p>
        <div className="shortcut-row">
          <kbd>⌘N</kbd>
          <span>新建连接</span>
          <kbd>⌘T</kbd>
          <span>打开终端</span>
          <kbd>⌘?</kbd>
          <span>查看帮助</span>
        </div>
      </main>
    );
  const active =
    sessions.find((item) => item.id === activeSessionId) ?? sessions[0];
  const effectiveLayout =
    isInteractiveTerminal(active)
      ? terminalLayout && layoutSessions(terminalLayout).includes(active.id)
        ? terminalLayout
        : leaf(active.id)
      : null;
  const rects = effectiveLayout ? layoutRects(effectiveLayout) : [];
  const rectBySession = new Map(rects.map((rect) => [rect.sessionId, rect]));
  const visibleIds = new Set(rectBySession.keys());
  return (
    <main className="workspace">
      <div
        className="session-tabs"
        role="tablist"
        aria-label="打开的会话"
        onKeyDown={(event) =>
          moveTabFocus(
            event,
            sessions.map((session) => session.id),
            active.id,
            selectSession,
            "session-tab-",
          )
        }
      >
        {sessions.map((session) => (
          <div className="session-tab-wrap" key={session.id}>
            <button
              id={`session-tab-${session.id}`}
              role="tab"
              aria-selected={session.id === active.id}
              aria-controls={`session-panel-${session.id}`}
              tabIndex={session.id === active.id ? 0 : -1}
              aria-label={`${session.title}，${sessionStatusLabel(session.status)}${session.lastError ? `，${session.lastError}` : ""}`}
              className={`session-tab ${session.id === active.id ? "active" : ""} ${visibleIds.has(session.id) ? "in-layout" : ""}`}
              onClick={() => selectSession(session.id)}
            >
              <span
                className={`status-dot ${session.status}`}
                aria-hidden="true"
              />
              <span>{session.title}</span>
              {workspaceRuntime.terminalActivity.has(session.id) && (
                <i className="activity-mark" aria-label="有后台活动" />
              )}
              {session.status !== "online" && (
                <small>{sessionStatusLabel(session.status)}</small>
              )}
            </button>
            <IconButton
              icon={MoreHorizontal}
              label={`${session.title} 会话操作`}
              className="tab-menu-trigger"
              aria-haspopup="menu"
              aria-expanded={tabMenu === session.id}
              onClick={() =>
                setTabMenu(tabMenu === session.id ? null : session.id)
              }
            />
            {tabMenu === session.id && (
              <div className="tab-context-menu" role="menu">
                <button role="menuitem" onClick={() => void duplicate(session)}>
                  <Copy size={13} />
                  复制会话
                </button>
                <button role="menuitem" onClick={() => void reconnect(session)}>
                  <RefreshCw size={13} />
                  重新连接
                </button>
                {isInteractiveTerminal(session) && (
                  <>
                    <button
                      role="menuitem"
                      onClick={() => void split(session, "vertical")}
                    >
                      <Columns2 size={13} />
                      左右拆分
                    </button>
                    <button
                      role="menuitem"
                      onClick={() => void split(session, "horizontal")}
                    >
                      <Rows2 size={13} />
                      上下拆分
                    </button>
                    {visibleIds.size > 1 && visibleIds.has(session.id) && (
                      <button
                        role="menuitem"
                        onClick={() => removeFromLayout(session.id)}
                      >
                        <X size={13} />
                        移出拆分布局
                      </button>
                    )}
                  </>
                )}
                <button
                  role="menuitem"
                  className="danger"
                  onClick={() => {
                    if (
                      !settings.confirmCloseActiveSession ||
                      confirm(`关闭“${session.title}”会话？`)
                    )
                      void close(session.id);
                  }}
                >
                  <X size={13} />
                  关闭
                </button>
              </div>
            )}
          </div>
        ))}
        <div className="tab-spacer" />
        <IconButton
          icon={Users}
          label="多主机执行"
          onClick={() => setBatchDialogOpen(true)}
        />
        {isInteractiveTerminal(active) && (
          <IconButton icon={ClipboardList} label="粘贴历史" onClick={() => setPasteHistoryOpen(true)} />
        )}
        {isInteractiveTerminal(active) && (
          <IconButton icon={Clock3} label="行时间戳" active={showTimestamps} onClick={() => setShowTimestamps(!showTimestamps)} />
        )}
        {isInteractiveTerminal(active) && (
          <IconButton icon={Search} label="跨标签搜索" onClick={() => setGlobalSearchOpen(true)} />
        )}
        {isInteractiveTerminal(active) && (
          <IconButton
            icon={Highlighter}
            label="高亮与通知"
            onClick={() => setTriggerDialogOpen(true)}
          />
        )}
        {isInteractiveTerminal(active) && (
          <IconButton
            icon={FileClock}
            label="会话日志"
            onClick={() => setLogDialogOpen(true)}
          />
        )}
        {isInteractiveTerminal(active) && (
          <IconButton
            icon={Search}
            label="搜索终端"
            onClick={() => setFindOpen(!findOpen)}
            active={findOpen}
          />
        )}
      </div>
      <div
        id={`session-panel-${active.id}`}
        role="tabpanel"
        aria-labelledby={`session-tab-${active.id}`}
        className="session-panel"
      >
        {active.sessionType === "rdp" ? (
          <RdpWorkspace
            session={active}
            onFocus={() => void api.rdpFocus(active.id).catch((reason)=>setError(errorMessage(reason)))}
            onHide={() => void api.rdpHide(active.id).catch((reason)=>setError(errorMessage(reason)))}
            onReconnect={() => void reconnect(active)}
            onClose={() => void close(active.id)}
          />
        ) : (
          <>
            {findOpen && (
              <form
                className="terminal-find"
                onSubmit={(event) => {
                  event.preventDefault();
                  refs.get(active.id)?.current?.findNext(query);
                }}
              >
                <Search size={14} />
                <input
                  autoFocus
                  value={query}
                  onChange={(event) => {
                    setQuery(event.target.value);
                    refs.get(active.id)?.current?.findNext(event.target.value);
                  }}
                  placeholder="在终端中查找"
                  aria-label="搜索终端输出"
                />
                <kbd>Return</kbd>
                <IconButton
                  icon={X}
                  label="关闭搜索"
                  onClick={() => setFindOpen(false)}
                />
              </form>
            )}
            <div
              className={`terminal-stack ${bottomOpen ? "with-bottom" : ""}`}
              ref={stackRef}
              style={
                bottomOpen
                  ? ({
                      "--bottom-panel-height": `${bottomHeight}px`,
                    } as React.CSSProperties)
                  : undefined
              }
            >
              <div className="terminal-area">
                {sessions
                  .filter((session) => isInteractiveTerminal(session))
                  .map((session) => {
                    const rect = rectBySession.get(session.id);
                    const style = rect
                      ? ({
                          left: `${rect.left}%`,
                          top: `${rect.top}%`,
                          width: `${rect.width}%`,
                          height: `${rect.height}%`,
                        } as React.CSSProperties)
                      : undefined;
                    return (
                      <TerminalView
                        key={session.id}
                        ref={refs.get(session.id)}
                        session={session}
                        visible={Boolean(rect)}
                        focused={session.id === active.id}
                        showTimestamps={showTimestamps}
                        style={style}
                      />
                    );
                  })}
                {effectiveLayout && (
                  <TerminalSplitLayout
                    layout={effectiveLayout}
                    onRatioChange={(id, ratio) =>
                      setTerminalLayout((current) =>
                        current ? updateRatio(current, id, ratio) : current,
                      )
                    }
                  />
                )}
              </div>
              {copyMode && <div className="copy-mode-badge">Copy Mode · ↑↓ 选择行 · Return 复制 · Esc 退出</div>}
              {bottomOpen && (
                <div
                  className="panel-resizer horizontal"
                  role="separator"
                  aria-label="调整底部工具区高度"
                  aria-orientation="horizontal"
                  aria-valuemin={210}
                  aria-valuemax={520}
                  aria-valuenow={bottomHeight}
                  tabIndex={0}
                  onPointerDown={(event) => {
                    event.currentTarget.setPointerCapture(event.pointerId);
                    const startY = event.clientY;
                    const initial = bottomHeight;
                    const maximum = Math.min(
                      520,
                      (stackRef.current?.clientHeight ?? 730) - 180,
                    );
                    const move = (moveEvent: PointerEvent) =>
                      setBottomHeight(
                        clampPanelSize(
                          initial + startY - moveEvent.clientY,
                          210,
                          maximum,
                        ),
                      );
                    const stop = () => {
                      window.removeEventListener("pointermove", move);
                      window.removeEventListener("pointerup", stop);
                    };
                    window.addEventListener("pointermove", move);
                    window.addEventListener("pointerup", stop);
                  }}
                  onKeyDown={(event) => {
                    const next = resizeFromKeyboard(
                      bottomHeight,
                      event.key,
                      "horizontal",
                    );
                    if (next === bottomHeight) return;
                    event.preventDefault();
                    setBottomHeight(
                      clampPanelSize(
                        next,
                        210,
                        Math.min(
                          520,
                          (stackRef.current?.clientHeight ?? 730) - 180,
                        ),
                      ),
                    );
                  }}
                />
              )}
              {bottomOpen && (
                <section className="bottom-panel">
                  <nav
                    className="panel-tabs"
                    aria-label="会话工具"
                    role="tablist"
                    onKeyDown={(event) =>
                      moveTabFocus(
                        event,
                        panelOrder,
                        activePanel,
                        setPanel,
                        "tool-tab-",
                      )
                    }
                  >
                    <button
                      id="tool-tab-files"
                      role="tab"
                      aria-selected={activePanel === "files"}
                      aria-controls="tool-panel"
                      tabIndex={activePanel === "files" ? 0 : -1}
                      className={activePanel === "files" ? "active" : ""}
                      onClick={() => setPanel("files")}
                    >
                      <Files size={15} />
                      文件
                    </button>
                    <button
                      id="tool-tab-commands"
                      role="tab"
                      aria-selected={activePanel === "commands"}
                      aria-controls="tool-panel"
                      tabIndex={activePanel === "commands" ? 0 : -1}
                      className={activePanel === "commands" ? "active" : ""}
                      onClick={() => setPanel("commands")}
                    >
                      <History size={15} />
                      快捷命令
                    </button>
                    <button
                      id="tool-tab-transfers"
                      role="tab"
                      aria-selected={activePanel === "transfers"}
                      aria-controls="tool-panel"
                      tabIndex={activePanel === "transfers" ? 0 : -1}
                      className={activePanel === "transfers" ? "active" : ""}
                      onClick={() => setPanel("transfers")}
                    >
                      <Activity size={15} />
                      传输
                    </button>
                    <button
                      id="tool-tab-system"
                      role="tab"
                      aria-selected={activePanel === "system"}
                      aria-controls="tool-panel"
                      tabIndex={activePanel === "system" ? 0 : -1}
                      className={activePanel === "system" ? "active" : ""}
                      onClick={() => setPanel("system")}
                    >
                      <ServerCog size={15} />
                      系统信息
                    </button>
                    <IconButton
                      icon={X}
                      label="折叠工具面板"
                      onClick={() => setBottomOpen(false)}
                    />
                  </nav>
                  <div
                    id="tool-panel"
                    role="tabpanel"
                    aria-labelledby={`tool-tab-${activePanel}`}
                    className="panel-content"
                  >
                    {activePanel === "files" && (active.sessionType === "local" ? <div className="empty-files"><Files size={28}/><span>本地 Shell 使用终端命令访问本机文件</span></div> : <FileManager key={active.id} session={active} />)}{" "}
                    {activePanel === "commands" && (
                      <CommandPanel session={active} onError={setError} />
                    )}{" "}
                    {activePanel === "transfers" && <TransferQueue />}{" "}
                    {activePanel === "system" && (
                      <SystemInfoPanel sessionId={active.id} />
                    )}
                  </div>
                </section>
              )}
            </div>
          </>
        )}
      </div>
      {logDialogOpen && isInteractiveTerminal(active) && (
        <SessionLogDialog
          session={active}
          onClose={() => setLogDialogOpen(false)}
          onError={(message) => setError(message)}
        />
      )}
      {triggerDialogOpen && isInteractiveTerminal(active) && (
        <TriggerRulesDialog
          session={active}
          onClose={() => setTriggerDialogOpen(false)}
          onError={(message) => setError(message)}
        />
      )}
      {batchDialogOpen && (
        <BatchExecutionDialog
          connections={connections}
          connect={connect}
          onClose={() => setBatchDialogOpen(false)}
          onError={(message) => setError(message)}
        />
      )}
      {pasteRequest && (
        <PasteSafetyDialog text={pasteRequest.text} onClose={() => setPasteRequest(null)} onConfirm={() => { refs.get(pasteRequest.sessionId)?.current?.paste(pasteRequest.text); setPasteRequest(null); }} />
      )}
      {pasteHistoryOpen && (
        <PasteHistoryDialog items={workspaceRuntime.pasteHistory} onClose={() => setPasteHistoryOpen(false)} onClear={() => { workspaceRuntime.pasteHistory = []; setActivityVersion(activityVersion + 1); }} onSelect={(text) => { setPasteHistoryOpen(false); const request = { sessionId: active.id, text }; const risk = pasteRisk(text); if (risk.multiline || risk.highRisk) setPasteRequest(request); else refs.get(active.id)?.current?.paste(text); }} />
      )}
      {globalSearchOpen && (
        <GlobalTerminalSearch sessions={sessions.filter((item) => isInteractiveTerminal(item))} onClose={() => setGlobalSearchOpen(false)} onSelect={(sessionId, line) => { selectSession(sessionId); requestAnimationFrame(() => refs.get(sessionId)?.current?.selectLine(line)); setGlobalSearchOpen(false); }} />
      )}
    </main>
  );
}

const sessionStatusLabel = (status: TerminalSession["status"]) =>
  ({
    connecting: "连接中",
    online: "在线",
    reconnecting: "重连中",
    failed: "失败",
    closed: "已关闭",
  })[status];
const panelOrder = ["files", "commands", "transfers", "system"] as const;

function moveTabFocus<T extends string>(
  event: React.KeyboardEvent,
  ids: readonly T[],
  active: T,
  select: (id: T) => void,
  idPrefix: string,
) {
  if ((event.target as HTMLElement).getAttribute("role") !== "tab") return;
  if (!["ArrowLeft", "ArrowRight", "Home", "End"].includes(event.key)) return;
  event.preventDefault();
  const current = Math.max(0, ids.indexOf(active));
  const next =
    event.key === "Home"
      ? 0
      : event.key === "End"
        ? ids.length - 1
        : event.key === "ArrowRight"
          ? (current + 1) % ids.length
          : (current - 1 + ids.length) % ids.length;
  const id = ids[next];
  if (!id) return;
  select(id);
  requestAnimationFrame(() =>
    document.getElementById(`${idPrefix}${id}`)?.focus(),
  );
}
