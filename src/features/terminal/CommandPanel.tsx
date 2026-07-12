import { Command, History, X } from "lucide-react";
import { useCallback, useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import type { CommandSnippet, TerminalSession } from "../../types";
import { CommandTemplateDialog } from "./CommandTemplateDialog";
import { SmartCommandEntry } from "./SmartCommandEntry";
import { isHighRiskCommand, templateParameters } from "./smart-command";

const builtInSnippets:CommandSnippet[] = [
  {id:"system",name:"系统概览",command:"uname -a && uptime",description:"",tags:[],sortOrder:0,builtIn:true},
  {id:"disk",name:"磁盘使用",command:"df -h",description:"",tags:[],sortOrder:1,builtIn:true},
  {id:"memory",name:"内存排行",command:"ps aux --sort=-%mem | head",description:"",tags:[],sortOrder:2,builtIn:true}
];

export function CommandPanel({ session, onError }: { session: TerminalSession; onError: (message: string|null) => void }) {
  const [snippets,setSnippets]=useState<CommandSnippet[]>([]);
  const [history,setHistory]=useState<string[]>([]);
  const [draft,setDraft]=useState("");
  const [pendingTemplate,setPendingTemplate]=useState<string|null>(null);
  const load=useCallback(()=>{
    void api.listSnippets().then((items)=>setSnippets([...builtInSnippets,...items]));
    void api.listHistory(session.connectionId).then(setHistory);
  },[session.connectionId]);
  useEffect(()=>{load();},[load]);
  const execute=async(command:string)=>{if(isHighRiskCommand(command)&&!confirm(`这是高风险命令，请核对后确认执行：\n\n${command}`))return;try{await api.terminalInput(session.id,`${command}\n`);await api.addHistory(session.connectionId,command);setDraft("");setHistory(await api.listHistory(session.connectionId));}catch(error){onError(String(error));}};
  const run=(command:string)=>{const normalized=command.trim();if(!normalized)return;if(templateParameters(normalized).length){setPendingTemplate(normalized);return;}void execute(normalized);};
  const add=async()=>{const name=prompt("快捷命令名称");if(!name||!draft.trim())return;try{await api.saveSnippet({id:crypto.randomUUID(),name,command:draft.trim(),description:"",tags:[],sortOrder:snippets.length});setDraft("");load();}catch(error){onError(String(error));}};
  const remove=async(id:string,name:string)=>{if(!confirm(`删除快捷命令“${name}”？`))return;try{await api.deleteSnippet(id);load();}catch(error){onError(String(error));}};
  return <div className="commands-panel"><SmartCommandEntry session={session} snippets={snippets} history={history} draft={draft} setDraft={setDraft} onRun={run} onSave={add}/><div className="command-grid">{snippets.map((item) => <div key={item.id} className="command-card"><button onClick={() => run(item.command)}><Command size={16}/><strong>{item.name}</strong><code>{item.command}</code></button>{!item.builtIn&&<IconButton icon={X} label={`删除快捷命令 ${item.name}`} onClick={()=>void remove(item.id,item.name)}/>}</div>)}</div>{history.length>0&&<div className="history-list"><h3>最近命令</h3>{history.slice(0,10).map((command,index)=><button key={`${index}-${command}`} onClick={()=>setDraft(command)}><History size={13}/><code>{command}</code></button>)}</div>}{pendingTemplate&&<CommandTemplateDialog template={pendingTemplate} onClose={()=>setPendingTemplate(null)} onRun={(command)=>{setPendingTemplate(null);void execute(command);}}/>}</div>;
}
