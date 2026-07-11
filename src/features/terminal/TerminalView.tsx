import { forwardRef, useEffect, useImperativeHandle, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { SearchAddon } from "@xterm/addon-search";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { api } from "../../lib/api";
import type { TerminalSession } from "../../types";
import { createTerminalInputQueue } from "../../lib/terminal-input";
import { parseOsc7Cwd, workspaceRuntime } from "../../lib/workspace-runtime";

export interface TerminalActions { findNext: (term: string) => boolean; clear: () => void; focus: () => void }

export const TerminalView = forwardRef<TerminalActions, { session: TerminalSession; active: boolean; pane?: "primary"|"secondary" }>(({ session, active, pane = "primary" }, ref) => {
  const containerRef = useRef<HTMLDivElement>(null);
  const terminalRef = useRef<Terminal | null>(null);
  const searchRef = useRef<SearchAddon | null>(null);
  useImperativeHandle(ref, () => ({ findNext: (term) => searchRef.current?.findNext(term, { incremental: true }) ?? false, clear: () => terminalRef.current?.clear(), focus: () => terminalRef.current?.focus() }), []);
  useEffect(() => {
    const container = containerRef.current; if (!container) return;
    const terminal = new Terminal({
      allowProposedApi: false, cursorBlink: true, cursorStyle: "bar", convertEol: false, scrollback: 10000,
      fontFamily: "SFMono-Regular, Menlo, Monaco, Consolas, monospace", fontSize: 13, lineHeight: 1.25,
      theme: { background: "#07101d", foreground: "#dce6f4", cursor: "#4ade80", cursorAccent: "#07101d", selectionBackground: "#315b8e88", black: "#111827", red: "#fb7185", green: "#4ade80", yellow: "#facc15", blue: "#60a5fa", magenta: "#c084fc", cyan: "#22d3ee", white: "#e5e7eb", brightBlack: "#64748b" }
    });
    const fit = new FitAddon(); const search = new SearchAddon();
    terminal.loadAddon(fit); terminal.loadAddon(search); terminal.loadAddon(new WebLinksAddon()); searchRef.current = search;
    const cwdHandler=terminal.parser.registerOscHandler(7,(value)=>{const cwd=parseOsc7Cwd(value);if(cwd)workspaceRuntime.cwdBySession.set(session.id,cwd);return Boolean(cwd);});
    terminal.open(container); fit.fit(); terminal.focus(); terminalRef.current = terminal;
    const enqueueInput=createTerminalInputQueue((data)=>api.terminalInput(session.id,data));
    const dataDisposable = terminal.onData((data) => {if(api.isDesktop())void enqueueInput(data).catch(()=>undefined);else terminal.write(data);});
    terminal.attachCustomKeyEventHandler((event) => {
      if (event.type !== "keydown") return true;
      if (event.metaKey && event.key.toLowerCase() === "c" && terminal.hasSelection()) { void navigator.clipboard.writeText(terminal.getSelection()); return false; }
      if (event.metaKey && event.key.toLowerCase() === "v") { void navigator.clipboard.readText().then((text) => terminal.paste(text)); return false; }
      return !(event.metaKey && ["f", "k", "w", "t"].includes(event.key.toLowerCase()));
    });
    const resize = new ResizeObserver(() => { fit.fit(); void api.terminalResize(session.id, terminal.cols, terminal.rows); }); resize.observe(container);
    const pending:Uint8Array[]=[];let writing=false;let disposed=false;const flush=()=>{if(disposed||writing||!pending.length)return;writing=true;let length=0;const chunks:Uint8Array[]=[];while(pending.length&&length<64*1024){const chunk=pending.shift()!;chunks.push(chunk);length+=chunk.length;}const combined=new Uint8Array(length);let offset=0;for(const chunk of chunks){combined.set(chunk,offset);offset+=chunk.length;}terminal.write(combined,()=>{writing=false;flush();});};
    const unlistenPromise = api.onTerminalOutput((output) => { if (output.sessionId === session.id){const binary=atob(output.dataBase64);pending.push(Uint8Array.from(binary,(character)=>character.charCodeAt(0)));flush();} });
    if (!api.isDesktop()) terminal.writeln("\x1b[1;32mCNshell 浏览器预览\x1b[0m\r\n\r\n请运行 \x1b[36mnpm run tauri dev\x1b[0m 建立真实 SSH 会话。\r\n");
    return () => { disposed=true;pending.length=0;void unlistenPromise.then((unlisten) => unlisten()); resize.disconnect(); dataDisposable.dispose();cwdHandler.dispose(); terminal.dispose(); terminalRef.current = null; };
  }, [session.id]);
  useEffect(() => { if (active) terminalRef.current?.focus(); }, [active]);
  return <div className={`terminal-instance ${active ? "active" : ""} pane-${pane}`} ref={containerRef} aria-label={`${session.title} 终端`} />;
});

TerminalView.displayName = "TerminalView";
