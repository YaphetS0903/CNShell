import { Search,X } from "lucide-react";
import { useMemo,useState } from "react";
import { Modal } from "../../components/Modal";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import type { TerminalSession } from "../../types";

export function GlobalTerminalSearch({sessions,onSelect,onClose}:{sessions:TerminalSession[];onSelect:(sessionId:string,line:number)=>void;onClose:()=>void}){const[query,setQuery]=useState("");const matches=useMemo(()=>query.trim()?sessions.flatMap((session)=>workspaceRuntime.terminalSearchBySession.get(session.id)?.(query)??[]):[],[query,sessions]);return <Modal title="跨标签终端搜索" onClose={onClose} wide><div className="global-terminal-search"><label><Search size={15}/><input autoFocus value={query} onChange={(event)=>setQuery(event.target.value)} placeholder="搜索全部打开终端的滚屏内容" aria-label="全局终端搜索"/><span>{matches.length}</span></label><div>{matches.map((match,index)=>{const session=sessions.find((item)=>item.id===match.sessionId);return <button key={`${match.sessionId}-${match.line}-${index}`} onClick={()=>onSelect(match.sessionId,match.line)}><strong>{session?.title??match.sessionId}</strong><code>{match.preview||"空行"}</code><small>第 {match.line+1} 行</small></button>;})}{query&&!matches.length&&<p><X size={18}/>没有匹配内容</p>}</div></div></Modal>;}
