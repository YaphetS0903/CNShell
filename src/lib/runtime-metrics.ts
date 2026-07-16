import type { TerminalSession, TransferTask } from "../types";

export interface TransferMetric { bytes: number; time: number; speed: number; etaSeconds: number|null }

export const updateTransferMetric=(previous:TransferMetric|undefined,task:TransferTask,now:number):TransferMetric=>{
  if(!previous||task.transferredBytes<previous.bytes)return{bytes:task.transferredBytes,time:now,speed:0,etaSeconds:null};
  const elapsed=Math.max(0.001,(now-previous.time)/1000);const instant=(task.transferredBytes-previous.bytes)/elapsed;const speed=previous.speed>0?previous.speed*0.7+instant*0.3:instant;const remaining=Math.max(0,task.totalBytes-task.transferredBytes);return{bytes:task.transferredBytes,time:now,speed,etaSeconds:speed>0&&task.totalBytes>0?remaining/speed:null};
};

export const virtualWindow=(total:number,scrollTop:number,viewportHeight:number,rowHeight=26,overscan=10)=>{const start=Math.max(0,Math.floor(scrollTop/rowHeight)-overscan);const end=Math.min(total,Math.ceil((scrollTop+viewportHeight)/rowHeight)+overscan);return{start,end,top:start*rowHeight,bottom:Math.max(0,(total-end)*rowHeight)};};

export const appendMonitorSample=(history:number[],value:number,intervalMs:number)=>[...history.slice(-(Math.max(1,Math.floor(300_000/intervalMs))-1)),value];

export interface MonitorHistorySample { timestamp:number; cpu:number; received:number; sent:number; latency:number|null }
export const appendMonitorHistory=(history:MonitorHistorySample[],sample:MonitorHistorySample,intervalMs:number)=>[...history.slice(-(Math.max(1,Math.floor(300_000/intervalMs))-1)),sample];

const MONITOR_FAILURE_REPORT_THRESHOLD=3;
export const shouldReportMonitorPollError=(session:Pick<TerminalSession,"sessionType"|"status">|undefined,consecutiveFailures:number)=>consecutiveFailures>=MONITOR_FAILURE_REPORT_THRESHOLD&&!(session?.sessionType==="mosh"&&session.status==="reconnecting");
