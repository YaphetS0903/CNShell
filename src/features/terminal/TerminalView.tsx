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
import { useAppStore } from "../../store/app-store";
import { resolveTerminalPreferences, terminalFontFamilies, terminalThemes } from "./terminal-preferences";
import { open } from "@tauri-apps/plugin-dialog";
import type { ZmodemEvent } from "../../types";
import { primaryShortcutPressed } from "../../lib/platform";

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
  const [zmodem, setZmodem] = useState<ZmodemEvent | null>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const triggerConfigRef = useRef<TriggerConfig | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  const activeRef = useRef(focused);
  const settings = useAppStore((state)=>state.settings);
  const preferences = resolveTerminalPreferences(settings,session.connectionId);
  const preferencesRef = useRef(preferences);
  preferencesRef.current = preferences;
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
    const initialPreferences=preferencesRef.current;
    const terminal = new Terminal({
      allowProposedApi: false,
      cursorBlink: initialPreferences.cursorBlink,
      cursorStyle: initialPreferences.cursorStyle,
      convertEol: false,
      scrollback: initialPreferences.scrollback,
      fontFamily: terminalFontFamilies[initialPreferences.fontFamily],
      fontSize: initialPreferences.fontSize,
      lineHeight: initialPreferences.lineHeight,
      theme: terminalThemes[initialPreferences.colorScheme],
    });
    const fit = new FitAddon();
    fitRef.current=fit;
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
    triggerConfigRef.current=triggerConfig;
    const applyCursor = () => {
      terminal.options.cursorStyle = triggerConfig.enhancedCursor
        ? "block"
        : preferencesRef.current.cursorStyle;
      terminal.options.cursorBlink = preferencesRef.current.cursorBlink;
    };
    const configHandler = (event: Event) => {
      triggerConfig = (event as CustomEvent<TriggerConfig>).detail;
      triggerConfigRef.current=triggerConfig;
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
    const selectionDisposable = terminal.onSelectionChange(() => {
      const selected = terminal.getSelection();
      if (selected) workspaceRuntime.terminalSelectionBySession.set(session.id, selected.slice(0, 64 * 1024));
      else workspaceRuntime.terminalSelectionBySession.delete(session.id);
    });
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      const primary = primaryShortcutPressed(event);
      if (
        primary &&
        event.key.toLowerCase() === "c" &&
        terminal.hasSelection()
      ) {
        void navigator.clipboard.writeText(terminal.getSelection());
        return false;
      }
      if (primary && event.key.toLowerCase() === "v") {
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
        primary && ["f", "k", "w", "t", "=", "+", "-", "0"].includes(event.key.toLowerCase())
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
      workspaceRuntime.terminalSelectionBySession.delete(session.id);
      decorations.forEach((items) => items.forEach((item) => item.dispose()));
      void unlistenPromise.then((unlisten) => unlisten());
      resize.disconnect();
      scrollDisposable.dispose();
      dataDisposable.dispose();
      bellDisposable.dispose();
      selectionDisposable.dispose();
      cwdHandler.dispose();
      promptHandler.dispose();
      terminal.dispose();
      terminalRef.current = null;
      fitRef.current = null;
      triggerConfigRef.current = null;
    };
  }, [session.id, session.title]);
  useEffect(() => {
    const terminal = terminalRef.current;
    if (!terminal) return;
    terminal.options.fontFamily = terminalFontFamilies[preferences.fontFamily];
    terminal.options.fontSize = preferences.fontSize;
    terminal.options.lineHeight = preferences.lineHeight;
    terminal.options.scrollback = preferences.scrollback;
    terminal.options.theme = terminalThemes[preferences.colorScheme];
    terminal.options.cursorStyle = triggerConfigRef.current?.enhancedCursor
      ? "block"
      : preferences.cursorStyle;
    terminal.options.cursorBlink = preferences.cursorBlink;
    if (containerRef.current) {
      containerRef.current.style.backgroundColor =
        terminalThemes[preferences.colorScheme].background ?? "";
    }
    requestAnimationFrame(() => {
      if (terminalRef.current !== terminal) return;
      fitRef.current?.fit();
      void api.terminalResize(session.id, terminal.cols, terminal.rows);
    });
  }, [preferences, session.id]);
  useEffect(() => {
    if (focused) terminalRef.current?.focus();
  }, [focused]);
  useEffect(() => {
    let disposed=false;
    const unlisten=api.onZmodemEvent((event)=>{
      if(event.sessionId!==session.id)return;
      setZmodem(event);
      if(["completed","failed","cancelled"].includes(event.status))
        window.setTimeout(()=>setZmodem((current)=>current?.id===event.id?null:current),5000);
    });
    return()=>{disposed=true;void unlisten.then((stop)=>{if(disposed)stop();});};
  },[session.id]);
  const authorizeZmodem=async()=>{
    if(!zmodem)return;
    const path=await open({
      multiple:zmodem.direction==="upload",
      directory:zmodem.direction==="download",
      title:zmodem.direction==="download"?"选择 Zmodem 下载目录":"选择 Zmodem 上传文件",
    });
    if(!path){await api.cancelZmodem(session.id,zmodem.id);return;}
    try{setZmodem(await api.startZmodem(session.id,zmodem.id,Array.isArray(path)?path:[path]));}
    catch(error){setZmodem((current)=>current?{...current,status:"failed",error:String(error)}:current);window.setTimeout(()=>setZmodem((current)=>current?.id===zmodem.id?null:current),5000);}
  };
  const cancelZmodem=async()=>{
    if(!zmodem)return;
    try{setZmodem(await api.cancelZmodem(session.id,zmodem.id));}
    catch(error){setZmodem((current)=>current?{...current,status:"failed",error:String(error)}:current);window.setTimeout(()=>setZmodem((current)=>current?.id===zmodem.id?null:current),5000);}
  };
  const zmodemPercent=zmodem?.totalBytes?Math.min(100,Math.round(zmodem.transferredBytes/zmodem.totalBytes*100)):null;
  return (
    <div
      className={`terminal-instance ${visible ? "active" : ""} ${showTimestamps ? "with-timestamps" : ""}`}
      ref={containerRef}
      style={style}
      aria-label={`${session.title} 终端`}
    >
      <div className="terminal-host" ref={hostRef} />
      {zmodem&&<section className="zmodem-card" role="status" aria-live="polite">
        <header><strong>Zmodem {zmodem.direction==="download"?"下载":"上传"}</strong><span>{zmodem.status==="awaitingAuthorization"?"等待授权":zmodem.status==="running"?"传输中":zmodem.status==="completed"?"已完成":zmodem.status==="cancelled"?"已取消":"失败"}</span></header>
        {zmodem.fileName&&<div className="zmodem-file" title={zmodem.fileName}>{zmodem.fileName}</div>}
        {zmodem.status==="awaitingAuthorization"?<p>{zmodem.direction==="download"?"远端正在发送文件，请选择保存目录。":"远端正在接收文件，请选择要上传的文件。"}</p>:<>
          <div className="zmodem-progress"><i style={{width:`${zmodemPercent??0}%`}}/></div>
          <p>{zmodem.transferredBytes.toLocaleString()} bytes{zmodem.totalBytes!=null?` / ${zmodem.totalBytes.toLocaleString()} bytes`:""}{zmodemPercent!=null?` · ${zmodemPercent}%`:""}</p>
        </>}
        {zmodem.error&&<p className="zmodem-error">{zmodem.error}</p>}
        <footer>{zmodem.status==="awaitingAuthorization"&&<button className="button primary" onClick={()=>void authorizeZmodem()}>选择{zmodem.direction==="download"?"目录":"文件"}</button>}{["awaitingAuthorization","running"].includes(zmodem.status)&&<button className="button secondary" onClick={()=>void cancelZmodem()}>取消</button>}</footer>
      </section>}
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
