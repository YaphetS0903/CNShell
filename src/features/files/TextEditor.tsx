import { useCallback, useLayoutEffect, useRef, useState } from "react";
import {
  Braces,
  ChevronDown,
  ChevronUp,
  ExternalLink,
  LoaderCircle,
  Maximize2,
  Minimize2,
  Save,
  Search,
  Upload,
} from "lucide-react";
import { open } from "@tauri-apps/plugin-dialog";
import { parse as parseYaml, stringify as stringifyYaml } from "yaml";
import { api } from "../../lib/api";
import { Modal } from "../../components/Modal";
import { errorMessage } from "../../lib/format";
import { usePlatformCapabilities } from "../../lib/platform";
import { IconButton } from "../../components/IconButton";
import { RemoteCodeEditor, type CodeEditorActions } from "./RemoteCodeEditor";
import { externalApplicationDialogOptions } from "./external-application";
import type { ExternalEditSession } from "../../types";
import "./TextEditor.css";
import { useRemoteTextDocument } from "./use-remote-text-document";
import { TextConflictView } from "./TextConflictView";
import { registerEditorGuard } from "../../lib/editor-lifecycle";

export function TextEditor({
  sessionId,
  connectionId,
  connectionName,
  path,
  onClose,
}: {
  sessionId: string;
  connectionId?: string;
  connectionName?: string;
  path: string;
  onClose: () => void;
}) {
  const platform = usePlatformCapabilities();
  const {
    content,
    setContent,
    base,
    setBase,
    setModifiedAt,
    loading,
    saving: documentSaving,
    error,
    setError,
    saved,
    conflict,
    setConflict,
    persist,
    isSaving,
    draftError,
    unreadableDraft,
    restoredDraft,
    preserveDraft,
    discardDraft,
  } = useRemoteTextDocument(sessionId, path, connectionId);
  const [importing, setImporting] = useState(false);
  const [externalBusy, setExternalBusy] = useState(false);
  const [maximized, setMaximized] = useState(false);
  const externalBusyRef = useRef(false);
  const saving = documentSaving || importing || externalBusy;
  const supportsFormatting = /\.(jsonc?|ya?ml)$/i.test(path);
  const [externalEdit, setExternalEdit] = useState<ExternalEditSession | null>(
    null,
  );
  const editorRef = useRef<CodeEditorActions>(null);
  const canLeave = useCallback(() => {
    if (isSaving() || saving || externalBusyRef.current) {
      setError("正在处理文件，请等待操作完成后关闭");
      return false;
    }
    if (externalEdit) {
      setError("外部编辑副本尚未回传，请先回传或放弃副本");
      return false;
    }
    if (!preserveDraft()) return false;
    return (
      content === base ||
      confirm(
        "文件有未保存的修改。本地草稿已保留，重新打开此连接的文件时可恢复。继续关闭编辑器？",
      )
    );
  }, [isSaving, saving, setError, externalEdit, preserveDraft, content, base]);
  useLayoutEffect(
    () =>
      registerEditorGuard({
        canLeave,
        hasPendingWork: () =>
          content !== base ||
          Boolean(externalEdit) ||
          isSaving() ||
          saving ||
          externalBusyRef.current,
      }),
    [canLeave, content, base, externalEdit, isSaving, saving],
  );
  const format = () => {
    try {
      const lower = path.toLowerCase();
      if (lower.endsWith(".json") || lower.endsWith(".jsonc"))
        setContent(`${JSON.stringify(JSON.parse(content), null, 2)}\n`);
      else if (/\.(ya?ml)$/.test(lower))
        setContent(stringifyYaml(parseYaml(content)));
      else {
        setError("当前文件类型不支持自动格式化");
        return;
      }
      setError(null);
    } catch (reason) {
      setError(`格式化失败：${errorMessage(reason)}`);
    }
  };
  const overwrite = () => {
    if (!conflict || !confirm("远端内容会被当前本地版本覆盖。确认继续？"))
      return;
    setContent(conflict.local);
    void persist(conflict.remoteModifiedAt, conflict.local);
  };
  const startExternal = async (application?: string) => {
    if (saving || externalEdit || externalBusyRef.current) return;
    externalBusyRef.current = true;
    setExternalBusy(true);
    try {
      setExternalEdit(
        await api.startExternalEdit(sessionId, path, application),
      );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      externalBusyRef.current = false;
      setExternalBusy(false);
    }
  };
  const chooseExternal = async () => {
    if (saving || externalEdit || externalBusyRef.current) return;
    externalBusyRef.current = true;
    setExternalBusy(true);
    try {
      const application = await open(
        externalApplicationDialogOptions(
          platform,
          `选择${platform.displayName}外部文本编辑器`,
        ),
      );
      if (application)
        setExternalEdit(
          await api.startExternalEdit(sessionId, path, application),
        );
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      externalBusyRef.current = false;
      setExternalBusy(false);
    }
  };
  const importExternal = async () => {
    if (!externalEdit || saving || externalBusyRef.current) return;
    externalBusyRef.current = true;
    setImporting(true);
    try {
      const snapshot = await api.readExternalEdit(externalEdit.id);
      const success = await persist(
        snapshot.expectedModifiedAt,
        snapshot.content,
      );
      if (success) {
        await api.discardExternalEdit(externalEdit.id);
        setExternalEdit(null);
      }
    } catch (reason) {
      setError(errorMessage(reason));
    } finally {
      externalBusyRef.current = false;
      setImporting(false);
    }
  };
  const discardExternal = async () => {
    if (!externalEdit || saving || externalBusyRef.current) return;
    externalBusyRef.current = true;
    setExternalBusy(true);
    try {
      await api.discardExternalEdit(externalEdit.id);
    } catch {
      /* stale temporary files are cleaned on next launch */
    }
    setExternalEdit(null);
    externalBusyRef.current = false;
    setExternalBusy(false);
  };
  const close = () => {
    if (isSaving() || saving || externalBusyRef.current) {
      setError("正在保存文件，请等待保存完成后关闭");
      return;
    }
    if (
      (content !== base || unreadableDraft) &&
      !confirm(
        unreadableDraft
          ? "本地草稿无法读取，关闭将清除原草稿并放弃未保存的修改。继续？"
          : "文件有未保存的修改，关闭将放弃这些修改。继续？",
      )
    )
      return;
    if (
      externalEdit &&
      !confirm("外部编辑副本尚未回传，关闭将放弃该副本。继续？")
    )
      return;
    if (!discardDraft()) return;
    if (externalEdit)
      void api.discardExternalEdit(externalEdit.id).catch(() => undefined);
    onClose();
  };
  return (
    <Modal
      title={`${connectionName ? `${connectionName} · ` : ""}${path.split("/").at(-1) ?? path}`}
      onClose={close}
      wide
      dialogClassName={maximized ? "text-editor-modal-maximized" : undefined}
    >
      {loading ? (
        <div className="loading-state">
          <LoaderCircle className="spin" />
          读取远端文件…
        </div>
      ) : (
        <div className="text-editor">
          {error && <div className="inline-error">{error}</div>}
          {draftError && (
            <div className="inline-error" role="alert">
              {draftError}
            </div>
          )}
          {restoredDraft && (
            <div role="status">已恢复本地草稿，请核对后保存到远端。</div>
          )}
          {externalEdit && (
            <div className="external-edit-banner">
              <ExternalLink size={16} />
              <div>
                <strong>外部编辑副本已打开</strong>
                <code>{externalEdit.localPath}</code>
              </div>
              <button
                className="button secondary"
                onClick={() => void discardExternal()}
                disabled={saving}
              >
                放弃
              </button>
              <button
                className="button primary"
                onClick={() => void importExternal()}
                disabled={saving}
              >
                <Upload size={14} />
                读取并回传
              </button>
            </div>
          )}
          {conflict ? (
            <TextConflictView
              conflict={conflict}
              onUseRemote={() => {
                setContent(conflict.remote);
                setBase(conflict.remote);
                setModifiedAt(conflict.remoteModifiedAt);
                setConflict(null);
              }}
              onKeepEditing={() => setConflict(null)}
              onOverwrite={overwrite}
            />
          ) : (
            <>
              <div className="editor-toolbar">
                <IconButton
                  icon={Search}
                  label="搜索与替换"
                  onClick={() => editorRef.current?.search()}
                />
                <IconButton
                  icon={ChevronUp}
                  label="折叠全部"
                  onClick={() => editorRef.current?.fold()}
                />
                <IconButton
                  icon={ChevronDown}
                  label="展开全部"
                  onClick={() => editorRef.current?.unfold()}
                />
                <span className="toolbar-separator" />
                <button
                  className="mini-button"
                  onClick={format}
                  disabled={!supportsFormatting}
                  title={
                    supportsFormatting
                      ? "格式化当前文件"
                      : "仅 JSON、JSONC 和 YAML 文件支持格式化"
                  }
                >
                  <Braces size={13} />
                  格式化
                </button>
                <button
                  className="mini-button"
                  onClick={() => void startExternal()}
                  disabled={saving || Boolean(externalEdit)}
                >
                  <ExternalLink size={13} />
                  外部应用
                </button>
                <button
                  className="mini-button"
                  onClick={() => void chooseExternal()}
                  disabled={saving || Boolean(externalEdit)}
                >
                  选择应用
                </button>
                <span className="editor-path" title={path}>
                  {path}
                </span>
                <IconButton
                  icon={maximized ? Minimize2 : Maximize2}
                  label={maximized ? "还原编辑区" : "放大编辑区"}
                  onClick={() => setMaximized((current) => !current)}
                />
              </div>
              <RemoteCodeEditor
                ref={editorRef}
                path={path}
                value={content}
                onChange={setContent}
              />
            </>
          )}
          <footer>
            <span>
              {new Blob([content]).size.toLocaleString()} bytes · UTF-8
              {content !== base
                ? ` · 未保存${draftError ? "" : " · 本地草稿已保留"}`
                : saved
                  ? " · 已保存"
                  : ""}
            </span>
            <div>
              {content !== base && (
                <button
                  className="button secondary"
                  disabled={saving}
                  onClick={() => {
                    if (canLeave()) onClose();
                  }}
                >
                  保留草稿并关闭
                </button>
              )}
              <button className="button secondary" onClick={close}>
                关闭
              </button>
              <button
                className="button primary"
                onClick={() => void persist()}
                disabled={saving || content === base}
              >
                <Save size={15} />
                {saving ? "保存中…" : "保存到服务器"}
              </button>
            </div>
          </footer>
        </div>
      )}
    </Modal>
  );
}
