export interface TextDraft {
  content: string;
  base: string;
  modifiedAt: number | null;
}

const key = (connectionId: string, path: string) =>
  `cnshell-text-draft-v1:${JSON.stringify([connectionId, path])}`;

export function readTextDraft(
  connectionId: string,
  path: string,
): TextDraft | null {
  const stored = localStorage.getItem(key(connectionId, path));
  if (!stored) return null;
  const draft: unknown = JSON.parse(stored);
  if (
    !draft ||
    typeof draft !== "object" ||
    !("content" in draft) ||
    typeof draft.content !== "string" ||
    !("base" in draft) ||
    typeof draft.base !== "string" ||
    !("modifiedAt" in draft) ||
    (draft.modifiedAt !== null &&
      (typeof draft.modifiedAt !== "number" ||
        !Number.isFinite(draft.modifiedAt)))
  )
    throw new Error("本地草稿格式无效");
  return draft as TextDraft;
}

export function writeTextDraft(
  connectionId: string,
  path: string,
  draft: TextDraft,
) {
  if (draft.content === draft.base) removeTextDraft(connectionId, path);
  else localStorage.setItem(key(connectionId, path), JSON.stringify(draft));
}

export function removeTextDraft(connectionId: string, path: string) {
  localStorage.removeItem(key(connectionId, path));
}
