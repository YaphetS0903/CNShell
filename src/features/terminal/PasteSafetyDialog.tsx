import { AlertTriangle, Clipboard, Trash2 } from "lucide-react";
import { Modal } from "../../components/Modal";
import { pasteRisk } from "./terminal-safety";

export function PasteSafetyDialog({
  text,
  onClose,
  onConfirm,
}: {
  text: string;
  onClose: () => void;
  onConfirm: () => void;
}) {
  const risk = pasteRisk(text);
  return (
    <Modal title="确认粘贴到远端终端" onClose={onClose}>
      <div className="paste-safety">
        <p className={risk.highRisk ? "danger" : ""}>
          <AlertTriangle size={16} />
          {risk.highRisk
            ? "检测到可能具有破坏性的命令，请逐行核对。"
            : `即将粘贴 ${risk.lines} 行内容，远端 Shell 可能立即执行。`}
        </p>
        <pre>{text}</pre>
        <footer className="form-actions">
          <button className="button secondary" onClick={onClose}>
            取消
          </button>
          <button className="button primary" onClick={onConfirm}>
            <Clipboard size={14} />
            确认粘贴
          </button>
        </footer>
      </div>
    </Modal>
  );
}

export function PasteHistoryDialog({
  items,
  onSelect,
  onClear,
  onClose,
}: {
  items: string[];
  onSelect: (text: string) => void;
  onClear: () => void;
  onClose: () => void;
}) {
  return (
    <Modal title="粘贴历史" onClose={onClose}>
      <div className="paste-history">
        <p className="paste-history-note">
          记录本次运行中通过终端粘贴的最近 20 条内容；关闭应用后自动清除。
        </p>
        {items.length ? (
          items.map((item, index) => (
            <button key={`${index}-${item}`} onClick={() => onSelect(item)}>
              <code>{item}</code>
              <small>{item.split(/\r?\n/).length} 行</small>
            </button>
          ))
        ) : (
          <div className="paste-history-empty" role="status">
            <Clipboard size={18} />
            <span>
              <strong>本次运行还没有粘贴内容</strong>
              <small>在任意终端使用粘贴后，可从这里再次选用。</small>
            </span>
          </div>
        )}
        <footer className="form-actions">
          <button
            className="button secondary"
            onClick={onClear}
            disabled={!items.length}
          >
            <Trash2 size={13} />
            清空
          </button>
          <button className="button primary" onClick={onClose}>
            完成
          </button>
        </footer>
      </div>
    </Modal>
  );
}
