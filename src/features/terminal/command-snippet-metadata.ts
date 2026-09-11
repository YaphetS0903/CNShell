import type { CommandSnippet } from "../../types";

export const pinnedSnippetTag = "cnshell:pinned";

export const isSnippetPinned = (snippet: CommandSnippet) =>
  snippet.tags.includes(pinnedSnippetTag);

export const snippetGroup = (snippet: CommandSnippet) =>
  snippet.tags.find((tag) => tag !== pinnedSnippetTag) ?? "";

export const snippetTags = (group: string, pinned: boolean) => [
  ...(group.trim() ? [group.trim()] : []),
  ...(pinned ? [pinnedSnippetTag] : []),
];
