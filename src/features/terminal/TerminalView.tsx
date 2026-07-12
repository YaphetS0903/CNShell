import {
  forwardRef,
  useEffect,
  useImperativeHandle,
  useRef,
  useState,
} from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { createTerminalInputQueue } from "../../lib/terminal-input";
import { parseOsc7Cwd, workspaceRuntime } from "../../lib/workspace-runtime";
import {
  accessibleRuleColors,
  findTriggerMatches,
  loadTriggerConfig,
  notifyTerminal,
  terminalCellWidth,
  type TriggerConfig,
  type TriggerEvent,
} from "./terminal-triggers";
import { searchBuffer } from "./terminal-safety";

export interface TerminalActions {
  findNext: (term: string) => boolean;
  clear: () => void;
  focus: () => void;
  paste: (text: string) => void;
  copyMode: (action: "start" | "up" | "down" | "copy" | "exit") => void;
  selectLine: (line: number) => void;
}

export const TerminalView = forwardRef<
  TerminalActions,
  {
    session: TerminalSession;
    visible: boolean;
    focused: boolean;
    showTimestamps?: boolean;
    style?: React.CSSProperties;
  }
>(({ session, visible, focused, showTimestamps = false, style }, ref) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const hostRef = useRef<HTMLDivElement>(null);
  const [timestampRows, setTimestampRows] = useState<
    { line: number; timestamp: number | null }[]
  >([]);
  const terminalRef = useRef<Terminal | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const activeRef = useRef(focused);
  activeRef.current = focused;
  const copyLineRef = useRef<number | null>(null);
  useImperativeHandle(
    ref,
    () => ({
      findNext: (term) =>
        searchRef.current?.findNext(term, { incremental: true }) ?? false,
      clear: () => terminalRef.current?.clear(),
      focus: () => terminalRef.current?.focus(),
      paste: (text) => terminalRef.current?.paste(text),
      copyMode: (action) => {
        const terminal = terminalRef.current;
        if (!terminal) return;
        const buffer = terminal.buffer.active;
        if (action === "exit") {
          copyLineRef.current = null;
          terminal.clearSelection();
          terminal.focus();
          return;
        }
        if (action === "start")
          copyLineRef.current = buffer.baseY + buffer.cursorY;
        if (copyLineRef.current == null) return;
        if (action === "up")
          copyLineRef.current = Math.max(0, copyLineRef.current - 1);
        if (action === "down")
          copyLineRef.current = Math.min(
            buffer.length - 1,
            copyLineRef.current + 1,
          );
        terminal.selectLines(copyLineRef.current, copyLineRef.current);
        terminal.scrollToLine(copyLineRef.current);
        if (action === "copy")
          void navigator.clipboard.writeText(terminal.getSelection());
      },
      selectLine: (line) => {
        const terminal = terminalRef.current;
        if (!terminal) return;
        terminal.selectLines(line, line);
        terminal.scrollToLine(line);
        terminal.focus();
      },
    }),
    [],
  );
  useEffect(() => {
    const container = hostRef.current;
    if (!container) return;
    const terminal = new Terminal({
      allowProposedApi: false,
      cursorBlink: true,
      cursorStyle: "bar",
      convertEol: false,
      scrollback: 10000,
      fontFamily: "SFMono-Regular, Menlo, Monaco, Consolas, monospace",
      fontSize: 13,
      lineHeight: 1.25,
      theme: {
        background: "#07101d",
        foreground: "#dce6f4",
        cursor: "#4ade80",
        cursorAccent: "#07101d",
        selectionBackground: "#315b8e88",
        black: "#111827",
        red: "#fb7185",
        green: "#4ade80",
        yellow: "#facc15",
        blue: "#60a5fa",
        magenta: "#c084fc",
        cyan: "#22d3ee",
        white: "#e5e7eb",
        brightBlack: "#64748b",
      },
    });
    const fit = new FitAddon();
    const search = new SearchAddon();
    terminal.loadAddon(fit);
    terminal.loadAddon(search);
    terminal.loadAddon(new WebLinksAddon());
    searchRef.current = search;
    const cwdHandler = terminal.parser.registerOscHandler(7, (value) => {
      const cwd = parseOsc7Cwd(value);
      if (cwd) workspaceRuntime.cwdBySession.set(session.id, cwd);
      return Boolean(cwd);
    });
    let triggerConfig = loadTriggerConfig();
    const applyCursor = () => {
      terminal.options.cursorStyle = triggerConfig.enhancedCursor
        ? "block"
        : "bar";
      terminal.options.cursorBlink = true;
    };
    const configHandler = (event: Event) => {
      triggerConfig = (event as CustomEvent<TriggerConfig>).detail;
      applyCursor();
    };
    window.addEventListener("cnshell-trigger-config", configHandler);
    applyCursor();
    const cooldowns = new Map<string, number>();
    const processed = new Set<string>();
    const decorations = new Map<number, { dispose: () => void }[]>();
    let taskStarted: number | null = null;
    let taskTimer: number | null = null;
    const allowed = (key: string, seconds: number) => {
      const now = Date.now(),
        previous = cooldowns.get(key) ?? 0;
      if (now - previous < seconds * 1000) return false;
      cooldowns.set(key, now);
      return true;
    };
    const notify = (
      key: string,
      title: string,
      body: string,
      cooldown = 30,
    ) => {
      if (triggerConfig.notificationsEnabled && allowed(key, cooldown))
        notifyTerminal(title, body);
    };
    const promptHandler = terminal.parser.registerOscHandler(133, (value) => {
      if (value.startsWith("C")) taskStarted = Date.now();
      if (value.startsWith("A")) completeLongTask();
      return true;
    });
    const completeLongTask = () => {
      if (
        taskStarted &&
        triggerConfig.longTaskNotifications &&
        Date.now() - taskStarted >= triggerConfig.longTaskSeconds * 1000
      )
        notify(
          `task-${session.id}`,
          `${session.title} · 任务完成`,
          `运行时间 ${Math.round((Date.now() - taskStarted) / 1000)} 秒`,
          5,
        );
      taskStarted = null;
    };
    const schedulePromptCheck = () => {
      if (taskTimer != null) window.clearTimeout(taskTimer);
      taskTimer = window.setTimeout(() => {
        const buffer = terminal.buffer.active;
        const line =
          buffer
            .getLine(buffer.baseY + buffer.cursorY)
            ?.translateToString(true) ?? "";
        if (/(?:[$#%>] |❯ )$/.test(line)) completeLongTask();
      }, 700);
    };
    const decorate = (start: number) => {
      const buffer = terminal.buffer.active;
      const current = buffer.baseY + buffer.cursorY;
      for (let row = Math.max(0, start); row <= current; row += 1) {
        decorations.get(row)?.forEach((item) => item.dispose());
        decorations.delete(row);
        const line = buffer.getLine(row)?.translateToString(true) ?? "";
        const items: { dispose: () => void }[] = [];
        for (const match of findTriggerMatches(line, triggerConfig.rules)) {
          const marker = terminal.registerMarker(row - current);
          const colors = accessibleRuleColors(
            match.rule.foreground,
            match.rule.background,
            triggerConfig.enforceContrast,
          );
          const decoration = terminal.registerDecoration({
            marker,
            x: terminalCellWidth(line.slice(0, match.index)),
            width: Math.max(1, terminalCellWidth(match.text)),
            foregroundColor: colors.foreground,
            backgroundColor: colors.background,
          });
          if (decoration) {
            if (match.rule.bold)
              decoration.onRender((element) => {
                element.style.fontWeight = "700";
              });
            items.push(decoration);
          } else marker.dispose();
          const eventKey = `${match.rule.id}:${row}:${match.index}:${line}`;
          if (processed.has(eventKey)) continue;
          processed.add(eventKey);
          if (processed.size > 5000)
            processed.delete(processed.values().next().value!);
          if (match.rule.recordEvent) {
            const event: TriggerEvent = {
              id: crypto.randomUUID(),
              sessionId: session.id,
              ruleId: match.rule.id,
              ruleName: match.rule.name,
              text: match.text,
              timestamp: new Date().toISOString(),
            };
            const events =
              workspaceRuntime.triggerEventsBySession.get(session.id) ?? [];
            workspaceRuntime.triggerEventsBySession.set(
              session.id,
              [event, ...events].slice(0, 100),
            );
          }
          if (match.rule.notify)
            notify(
              `${session.id}-${match.rule.id}`,
              `${session.title} · ${match.rule.name}`,
              match.text,
              match.rule.cooldownSeconds,
            );
        }
        if (items.length) decorations.set(row, items);
      }
    };
    const updateTimestampRows = () => {
      const buffer = terminal.buffer.active;
      const timestamps = new Map(
        (
          workspaceRuntime.terminalTimestampsBySession.get(session.id) ?? []
        ).map((item) => [item.line, item.timestamp]),
      );
      setTimestampRows(
        Array.from({ length: terminal.rows }, (_, offset) => {
          const line = buffer.viewportY + offset;
          return { line, timestamp: timestamps.get(line) ?? null };
        }),
      );
    };
    terminal.open(container);
    fit.fit();
    terminal.focus();
    terminalRef.current = terminal;
    updateTimestampRows();
    const enqueueInput = createTerminalInputQueue((data) =>
      api.terminalInput(session.id, data),
    );
    const dataDisposable = terminal.onData((data) => {
      if (data.includes("\r")) taskStarted = Date.now();
      if (api.isDesktop()) void enqueueInput(data).catch(() => undefined);
      else terminal.write(data);
    });
    const bellDisposable = terminal.onBell(() => {
      if (triggerConfig.bellNotifications)
        notify(
          `bell-${session.id}`,
          `${session.title} · 终端 Bell`,
          "远端终端请求你的注意",
          10,
        );
    });
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (
        event.metaKey &&
        event.key.toLowerCase() === "c" &&
        terminal.hasSelection()
      ) {
        void navigator.clipboard.writeText(terminal.getSelection());
        return false;
      }
      if (event.metaKey && event.key.toLowerCase() === "v") {
        void navigator.clipboard
          .readText()
          .then((text) =>
            window.dispatchEvent(
              new CustomEvent("cnshell-paste-request", {
                detail: { sessionId: session.id, text },
              }),
            ),
          );
        return false;
      }
      return !(
        event.metaKey && ["f", "k", "w", "t"].includes(event.key.toLowerCase())
      );
    });
    const scrollDisposable = terminal.onScroll(updateTimestampRows);
    const resize = new ResizeObserver(() => {
      fit.fit();
      updateTimestampRows();
      void api.terminalResize(session.id, terminal.cols, terminal.rows);
    });
    resize.observe(container);
    const searchLines = (query: string) => {
      const buffer = terminal.buffer.normal;
      const lines = Array.from(
        { length: buffer.length },
        (_, line) => buffer.getLine(line)?.translateToString(true) ?? "",
      );
      return searchBuffer(lines, query, session.id);
    };
    workspaceRuntime.terminalSearchBySession.set(session.id, searchLines);
    const pending: Uint8Array[] = [];
    let writing = false;
    let disposed = false;
    const flush = () => {
      if (disposed || writing || !pending.length) return;
      writing = true;
      let length = 0;
      const chunks: Uint8Array[] = [];
      while (pending.length && length < 64 * 1024) {
        const chunk = pending.shift()!;
        chunks.push(chunk);
        length += chunk.length;
      }
      const combined = new Uint8Array(length);
      let offset = 0;
      for (const chunk of chunks) {
        combined.set(chunk, offset);
        offset += chunk.length;
      }
      const start = Math.max(
        0,
        terminal.buffer.active.baseY + terminal.buffer.active.cursorY - 1,
      );
      terminal.write(combined, () => {
        try {
          decorate(start);
          const buffer = terminal.buffer.active;
          const current = buffer.baseY + buffer.cursorY;
          const timestamps =
            workspaceRuntime.terminalTimestampsBySession.get(session.id) ?? [];
          const known = new Set(timestamps.map((item) => item.line));
          for (let line = start; line <= current; line += 1)
            if (!known.has(line))
              timestamps.push({ line, timestamp: Date.now() });
          workspaceRuntime.terminalTimestampsBySession.set(
            session.id,
            timestamps.slice(-10_000),
          );
          updateTimestampRows();
          schedulePromptCheck();
        } catch {
          /* decoration failures must never stall terminal output */
        } finally {
          writing = false;
          flush();
        }
      });
    };
    const unlistenPromise = api.onTerminalOutput((output) => {
      if (output.sessionId === session.id) {
        if (!activeRef.current) {
          workspaceRuntime.terminalActivity.add(session.id);
          window.dispatchEvent(
            new CustomEvent("cnshell-terminal-activity", {
              detail: { sessionId: session.id },
            }),
          );
          if (triggerConfig.backgroundNotifications)
            notify(
              `background-${session.id}`,
              `${session.title} · 后台活动`,
              "终端收到新的远端输出",
              30,
            );
        }
        const binary = atob(output.dataBase64);
        pending.push(
          Uint8Array.from(binary, (character) => character.charCodeAt(0)),
        );
        flush();
      }
    });
    if (!api.isDesktop())
      terminal.writeln(
        "\x1b[1;32mCNshell 浏览器预览\x1b[0m\r\n\r\n请运行 \x1b[36mnpm run tauri dev\x1b[0m 建立真实 SSH 会话。\r\n",
      );
    return () => {
      disposed = true;
      pending.length = 0;
      if (taskTimer != null) window.clearTimeout(taskTimer);
      window.removeEventListener("cnshell-trigger-config", configHandler);
      workspaceRuntime.terminalSearchBySession.delete(session.id);
      workspaceRuntime.terminalTimestampsBySession.delete(session.id);
      decorations.forEach((items) => items.forEach((item) => item.dispose()));
      void unlistenPromise.then((unlisten) => unlisten());
      resize.disconnect();
      scrollDisposable.dispose();
      dataDisposable.dispose();
      bellDisposable.dispose();
      cwdHandler.dispose();
      promptHandler.dispose();
      terminal.dispose();
      terminalRef.current = null;
    };
  }, [session.id, session.title]);
  useEffect(() => {
    if (focused) terminalRef.current?.focus();
  }, [focused]);
  return (
    <div
      className={`terminal-instance ${visible ? "active" : ""} ${showTimestamps ? "with-timestamps" : ""}`}
      ref={containerRef}
      style={style}
      aria-label={`${session.title} 终端`}
    >
      <div className="terminal-host" ref={hostRef} />
      {showTimestamps && (
        <div className="terminal-timestamp-gutter" aria-label="终端逐行时间戳">
          {timestampRows.map((item) => (
            <time key={item.line}>
              {item.timestamp
                ? new Date(item.timestamp).toLocaleTimeString([], {
                    hour12: false,
                    hour: "2-digit",
                    minute: "2-digit",
                    second: "2-digit",
                  })
                : ""}
            </time>
          ))}
        </div>
      )}
    </div>
  );
});

TerminalView.displayName = "TerminalView";
