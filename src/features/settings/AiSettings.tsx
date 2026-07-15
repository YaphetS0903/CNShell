import { Copy, MessageSquareText, Play, Save, Sparkles, Trash2 } from "lucide-react";
import { useEffect, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import type { AiRequestPreview, AiProviderProfile, BackgroundTask } from "../../types";

export function AiSettings({ onError }: { onError: (message: string) => void }) {
  const [providers, setProviders] = useState<AiProviderProfile[]>([]);
  const [selectedId, setSelectedId] = useState("");
  const [name, setName] = useState("");
  const [endpoint, setEndpoint] = useState("https://api.openai.com/v1");
  const [model, setModel] = useState("");
  const [apiKey, setApiKey] = useState("");
  const [kind, setKind] = useState("command");
  const [content, setContent] = useState("");
  const [preview, setPreview] = useState<AiRequestPreview | null>(null);
  const [task, setTask] = useState<BackgroundTask | null>(null);
  const [answer, setAnswer] = useState("");

  const select = (provider: AiProviderProfile) => { setSelectedId(provider.id); setName(provider.name); setEndpoint(provider.endpoint); setModel(provider.model); setApiKey(""); };
  useEffect(() => { void api.listAiProviders().then((items) => { setProviders(items); if (items[0]) select(items[0]); }).catch((error) => onError(errorMessage(error))); }, [onError]);
  useEffect(() => {
    if (!task || ["completed", "failed", "cancelled"].includes(task.status)) return;
    const timer = window.setInterval(() => {
      void api.getTask(task.id).then((next) => { setTask(next); if (next.status === "completed") setAnswer((next.result as { content?: string })?.content ?? ""); }).catch((error) => onError(errorMessage(error)));
    }, 400);
    return () => window.clearInterval(timer);
  }, [task, onError]);
  const save = async () => {
    try { const saved = await api.saveAiProvider({ id: selectedId || crypto.randomUUID(), name, endpoint, model, apiKey: apiKey || null }); setProviders((current) => [...current.filter((item) => item.id !== saved.id), saved]); select(saved); } catch (error) { onError(errorMessage(error)); }
  };
  const remove = async () => { if (!selectedId || !confirm("删除 AI Provider 配置及 Keychain API Key？")) return; try { await api.deleteAiProvider(selectedId); setProviders((current) => current.filter((item) => item.id !== selectedId)); setSelectedId(""); } catch (error) { onError(errorMessage(error)); } };
  const inspect = async () => { try { setAnswer(""); setPreview(await api.previewAi({ providerId: selectedId, kind, content })); } catch (error) { onError(errorMessage(error)); } };
  const execute = async () => { if (!preview) return; if (!confirm(`将把预览中的脱敏文本发送到 ${preview.providerName}，不会自动执行命令。确认发送？`)) return; try { setTask(await api.executeAi(preview.requestId)); } catch (error) { onError(errorMessage(error)); } };
  const copy = async () => { if (answer) await navigator.clipboard.writeText(answer); };

  return <section className="ai-settings" aria-label="AI 辅助">
    <div className="section-heading"><div><h3><Sparkles size={16} /> AI 辅助</h3></div></div>
    <div className="ai-providers">{providers.map((provider) => <button key={provider.id} className={provider.id === selectedId ? "active" : ""} onClick={() => select(provider)}><strong>{provider.name}</strong><small>{provider.endpoint} · {provider.model} · {provider.hasApiKey ? "已配置 Key" : "无 Key"}</small></button>)}</div>
    <div className="automation-meta"><label><span>Provider 名称</span><input value={name} onChange={(event) => setName(event.target.value)} /></label><label><span>兼容 endpoint</span><input value={endpoint} onChange={(event) => setEndpoint(event.target.value)} /></label><label><span>模型</span><input value={model} onChange={(event) => setModel(event.target.value)} placeholder="由 Provider 提供" /></label><label><span>API Key</span><input type="password" value={apiKey} onChange={(event) => setApiKey(event.target.value)} placeholder={selectedId ? "留空保持原 Key" : "可选，本地端点可不填"} /></label></div>
    <div className="ai-actions"><button className="button secondary" onClick={() => void save()}><Save size={14} /> 保存 Provider</button><IconButton icon={Trash2} label="删除 AI Provider" disabled={!selectedId} onClick={() => void remove()} /></div>
    <div className="ai-request"><label><span>请求类型</span><select value={kind} onChange={(event) => { setKind(event.target.value); setPreview(null); }}><option value="command">生成命令</option><option value="explain">解释错误</option><option value="summarize">总结日志</option></select></label><label><span>选中的文本</span><textarea aria-label="AI 输入" value={content} onChange={(event) => { setContent(event.target.value); setPreview(null); }} spellCheck={false} /></label><div className="ai-actions"><button className="button secondary" disabled={!selectedId || !content.trim()} onClick={() => void inspect()}><MessageSquareText size={14} /> 生成脱敏预览</button>{preview && <button className="button primary" onClick={() => void execute()}><Play size={14} /> 确认发送</button>}</div></div>
    {preview && <div className="ai-preview" aria-live="polite"><strong>{preview.providerName} · {preview.model}</strong><small>将发送的脱敏文本（{preview.redactions.length ? preview.redactions.join(", ") : "未发现敏感字段"}）</small><pre>{preview.redactedContent}</pre><small>预览有效至 {preview.expiresAt}</small></div>}
    {task && <p className="muted-copy" aria-live="polite">任务状态：{task.status}{task.error ? ` · ${task.error}` : ""}</p>}
    {answer && <div className="ai-answer"><div><strong>AI 结果</strong><IconButton icon={Copy} label="复制 AI 结果" onClick={() => void copy()} /></div><pre>{answer}</pre></div>}
  </section>;
}
