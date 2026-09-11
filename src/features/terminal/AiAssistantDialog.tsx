import {
  Copy,
  MessageSquareText,
  Play,
  Settings2,
  Sparkles,
} from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import { useAppStore } from "../../store/app-store";
import type {
  AiProviderProfile,
  AiRequestPreview,
  BackgroundTask,
  TerminalSession,
} from "../../types";

type RequestKind = "command" | "explain" | "summarize";

export function AiAssistantDialog({
  session,
  onClose,
  onError,
}: {
  session: TerminalSession;
  onClose: () => void;
  onError: (message: string) => void;
}) {
  const setSettingsOpen = useAppStore((state) => state.setSettingsOpen);
  const initialContent =
    workspaceRuntime.terminalSelectionBySession.get(session.id) ?? "";
  const [providers, setProviders] = useState<AiProviderProfile[]>([]);
  const [loading, setLoading] = useState(true);
  const [selectedId, setSelectedId] = useState("");
  const [kind, setKind] = useState<RequestKind>(
    initialContent ? "explain" : "command",
  );
  const [content, setContent] = useState(initialContent);
  const [preview, setPreview] = useState<AiRequestPreview | null>(null);
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [answer, setAnswer] = useState("");

  useEffect(() => {
    void api
      .listAiProviders()
      .then((items) => {
        setProviders(items);
        setSelectedId(items[0]?.id ?? "");
      })
      .catch((error) => onError(errorMessage(error)))
      .finally(() => setLoading(false));
  }, [onError]);

  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status))
      return;
    const timer = window.setInterval(() => {
      void api
        .getTask(task.id)
        .then((next) => {
          setTask(next);
          if (next.status === "completed")
            setAnswer((next.result as { content?: string })?.content ?? "");
        })
        .catch((error) => onError(errorMessage(error)));
    }, 400);
    return () => window.clearInterval(timer);
  }, [task, onError]);

  const inspect = async () => {
    try {
      setAnswer("");
      setTask(null);
      setPreview(
        await api.previewAi({ providerId: selectedId, kind, content }),
      );
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  const execute = async () => {
    if (!preview) return;
    if (
      !confirm(
        `将把预览中的脱敏文本发送到 ${preview.providerName}，返回内容不会自动执行。确认发送？`,
      )
    )
      return;
    try {
      setAnswer("");
      setTask(await api.executeAi(preview.requestId));
    } catch (error) {
      onError(errorMessage(error));
    }
  };

  const openProviderSettings = () => {
    onClose();
    setSettingsOpen(true, { category: "automation", moduleId: "ai" });
  };

  return (
    <Modal title={`${session.title} · AI 辅助`} onClose={onClose} wide>
      <div className="ai-assistant-dialog">
        {loading ? (
          <div className="loading-state">正在读取 AI Provider…</div>
        ) : providers.length === 0 ? (
          <div className="ai-assistant-empty">
            <Sparkles size={28} />
            <strong>尚未配置 AI Provider</strong>
            <span>添加兼容服务和模型后，才能生成脱敏请求预览。</span>
            <button className="button primary" onClick={openProviderSettings}>
              <Settings2 size={14} />
              前往 AI 设置
            </button>
          </div>
        ) : (
          <>
            <div className="ai-assistant-controls">
              <label>
                <span>Provider</span>
                <select
                  aria-label="AI Provider"
                  value={selectedId}
                  onChange={(event) => {
                    setSelectedId(event.target.value);
                    setPreview(null);
                  }}
                >
                  {providers.map((provider) => (
                    <option key={provider.id} value={provider.id}>
                      {provider.name} · {provider.model}
                    </option>
                  ))}
                </select>
              </label>
              <label>
                <span>请求类型</span>
                <select
                  value={kind}
                  onChange={(event) => {
                    setKind(event.target.value as RequestKind);
                    setPreview(null);
                  }}
                >
                  <option value="command">生成命令</option>
                  <option value="explain">解释错误</option>
                  <option value="summarize">总结日志</option>
                </select>
              </label>
            </div>
            <label className="ai-assistant-input">
              <span>
                输入内容
                {initialContent && <small>已带入当前终端选区</small>}
              </span>
              <textarea
                data-modal-initial-focus
                aria-label="AI 输入"
                value={content}
                onChange={(event) => {
                  setContent(event.target.value);
                  setPreview(null);
                }}
                placeholder={requestPlaceholder(kind)}
                spellCheck={false}
              />
            </label>
            <div className="ai-actions">
              <button
                className="button secondary"
                disabled={!selectedId || !content.trim()}
                onClick={() => void inspect()}
              >
                <MessageSquareText size={14} />
                生成脱敏预览
              </button>
              {preview && (
                <button
                  className="button primary"
                  onClick={() => void execute()}
                >
                  <Play size={14} />
                  确认发送
                </button>
              )}
            </div>
            {preview && (
              <div className="ai-preview" aria-live="polite">
                <strong>
                  {preview.providerName} · {preview.model}
                </strong>
                <small>
                  将发送的脱敏文本（
                  {preview.redactions.length
                    ? preview.redactions.join(", ")
                    : "未发现敏感字段"}
                  ）
                </small>
                <pre>{preview.redactedContent}</pre>
                <small>预览有效至 {preview.expiresAt}</small>
              </div>
            )}
            {task && (
              <p className="muted-copy" aria-live="polite">
                任务状态：{task.status}
                {task.error ? ` · ${task.error}` : ""}
              </p>
            )}
            {answer && (
              <div className="ai-answer">
                <div>
                  <strong>AI 结果</strong>
                  <IconButton
                    icon={Copy}
                    label="复制 AI 结果"
                    onClick={() => void navigator.clipboard.writeText(answer)}
                  />
                </div>
                <pre>{answer}</pre>
              </div>
            )}
          </>
        )}
      </div>
    </Modal>
  );
}

function requestPlaceholder(kind: RequestKind) {
  if (kind === "explain") return "粘贴或在终端中选中需要解释的错误输出";
  if (kind === "summarize") return "粘贴或在终端中选中需要总结的日志";
  return "描述希望生成的命令及必要约束";
}
