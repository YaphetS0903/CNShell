import { Save } from "lucide-react";
import { useState, type FormEvent } from "react";
import { Modal } from "../../components/Modal";

export interface CommandSnippetDraft {
  name: string;
  command: string;
  group: string;
  pinned: boolean;
}

export function CommandSnippetDialog({
  initial,
  editing,
  onClose,
  onSave,
}: {
  initial: CommandSnippetDraft;
  editing: boolean;
  onClose: () => void;
  onSave: (draft: CommandSnippetDraft) => Promise<void>;
}) {
  const [draft, setDraft] = useState(initial);
  const [busy, setBusy] = useState(false);
  const submit = async (event: FormEvent) => {
    event.preventDefault();
    if (!draft.name.trim() || !draft.command.trim()) return;
    setBusy(true);
    try {
      await onSave({
        ...draft,
        name: draft.name.trim(),
        command: draft.command.trim(),
        group: draft.group.trim(),
      });
    } finally {
      setBusy(false);
    }
  };
  return (
    <Modal title={editing ? "编辑快捷命令" : "保存快捷命令"} onClose={onClose}>
      <form
        className="command-snippet-form"
        onSubmit={(event) => void submit(event)}
      >
        <label>
          <span>名称</span>
          <input
            data-modal-initial-focus
            value={draft.name}
            onChange={(event) =>
              setDraft({ ...draft, name: event.target.value })
            }
            placeholder="例如：查看服务状态"
            required
          />
        </label>
        <label>
          <span>命令</span>
          <textarea
            value={draft.command}
            onChange={(event) =>
              setDraft({ ...draft, command: event.target.value })
            }
            rows={4}
            required
          />
        </label>
        <label>
          <span>分组</span>
          <input
            value={draft.group}
            onChange={(event) =>
              setDraft({ ...draft, group: event.target.value })
            }
            placeholder="例如：服务管理（可选）"
          />
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.pinned}
            onChange={(event) =>
              setDraft({ ...draft, pinned: event.target.checked })
            }
          />
          <span>置顶显示</span>
        </label>
        <footer className="form-actions">
          <button type="button" className="button secondary" onClick={onClose}>
            取消
          </button>
          <button
            type="submit"
            className="button primary"
            disabled={busy || !draft.name.trim() || !draft.command.trim()}
          >
            <Save size={14} /> {busy ? "保存中…" : "保存"}
          </button>
        </footer>
      </form>
    </Modal>
  );
}
