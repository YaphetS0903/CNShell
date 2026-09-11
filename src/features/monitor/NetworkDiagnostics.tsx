import { Activity,LoaderCircle,Network,RefreshCw,Route,Search,Send,Square } from "lucide-react";
import { useCallback,useEffect,useMemo,useState } from "react";
import { api } from "../../lib/api";
import { waitForTask } from "../../lib/background-task";
import { errorMessage } from "../../lib/format";
import type { BackgroundTask,NetworkDiagnosticResult,NetworkSocketReport } from "../../types";
import "./MonitorTools.css";
import "./NetworkDiagnostics.css";

export function NetworkDiagnostics({sessionId,onError}:{sessionId:string;onError:(message:string)=>void}){
  const[sockets,setSockets]=useState<NetworkSocketReport|null>(null);
  const[loading,setLoading]=useState(true);
  const[target,setTarget]=useState("");
  const[task,setTask]=useState<BackgroundTask|null>(null);
  const[result,setResult]=useState<NetworkDiagnosticResult|null>(null);
  const[pingHistory,setPingHistory]=useState<{time:number;latency:number}[]>([]);
  const[socketQuery,setSocketQuery]=useState("");
  const[stateFilter,setStateFilter]=useState("all");
  const[visibleLimit,setVisibleLimit]=useState(200);
  const load=useCallback(()=>{setLoading(true);api.networkSockets(sessionId).then(setSockets).catch((error)=>onError(errorMessage(error))).finally(()=>setLoading(false));},[onError,sessionId]);
  useEffect(()=>{void load();},[load]);
  const filteredSockets=useMemo(()=>{
    const query=socketQuery.trim().toLocaleLowerCase("zh-CN");
    return(sockets?.items??[]).filter((socket)=>(stateFilter==="all"||socket.state===stateFilter)&&(!query||`${socket.protocol} ${socket.state} ${socket.localAddress} ${socket.peerAddress} ${socket.process}`.toLocaleLowerCase("zh-CN").includes(query)));
  },[socketQuery,sockets,stateFilter]);
  useEffect(()=>setVisibleLimit(200),[sessionId,socketQuery,stateFilter]);
  const displayedSockets=filteredSockets.slice(0,visibleLimit);
  const socketStates=useMemo(()=>[...new Set((sockets?.items??[]).map((socket)=>socket.state))].sort(),[sockets]);
  const run=async(kind:"ping"|"traceroute")=>{if(!target.trim())return;try{const started=await api.startNetworkDiagnostic(sessionId,kind,target.trim());setTask(started);const value=await waitForTask(started) as NetworkDiagnosticResult;setResult(value);if(kind==="ping"){const latency=parseAverageLatency(value.output);if(latency!=null)setPingHistory((current)=>[...current,{time:Date.now(),latency}].slice(-20));}}catch(error){if((error as DOMException).name!=="AbortError")onError(errorMessage(error));}finally{setTask(null);}};
  return <section className="network-diagnostics">
    <div className="section-heading"><div><h3><Network size={15}/>端口与连接</h3><p>显示 {displayedSockets.length}/{filteredSockets.length} 条，共 {sockets?.items.length??0} 条</p></div><button className="mini-button" onClick={()=>void load()} disabled={loading}><RefreshCw size={12}/>刷新</button></div>
    <div className="diagnostic-runner"><form onSubmit={(event)=>{event.preventDefault();void run("ping");}}><input value={target} onChange={(event)=>setTarget(event.target.value)} placeholder="主机名或 IP 地址" aria-label="网络诊断目标"/><button className="button secondary" disabled={!target.trim()||Boolean(task)}><Activity size={14}/>Ping</button><button type="button" className="button secondary" disabled={!target.trim()||Boolean(task)} onClick={()=>void run("traceroute")}><Route size={14}/>Trace</button>{task&&<button type="button" className="button secondary" onClick={()=>void api.cancelTask(task.id)}><Square size={12}/>取消</button>}</form>{pingHistory.length>1&&<PingTrend samples={pingHistory}/>} {result&&<div className="diagnostic-output"><header><strong>{result.kind==="ping"?"Ping":"Traceroute"} · {result.target}</strong><span>{result.durationMs} ms</span></header><pre>{result.output}</pre></div>}</div>
    <div className="socket-filters"><label><Search size={13}/><input value={socketQuery} onChange={(event)=>setSocketQuery(event.target.value)} placeholder="搜索端口、地址或进程" aria-label="搜索端口与连接"/></label><select aria-label="连接状态筛选" value={stateFilter} onChange={(event)=>setStateFilter(event.target.value)}><option value="all">全部状态</option>{socketStates.map((state)=><option key={state} value={state}>{socketStateLabel(state)}</option>)}</select></div>
    {sockets?.warning&&<div className="inline-warning">{sockets.warning}</div>}
    {loading?<div className="loading-state"><LoaderCircle className="spin"/>读取远端连接…</div>:filteredSockets.length?<div className="socket-table"><header><span>协议</span><span>状态</span><span>本地地址</span><span>对端地址</span><span>进程</span></header>{displayedSockets.map((socket,index)=><div key={`${index}-${socket.protocol}-${socket.localAddress}`}><code>{socket.protocol}</code><span>{socketStateLabel(socket.state)}</span><code>{socket.localAddress}</code><code>{socket.peerAddress}</code><span title={socket.process}>{socket.process||"—"}</span></div>)}</div>:<p className="socket-empty">当前筛选下没有连接</p>}
    {!loading&&displayedSockets.length<filteredSockets.length&&<div className="socket-list-more"><button className="button secondary" onClick={()=>setVisibleLimit((current)=>current+200)}>继续显示 200 条（剩余 {filteredSockets.length-displayedSockets.length} 条）</button></div>}
  </section>;
}

function socketStateLabel(state:string){return({LISTEN:"监听",ESTAB:"已建立",ESTABLISHED:"已建立",TIME_WAIT:"等待关闭",CLOSE_WAIT:"等待应用关闭"} as Record<string,string>)[state]??state;}
function parseAverageLatency(output:string):number|null{const match=output.match(/=\s*[\d.]+\/([\d.]+)\/[\d.]+/);return match?Number(match[1]):null;}
function PingTrend({samples}:{samples:{time:number;latency:number}[]}){const max=Math.max(1,...samples.map((sample)=>sample.latency));const points=samples.map((sample,index)=>`${index/(samples.length-1)*100},${28-sample.latency/max*24}`).join(" ");return <div className="ping-trend"><span><Send size={12}/>Ping 历史 · {samples.at(-1)?.latency.toFixed(1)} ms</span><svg viewBox="0 0 100 30" preserveAspectRatio="none" role="img" aria-label="Ping 延迟历史趋势"><polyline points={points}/></svg></div>;}
