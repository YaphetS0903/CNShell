import { Ban, CheckCircle2, DownloadCloud, LoaderCircle, UploadCloud } from "lucide-react";
import { open, save } from "@tauri-apps/plugin-dialog";
import { useEffect, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage, formatBytes } from "../../lib/format";
import type { SerialTransferEvent, TerminalSession } from "../../types";
import "./SerialTransferPanel.css";

type Mode = "xmodem" | "xmodem1k" | "xmodemChecksum" | "ymodem" | "kermit";

export function SerialTransferPanel({
  session,
  onError,
}: {
  session: TerminalSession;
  onError: (message: string) => void;
}) {
  const [mode, setMode] = useState<Mode>("xmodem1k");
  const [transfer, setTransfer] = useState<SerialTransferEvent | null>(null);
  useEffect(() => {
    let disposed = false;
    const unlisten = api.onSerialTransfer((event) => {
      if (event.sessionId === session.id) setTransfer(event);
    });
    return () => {
      disposed = true;
      void unlisten.then((stop) => {
        if (disposed) stop();
      });
    };
  }, [session.id]);
  const running = transfer?.status === "running";
  const start = async (direction: "upload" | "download") => {
    if (!api.isDesktop()) {
      onError("X/Ymodem 传输需要运行 CNshell 桌面版");
      return;
    }
    try {
      let selected: string | string[] | null;
      if (direction === "upload") {
        selected = await open({
          directory: false,
          multiple: mode === "ymodem" || mode === "kermit",
          title: mode === "kermit" ? "选择 Kermit 上传文件" : mode === "ymodem" ? "选择 Ymodem 上传文件" : "选择 Xmodem 上传文件",
        });
      } else if (mode === "ymodem" || mode === "kermit") {
        selected = await open({
          directory: true,
          multiple: false,
          title: mode === "kermit" ? "选择 Kermit 下载目录" : "选择 Ymodem 下载目录",
        });
      } else {
        selected = await save({
          defaultPath: "xmodem-download.bin",
          title: "选择 Xmodem 保存位置",
        });
      }
      if (!selected) return;
      const paths = Array.isArray(selected) ? selected : [selected];
      setTransfer(
        await api.startSerialTransfer(session.id, mode, direction, paths),
      );
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const cancel = async () => {
    if (!transfer) return;
    try {
      setTransfer(await api.cancelSerialTransfer(transfer.id));
    } catch (error) {
      onError(errorMessage(error));
    }
  };
  const percent =
    transfer?.totalBytes && transfer.totalBytes > 0
      ? Math.min(100, (transfer.transferredBytes / transfer.totalBytes) * 100)
      : 0;
  return (
    <section className="serial-transfer-panel" aria-label="Serial 文件传输">
      <header>
        <div>
          <strong>Serial 文件传输</strong>
          <span>{mode === "ymodem" || mode === "kermit" ? "批量与文件名" : "单文件"}</span>
        </div>
        <label>
          <span>协议模式</span>
          <select
            aria-label="协议模式"
            value={mode}
            disabled={running}
            onChange={(event) => setMode(event.target.value as Mode)}
          >
            <option value="xmodem1k">Xmodem 1K + CRC</option>
            <option value="xmodem">Xmodem 128 + CRC</option>
            <option value="xmodemChecksum">Xmodem 128 + Checksum</option>
            <option value="ymodem">Ymodem Batch</option>
            <option value="kermit">Kermit Batch</option>
          </select>
        </label>
      </header>
      <div className="serial-transfer-actions">
        <button
          type="button"
          className="button primary"
          disabled={running}
          onClick={() => void start("upload")}
        >
          <UploadCloud size={16} />上传
        </button>
        <button
          type="button"
          className="button secondary"
          disabled={running}
          onClick={() => void start("download")}
        >
          <DownloadCloud size={16} />下载
        </button>
        {running && (
          <button type="button" className="button secondary" onClick={() => void cancel()}>
            <Ban size={16} />取消
          </button>
        )}
      </div>
      {transfer && (
        <div className={`serial-transfer-status ${transfer.status}`} role="status" aria-live="polite">
          <div className="serial-transfer-state">
            {transfer.status === "running" ? (
              <LoaderCircle className="spin" size={16} />
            ) : transfer.status === "completed" ? (
              <CheckCircle2 size={16} />
            ) : (
              <Ban size={16} />
            )}
            <strong>{statusLabel(transfer.status)}</strong>
            {transfer.fileName && <span title={transfer.fileName}>{transfer.fileName}</span>}
          </div>
          <div className="serial-transfer-progress"><i style={{ width: `${percent}%` }} /></div>
          <small>
            {formatBytes(transfer.transferredBytes)}
            {transfer.totalBytes != null && ` / ${formatBytes(transfer.totalBytes)}`}
            {transfer.error && ` · ${transfer.error}`}
          </small>
        </div>
      )}
    </section>
  );
}

function statusLabel(status: SerialTransferEvent["status"]) {
  if (status === "running") return "传输中";
  if (status === "completed") return "已完成";
  if (status === "cancelled") return "已取消";
  return "失败";
}
