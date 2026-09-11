import { useShallow } from "zustand/react/shallow";
import {
  Ban,
  CheckCircle2,
  DownloadCloud,
  LoaderCircle,
  Pause,
  Play,
  RotateCcw,
  UploadCloud,
} from "lucide-react";
import { useMemo, useState } from "react";
import { api } from "../../lib/api";
import { IconButton } from "../../components/IconButton";
import { useAppStore } from "../../store/app-store";
import { formatBytes } from "../../lib/format";
import { localPathName } from "../../lib/local-path";
import type { TransferStatus } from "../../types";
import { friendlyTransferError } from "../../lib/transfer-errors";
import "./TransferQueue.css";

type TransferFilter="all"|"active"|"failed"|"completed";

export function TransferQueue() {
  const { transfers, transferMetrics, sessions, addTransfer, setError } = useAppStore(
    useShallow((state) => ({
      transfers: state.transfers,
      transferMetrics: state.transferMetrics,
      sessions: state.sessions,
      addTransfer: state.addTransfer,
      setError: state.setError,
    })),
  );
  const[statusFilter,setStatusFilter]=useState<TransferFilter>("all");
  const[sessionFilter,setSessionFilter]=useState("all");
  const sessionOptions=useMemo(()=>{
    const ids=[...new Set(transfers.map((task)=>task.sessionId))];
    return ids.map((sessionId,index)=>({sessionId,label:sessions.find((session)=>session.id===sessionId)?.title??`已结束的连接 ${index+1}`}));
  },[sessions,transfers]);
  const counts=useMemo(()=>({
    all:transfers.length,
    active:transfers.filter((task)=>["queued","running","paused"].includes(task.status)).length,
    failed:transfers.filter((task)=>task.status==="failed").length,
    completed:transfers.filter((task)=>task.status==="completed").length,
  }),[transfers]);
  const visibleTransfers=transfers.filter((task)=>matchesTransferStatus(task.status,statusFilter)&&(sessionFilter==="all"||task.sessionId===sessionFilter));
  return (
    <div className="transfer-queue">
      <div className="transfer-toolbar">
        <div className="transfer-status-filters" aria-label="按状态筛选传输任务">
          {(["all","active","failed","completed"] as TransferFilter[]).map((filter)=><button key={filter} type="button" aria-pressed={statusFilter===filter} onClick={()=>setStatusFilter(filter)}>{transferFilterLabel(filter)}<span>{counts[filter]}</span></button>)}
        </div>
        <label><span>连接</span><select aria-label="按连接筛选传输任务" value={sessionFilter} onChange={(event)=>setSessionFilter(event.target.value)}><option value="all">全部连接</option>{sessionOptions.map((option)=><option key={option.sessionId} value={option.sessionId}>{option.label}</option>)}</select></label>
      </div>
      <div className="transfer-list">
      {visibleTransfers.map((task) => {
        const percent = task.totalBytes
          ? Math.min(100, (task.transferredBytes / task.totalBytes) * 100)
          : 0;
        const metric = transferMetrics[task.id];
        return (
          <article className="transfer-task" key={task.id}>
            <span className={`transfer-icon ${task.status}`}>
              {task.direction === "upload" ? (
                <UploadCloud size={18} />
              ) : (
                <DownloadCloud size={18} />
              )}
            </span>
            <div className="transfer-copy">
              <strong>{localPathName(task.source)}</strong>
              <span className="transfer-session">{sessionOptions.find((option)=>option.sessionId===task.sessionId)?.label}</span>
              <small>
                {task.source} → {task.destination}
              </small>
              <div className="transfer-progress">
                <i style={{ width: `${percent}%` }} />
              </div>
              <small>
                {formatBytes(task.transferredBytes)} /{" "}
                {formatBytes(task.totalBytes)} · {statusLabel(task.status)}
                {task.status === "running" && metric && metric.speed > 0
                  ? ` · ${formatBytes(metric.speed)}/s · 剩余 ${formatDuration(metric.etaSeconds)}`
                  : ""}
              </small>
              {task.error&&<div className="transfer-error"><strong>{friendlyTransferError(task.error)}</strong><details><summary>技术详情</summary><code>{task.error}</code></details></div>}
            </div>
            <div className="transfer-controls">
              {task.status === "running" && (
                <>
                  <IconButton
                    icon={Pause}
                    label="暂停传输"
                    onClick={() =>
                      api
                        .pauseTransfer(task.id)
                        .catch((error) => setError(String(error)))
                    }
                  />
                  <IconButton
                    icon={Ban}
                    label="取消传输"
                    onClick={() =>
                      api
                        .cancelTransfer(task.id)
                        .catch((error) => setError(String(error)))
                    }
                  />
                </>
              )}{" "}
              {task.status === "paused" && (
                <>
                  <IconButton
                    icon={Play}
                    label="继续传输"
                    onClick={() =>
                      api
                        .resumeTransfer(task.id)
                        .catch((error) => setError(String(error)))
                    }
                  />
                  <IconButton
                    icon={Ban}
                    label="取消传输"
                    onClick={() =>
                      api
                        .cancelTransfer(task.id)
                        .catch((error) => setError(String(error)))
                    }
                  />
                </>
              )}{" "}
              {["failed", "cancelled"].includes(task.status) && (
                <IconButton
                  icon={RotateCcw}
                  label="重试传输"
                  onClick={() =>
                    api
                      .retryTransfer(task.id)
                      .then(addTransfer)
                      .catch((error) => setError(String(error)))
                  }
                />
              )}{" "}
              {task.status === "completed" && (
                <CheckCircle2 className="success" size={18} />
              )}{" "}
              {task.status === "queued" && (
                <>
                  <LoaderCircle className="spin" size={18} />
                  <IconButton
                    icon={Ban}
                    label="取消传输"
                    onClick={() =>
                      api
                        .cancelTransfer(task.id)
                        .catch((error) => setError(String(error)))
                    }
                  />
                </>
              )}
            </div>
          </article>
        );
      })}
      {!visibleTransfers.length && (
        <div className="empty-files">
          <CheckCircle2 size={28} />
          <span>{transfers.length?"当前筛选下没有传输任务":"暂无传输任务"}</span>
        </div>
      )}
      </div>
    </div>
  );
}

const matchesTransferStatus=(status:TransferStatus,filter:TransferFilter)=>filter==="all"||(filter==="active"?["queued","running","paused"].includes(status):status===filter);
const transferFilterLabel=(filter:TransferFilter)=>({all:"全部",active:"进行中",failed:"失败",completed:"已完成"})[filter];

const statusLabel = (status: string) =>
  ({
    queued: "等待中",
    running: "传输中",
    paused: "已暂停",
    completed: "已完成",
    failed: "失败",
    cancelled: "已取消",
  })[status] ?? status;
const formatDuration = (seconds: number | null) =>
  seconds === null
    ? "计算中"
    : seconds < 60
      ? `${Math.ceil(seconds)} 秒`
      : seconds < 3600
        ? `${Math.ceil(seconds / 60)} 分钟`
        : `${Math.floor(seconds / 3600)} 小时 ${Math.ceil((seconds % 3600) / 60)} 分钟`;
