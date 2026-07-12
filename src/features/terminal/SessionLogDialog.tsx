import { AlertTriangle, Download, FileClock, Square } from "lucide-react";
import { save } from "@tauri-apps/plugin-dialog";
import { useCallback, useEffect, useState } from "react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { formatBytes } from "../../lib/format";
import type { SessionLogStatus, TerminalSession } from "../../types";

type LogPreferences={format:"text"|"jsonl";lineTimestamps:boolean;retentionDays:number;maxTotalMb:number};
const preferenceKey="cnshell-session-log-preferences";
const defaults:LogPreferences={format:"text",lineTimestamps:false,retentionDays:30,maxTotalMb:500};

export function SessionLogDialog({session,onClose,onError}:{session:TerminalSession;onClose:()=>void;onError:(message:string)=>void}){
  const[preferences,setPreferences]=useState<LogPreferences>(()=>{try{return{...defaults,...JSON.parse(localStorage.getItem(preferenceKey)??"null")};}catch{return defaults;}});
  const[status,setStatus]=useState<SessionLogStatus|null>(null);const[busy,setBusy]=useState(false);
  const load=useCallback(()=>api.sessionLogStatus(session.id).then(setStatus).catch((error)=>onError(String(error))),[onError,session.id]);
  useEffect(()=>{void load();},[load]);
  useEffect(()=>{if(!status?.active)return;const timer=window.setInterval(()=>void load(),1000);return()=>window.clearInterval(timer);},[load,status?.active]);
  const start=async()=>{setBusy(true);try{localStorage.setItem(preferenceKey,JSON.stringify(preferences));setStatus(await api.startSessionLog(session.id,preferences.format,preferences.lineTimestamps,preferences.retentionDays,preferences.maxTotalMb*1024*1024));}catch(error){onError(String(error));}finally{setBusy(false);}};
  const stop=async()=>{setBusy(true);try{setStatus(await api.stopSessionLog(session.id));}catch(error){onError(String(error));}finally{setBusy(false);}};
  const exportLog=async()=>{if(!status?.path)return;const extension=status.format==="jsonl"?"jsonl":"log";const destination=await save({title:"导出会话日志",defaultPath:`${session.title}.${extension}`,filters:[{name:status.format==="jsonl"?"JSON Lines":"文本日志",extensions:[extension]}]});if(!destination)return;setBusy(true);try{await api.exportSessionLog(session.id,destination);}catch(error){onError(String(error));}finally{setBusy(false);}};
  return <Modal title={`${session.title} · 会话日志`} onClose={onClose}>
    <div className="session-log-dialog">
      <p className="log-warning"><AlertTriangle size={16}/>日志可能包含命令、主机信息、令牌或其他秘密。仅在需要时启用，并妥善保管导出文件。</p>
      {status?.active?<div className="log-active"><FileClock size={22}/><div><strong>正在记录</strong><span>{status.format==="jsonl"?"JSONL":"纯文本"}{status.lineTimestamps?" · 逐行时间戳":""} · {formatBytes(status.bytesWritten)}</span><code title={status.path??""}>{status.path}</code></div></div>:<div className="log-settings">
        <label><span>日志格式</span><select value={preferences.format} onChange={(event)=>setPreferences((current)=>({...current,format:event.target.value as LogPreferences["format"]}))}><option value="text">纯文本</option><option value="jsonl">JSON Lines</option></select></label>
        <label className="check-row"><input type="checkbox" checked={preferences.lineTimestamps} disabled={preferences.format==="jsonl"} onChange={(event)=>setPreferences((current)=>({...current,lineTimestamps:event.target.checked}))}/><span>纯文本逐行添加时间戳</span></label>
        <div className="log-number-grid"><label><span>自动清理</span><input type="number" min={1} max={3650} value={preferences.retentionDays} onChange={(event)=>setPreferences((current)=>({...current,retentionDays:Number(event.target.value)}))}/><small>天</small></label><label><span>总容量上限</span><input type="number" min={10} max={20480} value={preferences.maxTotalMb} onChange={(event)=>setPreferences((current)=>({...current,maxTotalMb:Number(event.target.value)}))}/><small>MB</small></label></div>
        {status?.path&&<div className="log-last"><span>本次会话最近日志</span><code>{status.path}</code><small>{formatBytes(status.bytesWritten)}{status.error?` · ${status.error}`:""}</small></div>}
      </div>}
      {status?.error&&<div className="inline-error">{status.error}</div>}
      <footer className="form-actions"><button className="button secondary" onClick={()=>void exportLog()} disabled={busy||!status?.path}><Download size={14}/>导出</button>{status?.active?<button className="button primary" onClick={()=>void stop()} disabled={busy}><Square size={13}/>停止记录</button>:<button className="button primary" onClick={()=>void start()} disabled={busy}><FileClock size={14}/>开始记录</button>}</footer>
    </div>
  </Modal>;
}
