import { DownloadCloud, RefreshCw } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { check, type DownloadEvent, type Update } from "@tauri-apps/plugin-updater";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";

type UpdateState =
  | { status: "idle"; message: string }
  | { status: "checking"; message: string }
  | { status: "available"; message: string; update: Update }
  | { status: "downloading"; message: string; update: Update; received: number; total: number | null }
  | { status: "current"; message: string }
  | { status: "error"; message: string };

const candidateChannelMessage = "当前候选版未配置正式更新通道。正式发行包会从签名 HTTPS endpoint 检查更新。";

export function UpdateSettings({onError}:{onError:(message:string)=>void}) {
  const [state,setState]=useState<UpdateState>({status:"idle",message:candidateChannelMessage});
  const updateRef=useRef<Update|null>(null);
  useEffect(()=>()=>{void updateRef.current?.close();},[]);

  const checkNow=async()=>{
    if(!api.isDesktop()){setState({status:"error",message:"更新检查仅在 CNshell 桌面版中可用。"});return;}
    setState({status:"checking",message:"正在安全检查更新…"});
    try{
      await updateRef.current?.close();updateRef.current=null;
      const update=await check({timeout:15_000});
      if(!update){setState({status:"current",message:"当前已是最新版本。"});return;}
      updateRef.current=update;
      setState({status:"available",update,message:`发现 CNshell ${update.version}（当前 ${update.currentVersion}）`});
    }catch(reason){
      const message=errorMessage(reason);
      if(message.toLowerCase().includes("endpoints"))setState({status:"idle",message:candidateChannelMessage});
      else{setState({status:"error",message:`更新检查失败：${message}`});onError(`更新检查失败：${message}`);}
    }
  };

  const install=async(update:Update)=>{
    if(!confirm(`下载并安装 CNshell ${update.version}？安装完成后请重新启动应用。`))return;
    let received=0,total:number|null=null;
    setState({status:"downloading",update,message:"正在下载并验证签名更新…",received,total});
    try{
      await update.downloadAndInstall((event:DownloadEvent)=>{
        if(event.event==="Started")total=event.data.contentLength??null;
        if(event.event==="Progress")received+=event.data.chunkLength;
        setState({status:"downloading",update,message:"正在下载并验证签名更新…",received,total});
      },{timeout:120_000});
      updateRef.current=null;
      setState({status:"current",message:"更新已安装。请退出并重新启动 CNshell。"});
    }catch(reason){
      const message=`更新安装失败，当前版本保持可用：${errorMessage(reason)}`;
      setState({status:"error",message});onError(message);
    }
  };

  const busy=state.status==="checking"||state.status==="downloading";
  return <section><h3><DownloadCloud size={16}/>软件更新</h3><p className="muted-copy" role="status">{state.message}</p>{state.status==="available"&&state.update.body&&<details><summary>查看发布说明</summary><pre className="release-notes">{state.update.body}</pre></details>}{state.status==="downloading"&&(state.total?<progress aria-label="更新下载进度" max={state.total} value={state.received}>{state.received}</progress>:<progress aria-label="更新下载进度"/>)}<div className="backup-actions"><button className="button secondary" disabled={busy} onClick={()=>void checkNow()}><RefreshCw size={14} className={state.status==="checking"?"spin":undefined}/>检查更新</button>{state.status==="available"&&<button className="button primary" onClick={()=>void install(state.update)}><DownloadCloud size={14}/>下载并安装 {state.update.version}</button>}</div></section>;
}
