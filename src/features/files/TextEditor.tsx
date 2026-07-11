import { useEffect, useState } from "react";
import { LoaderCircle, Save } from "lucide-react";
import { api } from "../../lib/api";
import { Modal } from "../../components/Modal";
import { errorMessage } from "../../lib/format";

export function TextEditor({ sessionId, path, onClose }: { sessionId: string; path: string; onClose: () => void }) {
  const [content, setContent] = useState(""); const [modifiedAt, setModifiedAt] = useState<number|null>(null); const [loading, setLoading] = useState(true); const [saving, setSaving] = useState(false); const [error, setError] = useState<string|null>(null);
  useEffect(() => { api.openText(sessionId, path).then((file) => { setContent(file.content); setModifiedAt(file.modifiedAt); }).catch((reason) => setError(errorMessage(reason))).finally(() => setLoading(false)); }, [path, sessionId]);
  const save = async () => { setSaving(true); setError(null); try { await api.saveText(sessionId,path,content,modifiedAt); onClose(); } catch (reason) { setError(errorMessage(reason)); } finally { setSaving(false); } };
  return <Modal title={path.split("/").at(-1) ?? path} onClose={onClose} wide>{loading ? <div className="loading-state"><LoaderCircle className="spin"/>读取远端文件…</div> : <div className="text-editor">{error && <div className="inline-error">{error}</div>}<textarea spellCheck={false} value={content} onChange={(event) => setContent(event.target.value)} aria-label="远程文本内容"/><footer><span>{new Blob([content]).size.toLocaleString()} bytes · UTF-8</span><button className="button primary" onClick={save} disabled={saving}><Save size={15}/>{saving ? "保存中…" : "原子保存"}</button></footer></div>}</Modal>;
}
