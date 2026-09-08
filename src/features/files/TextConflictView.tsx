import { diffLines, type Change } from "diff";
import type { TextConflict } from "./use-remote-text-document";

export function TextConflictView({
  conflict,
  onUseRemote,
  onKeepEditing,
  onOverwrite,
}: {
  conflict: TextConflict;
  onUseRemote: () => void;
  onKeepEditing: () => void;
  onOverwrite: () => void;
}) {
  return (
    <div className="conflict-view">
      <div className="conflict-warning">
        <strong>远端文件在编辑期间发生变化</strong>
        <span>核对三个版本后选择如何继续，CNshell 不会静默覆盖。</span>
      </div>
      <div className="three-way-diff">
        <DiffColumn
          title="基础版本"
          changes={[{ value: conflict.base } as Change]}
        />
        <DiffColumn
          title="本地版本"
          changes={diffLines(conflict.base, conflict.local)}
        />
        <DiffColumn
          title="远端版本"
          changes={diffLines(conflict.base, conflict.remote)}
        />
      </div>
      <footer className="form-actions">
        <button className="button secondary" onClick={onKeepEditing}>
          继续编辑本地版本
        </button>
        <button className="button secondary" onClick={onUseRemote}>
          使用远端版本
        </button>
        <button className="button primary" onClick={onOverwrite}>
          以本地版本覆盖
        </button>
      </footer>
    </div>
  );
}
function DiffColumn({ title, changes }: { title: string; changes: Change[] }) {
  return (
    <section>
      <h3>{title}</h3>
      <pre>
        {changes.map((change, index) => (
          <span
            className={change.added ? "added" : change.removed ? "removed" : ""}
            key={index}
          >
            {change.value}
          </span>
        ))}
      </pre>
    </section>
  );
}
