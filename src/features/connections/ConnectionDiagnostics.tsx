import { CheckCircle2, CircleX, LoaderCircle, ShieldCheck } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { waitForTask } from "../../lib/background-task";
import type { ConnectionDiagnostic, ConnectionProfile } from "../../types";

export function ConnectionDiagnostics({connection,onClose,onError}:{connection:ConnectionProfile;onClose:()=>void;onError:(message:string)=>void}){
  const[items,setItems]=useState<ConnectionDiagnostic[]>([]);const[loading,setLoading]=useState(true);const[taskId,setTaskId]=useState<string|null>(null);const activeTask=useRef<string|null>(null);
  const run=useCallback(()=>{if(activeTask.current)void api.cancelTask(activeTask.current);setLoading(true);void api.startConnectionTest(connection.id).then(async(task)=>{activeTask.current=task.id;setTaskId(task.id);try{const result=await waitForTask(task);if(activeTask.current===task.id)setItems(result as ConnectionDiagnostic[]);}catch(error){if(activeTask.current===task.id&&(error as DOMException).name!=="AbortError")onError(errorMessage(error));}finally{if(activeTask.current===task.id){activeTask.current=null;setLoading(false);setTaskId(null);}}}).catch((error)=>{setLoading(false);onError(errorMessage(error));});},[connection.id,onError]);useEffect(()=>{run();return()=>{if(activeTask.current)void api.cancelTask(activeTask.current);};},[run]);
  const unknown=items.find((item)=>item.stage==="hostKey"&&!item.ok&&item.fingerprint);
  const trust=async()=>{if(!unknown?.fingerprint||!unknown.algorithm)return;try{await api.trustHost(connection.id,unknown.fingerprint,unknown.algorithm);run();}catch(error){onError(errorMessage(error));}};
  const close=()=>{if(taskId)void api.cancelTask(taskId);onClose();};
  return <Modal title={`${connection.name} · 连接诊断`} onClose={close}><div className="diagnostic-list">{loading&&<div className="loading-state"><LoaderCircle className="spin"/>正在检查 DNS、TCP、指纹与认证…</div>}{items.map((item,index)=><div className={`diagnostic-row ${item.ok?"ok":"failed"}`} key={`${item.stage}-${index}`}>{item.ok?<CheckCircle2 size={17}/>:<CircleX size={17}/>}<div><strong>{stageLabel(item.stage)}</strong><span>{item.message}</span>{item.fingerprint&&<code>{item.fingerprint}</code>}</div>{item.latencyMs!=null&&<small>{item.latencyMs} ms</small>}</div>)}{unknown&&<div className="diagnostic-trust"><ShieldCheck size={18}/><p>请先从服务器控制台核对指纹。确认一致后才能写入信任记录。</p><button className="button primary" onClick={trust}>已核对，信任此指纹</button></div>}<footer className="form-actions">{loading&&<button className="button secondary" onClick={()=>taskId&&void api.cancelTask(taskId)}>取消检测</button>}<button className="button secondary" onClick={run} disabled={loading}>重新检测</button><button className="button primary" onClick={close}>完成</button></footer></div></Modal>;
}
const stageLabel=(stage:ConnectionDiagnostic["stage"])=>({dns:"DNS",tcp:"TCP / SSH",proxy:"代理 / 跳板机",hostKey:"主机指纹",authentication:"认证",shell:"Shell",complete:"完成"}[stage]);
