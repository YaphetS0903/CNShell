import { formatBytes } from "../../lib/format";
import type { DiskInfo } from "../../types";

export function DiskItem({disk}:{disk:DiskInfo}) {
  return <div className="disk-item" aria-label={`${disk.mountPoint}，已用 ${formatBytes(disk.usedBytes)}，总量 ${formatBytes(disk.totalBytes)}，剩余 ${formatBytes(disk.availableBytes)}`}><div className="disk-heading"><span title={disk.mountPoint}>{disk.mountPoint}</span><code>{disk.usedPercent.toFixed(0)}%</code></div><div className="mini-progress"><i style={{width:`${disk.usedPercent}%`}}/></div><small>已用 {formatBytes(disk.usedBytes)} / {formatBytes(disk.totalBytes)} · 剩余 {formatBytes(disk.availableBytes)}</small></div>;
}
