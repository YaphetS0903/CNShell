import { useCallback, useEffect, useRef, useState } from "react";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import {
  readTextDraft,
  writeTextDraft,
  removeTextDraft,
  type TextDraft,
} from "./text-drafts";

export type TextConflict = {
  base: string;
  local: string;
  remote: string;
  remoteModifiedAt: number | null;
};

export function useRemoteTextDocument(
  sessionId: string,
  path: string,
  connectionId = sessionId,
) {
  const generationRef = useRef(0);
  const [content, setContentState] = useState("");
  const contentRef = useRef("");
  const revisionRef = useRef(0);
  const savingRef = useRef(false);
  const setContent = useCallback((value: string) => {
    if (value === contentRef.current) return;
    revisionRef.current += 1;
    contentRef.current = value;
    setContentState(value);
    setSaved(false);
  }, []);
  const [base, setBase] = useState("");
  const [modifiedAt, setModifiedAt] = useState<number | null>(null);
  const [loading, setLoading] = useState(true);
  const [saving, setSaving] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [saved, setSaved] = useState(false);
  const [conflict, setConflict] = useState<TextConflict | null>(null);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [draftReadError, setDraftReadError] = useState<string | null>(null);
  const [restoredDraft, setRestoredDraft] = useState(false);
  const preserveDraft = useCallback(() => {
    if (loading) return true;
    if (draftReadError) return false;
    try {
      writeTextDraft(connectionId, path, { content, base, modifiedAt });
      setDraftError(null);
      return true;
    } catch (reason) {
      setDraftError(
        `本地草稿保存失败，请先保存远端文件或明确放弃修改：${errorMessage(reason)}`,
      );
      return false;
    }
  }, [loading, draftReadError, connectionId, path, content, base, modifiedAt]);
  const discardDraft = () => {
    try {
      removeTextDraft(connectionId, path);
      setDraftError(null);
      setDraftReadError(null);
      return true;
    } catch (reason) {
      setDraftError(`本地草稿清理失败：${errorMessage(reason)}`);
      return false;
    }
  };
  useEffect(() => {
    const generation = ++generationRef.current;
    savingRef.current = false;
    setLoading(true);
    setSaving(false);
    setSaved(false);
    setError(null);
    setConflict(null);
    setDraftError(null);
    setDraftReadError(null);
    setRestoredDraft(false);
    setContent("");
    setBase("");
    setModifiedAt(null);
    let draft: TextDraft | null = null;
    try {
      draft = readTextDraft(connectionId, path);
      if (draft) {
        setContent(draft.content);
        setBase(draft.base);
        setModifiedAt(draft.modifiedAt);
        setRestoredDraft(true);
      }
    } catch (reason) {
      setDraftReadError(
        `读取本地草稿失败，原草稿仍保留：${errorMessage(reason)}`,
      );
    }
    api
      .openText(sessionId, path)
      .then((file) => {
        if (generation !== generationRef.current) return;
        if (draft && draft.content !== file.content) {
          // Keep the draft's original baseline until a changed remote is reviewed.
          if (draft.base !== file.content)
            setConflict({
              base: draft.base,
              local: draft.content,
              remote: file.content,
              remoteModifiedAt: file.modifiedAt,
            });
          else setModifiedAt(file.modifiedAt);
        } else {
          setContent(file.content);
          setBase(file.content);
          setModifiedAt(file.modifiedAt);
          setRestoredDraft(false);
        }
      })
      .catch((reason) => {
        if (generation === generationRef.current)
          setError(errorMessage(reason));
      })
      .finally(() => {
        if (generation === generationRef.current) setLoading(false);
      });
    return () => {
      generationRef.current += 1;
    };
  }, [path, sessionId, connectionId, setContent]);
  useEffect(() => {
    if (!loading) preserveDraft();
  }, [loading, preserveDraft]);
  const persist = async (expected = modifiedAt, value = content) => {
    if (savingRef.current) return false;
    savingRef.current = true;
    const revision = revisionRef.current;
    const generation = generationRef.current;
    setSaving(true);
    setError(null);
    setSaved(false);
    try {
      await api.saveText(sessionId, path, value, expected);
      if (generation !== generationRef.current) return false;
      const current = await api.openText(sessionId, path);
      if (generation !== generationRef.current) return false;
      const unchanged = revision === revisionRef.current;
      if (unchanged) setContent(current.content);
      setBase(current.content);
      setModifiedAt(current.modifiedAt);
      setConflict(null);
      setSaved(unchanged);
      setDraftReadError(null);
      return true;
    } catch (reason) {
      if (generation !== generationRef.current) return false;
      const message = errorMessage(reason);
      if (message.includes("已被其他程序修改")) {
        try {
          const remote = await api.openText(sessionId, path);
          if (generation !== generationRef.current) return false;
          const local =
            revision === revisionRef.current ? value : contentRef.current;
          setContent(local);
          setConflict({
            base,
            local,
            remote: remote.content,
            remoteModifiedAt: remote.modifiedAt,
          });
        } catch (refreshError) {
          if (generation === generationRef.current)
            setError(errorMessage(refreshError));
        }
      } else setError(message);
      return false;
    } finally {
      if (generation === generationRef.current) {
        savingRef.current = false;
        setSaving(false);
      }
    }
  };
  return {
    content,
    setContent,
    base,
    setBase,
    setModifiedAt,
    loading,
    saving,
    error,
    setError,
    saved,
    conflict,
    setConflict,
    persist,
    isSaving: () => savingRef.current,
    draftError: draftError ?? draftReadError,
    unreadableDraft: Boolean(draftReadError),
    restoredDraft,
    preserveDraft,
    discardDraft,
  };
}
