import type { DiskInfo, NetworkInfo, TerminalSession, TransferTask } from "../types";

export interface TransferMetric { bytes: number; time: number; speed: number; etaSeconds: number|null }

export const updateTransferMetric=(previous:TransferMetric|undefined,task:TransferTask,now:number):TransferMetric=>{
  if(!previous||task.transferredBytes<previous.bytes)return{bytes:task.transferredBytes,time:now,speed:0,etaSeconds:null};
  const elapsed=Math.max(0.001,(now-previous.time)/1000);const instant=(task.transferredBytes-previous.bytes)/elapsed;const speed=previous.speed>0?previous.speed*0.7+instant*0.3:instant;const remaining=Math.max(0,task.totalBytes-task.transferredBytes);return{bytes:task.transferredBytes,time:now,speed,etaSeconds:speed>0&&task.totalBytes>0?remaining/speed:null};
};

export const virtualWindow=(total:number,scrollTop:number,viewportHeight:number,rowHeight=26,overscan=10)=>{const start=Math.max(0,Math.floor(scrollTop/rowHeight)-overscan);const end=Math.min(total,Math.ceil((scrollTop+viewportHeight)/rowHeight)+overscan);return{start,end,top:start*rowHeight,bottom:Math.max(0,(total-end)*rowHeight)};};

export const appendMonitorSample=(history:number[],value:number,intervalMs:number)=>[...history.slice(-(Math.max(1,Math.floor(300_000/intervalMs))-1)),value];

export interface MonitorHistorySample { timestamp:number; cpu:number; received:number; sent:number; latency:number|null }
export const appendMonitorHistory=(history:MonitorHistorySample[],sample:MonitorHistorySample,intervalMs:number)=>[...history.slice(-(Math.max(1,Math.floor(300_000/intervalMs))-1)),sample];

const LOOPBACK_NETWORK_PATTERN=/^(lo|lo\d+|loopback)$/i;
const VIRTUAL_NETWORK_PATTERN=/^(docker\d*|veth|br-|virbr|vmnet|vboxnet|utun|tailscale|zt|wg\d*)/i;

export const isLoopbackNetwork=(name:string)=>LOOPBACK_NETWORK_PATTERN.test(name);
export const isVirtualNetwork=(name:string)=>VIRTUAL_NETWORK_PATTERN.test(name);

export const selectMonitorNetwork=(networks:NetworkInfo[],preferredName?:string|null)=>{
  if(preferredName){
    const preferred=networks.find((network)=>network.interfaceName===preferredName);
    if(preferred)return preferred;
  }
  const candidates=networks.filter((network)=>!isLoopbackNetwork(network.interfaceName));
  return [...(candidates.length?candidates:networks)].sort((left,right)=>{
    const virtualDifference=Number(isVirtualNetwork(left.interfaceName))-Number(isVirtualNetwork(right.interfaceName));
    if(virtualDifference)return virtualDifference;
    const trafficDifference=(right.rxTotalBytes+right.txTotalBytes)-(left.rxTotalBytes+left.txTotalBytes);
    if(trafficDifference)return trafficDifference;
    return left.interfaceName.localeCompare(right.interfaceName);
  })[0];
};

export const monitorHistoryKey=(sessionId:string,interfaceName?:string)=>`${sessionId}\u0000${interfaceName??"no-network"}`;

const TEMPORARY_FILESYSTEM_PATTERN = /^(?:tmpfs|devtmpfs|overlay|shm|proc|procfs|sysfs|cgroup2?|debugfs|tracefs|securityfs|pstore|efivarfs|mqueue|hugetlbfs|fusectl|configfs|ramfs|squashfs|nsfs|autofs)$/i;
const TEMPORARY_MOUNT_PATTERN = /^\/(?:proc|sys|dev|run)(?:\/|$)/;

export const isTemporaryDisk = (disk: Pick<DiskInfo, "filesystem" | "mountPoint">) =>
  disk.mountPoint !== "/" &&
  (TEMPORARY_FILESYSTEM_PATTERN.test(disk.filesystem) ||
    TEMPORARY_MOUNT_PATTERN.test(disk.mountPoint));

const MONITOR_FAILURE_REPORT_THRESHOLD=3;
export const shouldReportMonitorPollError=(session:Pick<TerminalSession,"sessionType"|"status">|undefined,consecutiveFailures:number)=>consecutiveFailures>=MONITOR_FAILURE_REPORT_THRESHOLD&&!(session?.sessionType==="mosh"&&session.status==="reconnecting");
