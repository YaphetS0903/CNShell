import { Activity, Columns2, Command, Copy, Files, History, MoreHorizontal, RefreshCw, Search, ServerCog, X } from "lucide-react";
import { createRef, useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { useAppStore } from "../../store/app-store";
import { IconButton } from "../../components/IconButton";
import { TerminalView, type TerminalActions } from "./TerminalView";
import { FileManager } from "../files/FileManager";
import { TransferQueue } from "../files/TransferQueue";
import { SystemInfoPanel } from "../monitor/SystemInfoPanel";
import type { CommandSnippet, ConnectionProfile, TerminalSession } from "../../types";
import { clampPanelSize, resizeFromKeyboard } from "../../lib/layout";
import { RdpWorkspace } from "../rdp/RdpWorkspace";
import "./TerminalWorkspace.css";

export default function TerminalWorkspace({connect}:{connect:(profile:ConnectionProfile)=>Promise<void>}) {
  const { sessions, connections, activeSessionId, activePanel, setActiveSession, updateSession, removeSession, setPanel, settings, setError } = useAppStore();
  const [findOpen, setFindOpen] = useState(false); const [query, setQuery] = useState(""); const [bottomOpen, setBottomOpen] = useState(true);const[bottomHeight,setBottomHeight]=useState(()=>clampPanelSize(Number(localStorage.getItem("cnshell-bottom-height"))||260,210,520));const stackRef=useRef<HTMLDivElement>(null);
  const [splitSessionId,setSplitSessionId]=useState<string|null>(null);const[tabMenu,setTabMenu]=useState<string|null>(null);
  const [refs] = useState(() => new Map<string, React.RefObject<TerminalActions | null>>());
  sessions.forEach((session) => { if (session.sessionType==="terminal"&&!refs.has(session.id)) refs.set(session.id, createRef<TerminalActions>()); });
  const close = useCallback(async (id: string) => { const session=useAppStore.getState().sessions.find((item)=>item.id===id);try { if(session?.sessionType==="rdp")await api.rdpClose(id);else await api.closeTerminal(id); } catch { /* already disconnected */ } removeSession(id); refs.delete(id); },[refs,removeSession]);
  const profileFor=(session:TerminalSession)=>connections.find((item)=>item.id===session.connectionId);
  const duplicate=async(session:TerminalSession)=>{const profile=profileFor(session);if(profile)await connect(profile);};
  const reconnect=async(session:TerminalSession)=>{const profile=profileFor(session);if(!profile)return;await close(session.id);await connect(profile);};
  const split=async(session:TerminalSession)=>{if(session.sessionType==="rdp")return;if(splitSessionId){setSplitSessionId(null);return;}const profile=profileFor(session);if(!profile)return;const before=new Set(useAppStore.getState().sessions.map((item)=>item.id));await connect(profile);const created=useAppStore.getState().sessions.find((item)=>!before.has(item.id));if(created)setSplitSessionId(created.id);};
  useEffect(() => {
    const handler = (event: KeyboardEvent) => {
      if (event.metaKey && event.key.toLowerCase() === "f" && sessions.find((item)=>item.id===activeSessionId)?.sessionType==="terminal") { event.preventDefault(); setFindOpen(true); }
      if (event.metaKey && event.key.toLowerCase() === "j") { event.preventDefault(); setBottomOpen((value) => !value); }
      if (event.metaKey && event.key.toLowerCase() === "k" && activeSessionId) { event.preventDefault(); refs.get(activeSessionId)?.current?.clear(); }
      if (event.metaKey && event.key.toLowerCase() === "w" && activeSessionId) { const session=sessions.find((item)=>item.id===activeSessionId);if(session&&(!settings.confirmCloseActiveSession||confirm(`关闭“${session.title}”会话？`))){event.preventDefault();void close(session.id);} }
      if (event.metaKey && /^[1-9]$/.test(event.key)) { const session = sessions[Number(event.key)-1]; if (session) { event.preventDefault(); setActiveSession(session.id); } }
    }; window.addEventListener("keydown", handler); return () => window.removeEventListener("keydown", handler);
  }, [activeSessionId, close, refs, sessions, setActiveSession, settings.confirmCloseActiveSession]);
  useEffect(()=>{const handler=()=>{const session=useAppStore.getState().sessions.find((item)=>item.id===useAppStore.getState().activeSessionId);if(session&&(!useAppStore.getState().settings.confirmCloseActiveSession||confirm(`关闭“${session.title}”会话？`)))void close(session.id);};window.addEventListener("cnshell-close-session",handler);return()=>window.removeEventListener("cnshell-close-session",handler);},[close]);
  useEffect(()=>{localStorage.setItem("cnshell-bottom-height",String(bottomHeight));},[bottomHeight]);
  useEffect(()=>{const listener=api.onTerminalStatus((status)=>updateSession(status.sessionId,{status:status.status,lastError:status.lastError}));return()=>{void listener.then((unlisten)=>unlisten());};},[updateSession]);
  if (!sessions.length) return <main className="welcome-workspace"><div className="welcome-mark"><Command size={42}/></div><span className="eyebrow">欢迎使用 CNshell</span><h2>从左侧选择一台服务器</h2><p>双击连接即可打开安全的 SSH 终端。文件、传输与监控会自动绑定到当前会话。</p><div className="shortcut-row"><kbd>⌘N</kbd><span>新建连接</span><kbd>⌘T</kbd><span>打开终端</span><kbd>⌘?</kbd><span>查看帮助</span></div></main>;
  const active = sessions.find((item) => item.id === activeSessionId) ?? sessions[0];
  return <main className="workspace">
    <div className="session-tabs" role="tablist" aria-label="打开的会话" onKeyDown={(event)=>moveTabFocus(event,sessions.map((session)=>session.id),active.id,setActiveSession,"session-tab-")}>
      {sessions.map((session) => <div className="session-tab-wrap" key={session.id}><button id={`session-tab-${session.id}`} role="tab" aria-selected={session.id === active.id} aria-controls={`session-panel-${session.id}`} tabIndex={session.id===active.id?0:-1} aria-label={`${session.title}，${sessionStatusLabel(session.status)}${session.lastError?`，${session.lastError}`:""}`} className={`session-tab ${session.id === active.id ? "active" : ""}`} onClick={() => setActiveSession(session.id)}><span className={`status-dot ${session.status}`} aria-hidden="true"/><span>{session.title}</span>{session.status!=="online"&&<small>{sessionStatusLabel(session.status)}</small>}</button><IconButton icon={MoreHorizontal} label={`${session.title} 会话操作`} className="tab-menu-trigger" aria-haspopup="menu" aria-expanded={tabMenu===session.id} onClick={()=>setTabMenu(tabMenu===session.id?null:session.id)}/>{tabMenu===session.id&&<div className="tab-context-menu" role="menu"><button role="menuitem" onClick={()=>void duplicate(session)}><Copy size={13}/>复制会话</button><button role="menuitem" onClick={()=>void reconnect(session)}><RefreshCw size={13}/>重新连接</button>{session.sessionType==="terminal"&&<button role="menuitem" onClick={()=>void split(session)}><Columns2 size={13}/>{splitSessionId?"关闭拆分":"左右拆分"}</button>}<button role="menuitem" className="danger" onClick={()=>{if(!settings.confirmCloseActiveSession||confirm(`关闭“${session.title}”会话？`))void close(session.id);}}><X size={13}/>关闭</button></div>}</div>)}
      <div className="tab-spacer"/>
      {active.sessionType==="terminal"&&<IconButton icon={Search} label="搜索终端" onClick={() => setFindOpen(!findOpen)} active={findOpen}/>}
    </div>
    <div id={`session-panel-${active.id}`} role="tabpanel" aria-labelledby={`session-tab-${active.id}`} className="session-panel">
    {active.sessionType==="rdp"?<RdpWorkspace session={active} onReconnect={()=>void reconnect(active)} onClose={()=>void close(active.id)}/>:<>{findOpen && <form className="terminal-find" onSubmit={(event) => { event.preventDefault(); refs.get(active.id)?.current?.findNext(query); }}><Search size={14}/><input autoFocus value={query} onChange={(event) => { setQuery(event.target.value); refs.get(active.id)?.current?.findNext(event.target.value); }} placeholder="在终端中查找" aria-label="搜索终端输出"/><kbd>Return</kbd><IconButton icon={X} label="关闭搜索" onClick={() => setFindOpen(false)}/></form>}
    <div className={`terminal-stack ${bottomOpen ? "with-bottom" : ""}`} ref={stackRef} style={bottomOpen?{"--bottom-panel-height":`${bottomHeight}px`} as React.CSSProperties:undefined}>
      <div className={`terminal-area ${splitSessionId?"split":""}`}>{sessions.filter((session)=>session.sessionType==="terminal").map((session) => <TerminalView key={session.id} ref={refs.get(session.id)} session={session} active={session.id === active.id||session.id===splitSessionId} pane={session.id===splitSessionId?"secondary":"primary"}/>)}</div>
      {bottomOpen&&<div className="panel-resizer horizontal" role="separator" aria-label="调整底部工具区高度" aria-orientation="horizontal" aria-valuemin={210} aria-valuemax={520} aria-valuenow={bottomHeight} tabIndex={0} onPointerDown={(event)=>{event.currentTarget.setPointerCapture(event.pointerId);const startY=event.clientY;const initial=bottomHeight;const maximum=Math.min(520,(stackRef.current?.clientHeight??730)-180);const move=(moveEvent:PointerEvent)=>setBottomHeight(clampPanelSize(initial+startY-moveEvent.clientY,210,maximum));const stop=()=>{window.removeEventListener("pointermove",move);window.removeEventListener("pointerup",stop);};window.addEventListener("pointermove",move);window.addEventListener("pointerup",stop);}} onKeyDown={(event)=>{const next=resizeFromKeyboard(bottomHeight,event.key,"horizontal");if(next===bottomHeight)return;event.preventDefault();setBottomHeight(clampPanelSize(next,210,Math.min(520,(stackRef.current?.clientHeight??730)-180)));}}/>}
      {bottomOpen && <section className="bottom-panel">
        <nav className="panel-tabs" aria-label="会话工具" role="tablist" onKeyDown={(event)=>moveTabFocus(event,panelOrder,activePanel,setPanel,"tool-tab-")}>
          <button id="tool-tab-files" role="tab" aria-selected={activePanel === "files"} aria-controls="tool-panel" tabIndex={activePanel==="files"?0:-1} className={activePanel === "files" ? "active" : ""} onClick={() => setPanel("files")}><Files size={15}/>文件</button>
          <button id="tool-tab-commands" role="tab" aria-selected={activePanel === "commands"} aria-controls="tool-panel" tabIndex={activePanel==="commands"?0:-1} className={activePanel === "commands" ? "active" : ""} onClick={() => setPanel("commands")}><History size={15}/>快捷命令</button>
          <button id="tool-tab-transfers" role="tab" aria-selected={activePanel === "transfers"} aria-controls="tool-panel" tabIndex={activePanel==="transfers"?0:-1} className={activePanel === "transfers" ? "active" : ""} onClick={() => setPanel("transfers")}><Activity size={15}/>传输</button>
          <button id="tool-tab-system" role="tab" aria-selected={activePanel === "system"} aria-controls="tool-panel" tabIndex={activePanel==="system"?0:-1} className={activePanel === "system" ? "active" : ""} onClick={() => setPanel("system")}><ServerCog size={15}/>系统信息</button>
          <IconButton icon={X} label="折叠工具面板" onClick={() => setBottomOpen(false)} />
        </nav>
        <div id="tool-panel" role="tabpanel" aria-labelledby={`tool-tab-${activePanel}`} className="panel-content">{activePanel === "files" && <FileManager session={active}/>} {activePanel === "commands" && <CommandPanel session={active} onError={setError}/>} {activePanel === "transfers" && <TransferQueue/>} {activePanel === "system" && <SystemInfoPanel sessionId={active.id}/>}</div>
      </section>}
    </div></>}
    </div>
  </main>;
}

const sessionStatusLabel=(status:TerminalSession["status"])=>({connecting:"连接中",online:"在线",reconnecting:"重连中",failed:"失败",closed:"已关闭"}[status]);
const panelOrder=["files","commands","transfers","system"] as const;

function moveTabFocus<T extends string>(event:React.KeyboardEvent,ids:readonly T[],active:T,select:(id:T)=>void,idPrefix:string){
  if((event.target as HTMLElement).getAttribute("role")!=="tab")return;
  if(!["ArrowLeft","ArrowRight","Home","End"].includes(event.key))return;
  event.preventDefault();
  const current=Math.max(0,ids.indexOf(active));
  const next=event.key==="Home"?0:event.key==="End"?ids.length-1:event.key==="ArrowRight"?(current+1)%ids.length:(current-1+ids.length)%ids.length;
  const id=ids[next];
  if(!id)return;
  select(id);
  requestAnimationFrame(()=>document.getElementById(`${idPrefix}${id}`)?.focus());
}

const builtInSnippets:CommandSnippet[] = [
  {id:"system",name:"系统概览",command:"uname -a && uptime",description:"",tags:[],sortOrder:0,builtIn:true},
  {id:"disk",name:"磁盘使用",command:"df -h",description:"",tags:[],sortOrder:1,builtIn:true},
  {id:"memory",name:"内存排行",command:"ps aux --sort=-%mem | head",description:"",tags:[],sortOrder:2,builtIn:true}
];

function CommandPanel({ session, onError }: { session: TerminalSession; onError: (message: string|null) => void }) {
  const [snippets,setSnippets]=useState<CommandSnippet[]>([]);
  const [history,setHistory]=useState<string[]>([]);
  const [draft,setDraft]=useState("");
  const load=useCallback(()=>{
    void api.listSnippets().then((items)=>setSnippets([...builtInSnippets,...items]));
    void api.listHistory(session.connectionId).then(setHistory);
  },[session.connectionId]);
  useEffect(()=>{load();},[load]);
  const run=async(command:string)=>{try{await api.terminalInput(session.id,`${command}\n`);await api.addHistory(session.connectionId,command);setDraft("");setHistory(await api.listHistory(session.connectionId));}catch(error){onError(String(error));}};
  const add=async()=>{const name=prompt("快捷命令名称");if(!name||!draft.trim())return;try{await api.saveSnippet({id:crypto.randomUUID(),name,command:draft.trim(),description:"",tags:[],sortOrder:snippets.length});load();}catch(error){onError(String(error));}};
  const remove=async(id:string,name:string)=>{if(!confirm(`删除快捷命令“${name}”？`))return;try{await api.deleteSnippet(id);load();}catch(error){onError(String(error));}};
  return <div className="commands-panel"><form className="command-entry" onSubmit={(event)=>{event.preventDefault();void run(draft);}}><Command size={15}/><input value={draft} onChange={(event)=>setDraft(event.target.value)} placeholder="输入命令，Return 执行"/><button type="button" className="mini-button" disabled={!draft.trim()} onClick={add}>保存为快捷命令</button><button className="button primary" disabled={!draft.trim()}>执行</button></form><div className="command-grid">{snippets.map((item) => <div key={item.id} className="command-card"><button onClick={() => void run(item.command)}><Command size={16}/><strong>{item.name}</strong><code>{item.command}</code></button>{!item.builtIn&&<IconButton icon={X} label={`删除快捷命令 ${item.name}`} onClick={()=>void remove(item.id,item.name)}/>}</div>)}</div>{history.length>0&&<div className="history-list"><h3>最近命令</h3>{history.slice(0,10).map((command,index)=><button key={`${index}-${command}`} onClick={()=>setDraft(command)}><History size={13}/><code>{command}</code></button>)}</div>}</div>;
}
