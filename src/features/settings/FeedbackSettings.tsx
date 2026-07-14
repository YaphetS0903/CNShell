import { Bug, ExternalLink, FileJson, FolderOpen, Lightbulb, MessageSquareText, ShieldCheck } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { save } from "@tauri-apps/plugin-dialog";
import { api, type FeedbackEnvironment } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { bugReportUrl, featureRequestUrl, latestReleaseUrl } from "./feedback-links";

export function FeedbackSettings({onError}:{onError:(message:string)=>void}) {
  const[environment,setEnvironment]=useState<FeedbackEnvironment|null>(null);
  const[lastDiagnosticPath,setLastDiagnosticPath]=useState<string|null>(null);
  const[status,setStatus]=useState("正在读取运行环境…");
  useEffect(()=>{void api.feedbackEnvironment().then((value)=>{setEnvironment(value);setStatus("环境信息已就绪");}).catch((error)=>{setStatus("环境信息暂不可用");onError(errorMessage(error));});},[onError]);
  const environmentLabel=useMemo(()=>environment?`CNshell ${environment.appVersion} · macOS ${environment.osVersion} · ${environment.architecture}`:status,[environment,status]);
  const openBug=()=>api.openExternal(bugReportUrl(environment)).catch((error)=>onError(errorMessage(error)));
  const openFeature=()=>api.openExternal(featureRequestUrl).catch((error)=>onError(errorMessage(error)));
  const openRelease=()=>api.openExternal(latestReleaseUrl).catch((error)=>onError(errorMessage(error)));
  const exportDiagnostics=async()=>{
    if(!api.isDesktop()){onError("诊断导出需要运行桌面版");return;}
    const path=await save({defaultPath:"CNshell-diagnostics.json",filters:[{name:"JSON",extensions:["json"]}]});
    if(!path)return;
    try{await api.exportDiagnostics(path);setLastDiagnosticPath(path);setStatus("脱敏诊断已导出");}catch(error){onError(errorMessage(error));}
  };
  const revealDiagnostics=()=>lastDiagnosticPath&&api.revealDiagnostics(lastDiagnosticPath).catch((error)=>onError(errorMessage(error)));
  return <section className="feedback-settings"><div className="section-heading"><div><h3><MessageSquareText size={16}/>反馈与诊断</h3><p>{environmentLabel}</p></div></div><div className="feedback-actions"><button className="button primary" onClick={()=>void openBug()}><Bug size={14}/>报告问题</button><button className="button secondary" onClick={()=>void openFeature()}><Lightbulb size={14}/>功能建议</button><button className="button secondary" onClick={()=>void openRelease()}><ExternalLink size={14}/>当前版本</button></div><div className="diagnostic-actions"><button className="button secondary" onClick={()=>void exportDiagnostics()}><FileJson size={14}/>导出脱敏诊断</button><button className="button secondary" disabled={!lastDiagnosticPath} onClick={()=>void revealDiagnostics()}><FolderOpen size={14}/>在 Finder 中显示</button><span role="status">{status}</span></div><p className="feedback-privacy"><ShieldCheck size={14}/>诊断包不包含主机、用户名、路径或命令，也不会自动上传。</p></section>;
}
