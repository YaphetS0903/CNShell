import { History, Pencil, PencilLine, Pin, Play, X } from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import { IconButton } from "../../components/IconButton";
import { api } from "../../lib/api";
import type {
  CommandHistorySummary,
  CommandSnippet,
  TerminalSession,
} from "../../types";
import { CommandTemplateDialog } from "./CommandTemplateDialog";
import { SmartCommandEntry } from "./SmartCommandEntry";
import { isHighRiskCommand, templateParameters } from "./smart-command";
import { publishRecordableCommand } from "../../lib/automation-recorder";
import {
  CommandSnippetDialog,
  type CommandSnippetDraft,
} from "./CommandSnippetDialog";
import {
  isSnippetPinned,
  snippetGroup,
  snippetTags,
} from "./command-snippet-metadata";

const builtInSnippets: CommandSnippet[] = [
  {
    id: "system",
    name: "系统概览",
    command: "uname -a && uptime",
    description: "",
    tags: [],
    sortOrder: 0,
    builtIn: true,
  },
  {
    id: "disk",
    name: "磁盘使用",
    command: "df -h",
    description: "",
    tags: [],
    sortOrder: 1,
    builtIn: true,
  },
  {
    id: "memory",
    name: "内存排行",
    command: "ps aux --sort=-%mem | head",
    description: "",
    tags: [],
    sortOrder: 2,
    builtIn: true,
  },
];

const formatHistoryTime = (value: string) => {
  const date = new Date(value);
  return Number.isNaN(date.getTime())
    ? value
    : new Intl.DateTimeFormat("zh-CN", {
        month: "numeric",
        day: "numeric",
        hour: "2-digit",
        minute: "2-digit",
      }).format(date);
};

export function CommandPanel({
  session,
  onError,
}: {
  session: TerminalSession;
  onError: (message: string | null) => void;
}) {
  const [snippets, setSnippets] = useState<CommandSnippet[]>([]);
  const [history, setHistory] = useState<CommandHistorySummary[]>([]);
  const [draft, setDraft] = useState("");
  const [pendingTemplate, setPendingTemplate] = useState<string | null>(null);
  const [editingSnippet, setEditingSnippet] = useState<CommandSnippet | null>(
    null,
  );
  const snippetGroups = useMemo(() => {
    const result: { name: string; items: CommandSnippet[] }[] = [];
    const pinned = snippets.filter(
      (item) => !item.builtIn && isSnippetPinned(item),
    );
    const builtIn = snippets.filter((item) => item.builtIn);
    if (pinned.length) result.push({ name: "置顶", items: pinned });
    if (builtIn.length) result.push({ name: "内置", items: builtIn });
    const custom = new Map<string, CommandSnippet[]>();
    for (const item of snippets) {
      if (item.builtIn || isSnippetPinned(item)) continue;
      const group = snippetGroup(item) || "未分组";
      custom.set(group, [...(custom.get(group) ?? []), item]);
    }
    for (const [name, items] of custom) result.push({ name, items });
    return result;
  }, [snippets]);
  const load = useCallback(() => {
    void api
      .listSnippets()
      .then((items) => setSnippets([...builtInSnippets, ...items]));
    void api.listHistorySummary(session.connectionId).then(setHistory);
  }, [session.connectionId]);
  useEffect(() => {
    load();
  }, [load]);
  const execute = async (command: string) => {
    if (
      isHighRiskCommand(command) &&
      !confirm(`这是高风险命令，请核对后确认执行：\n\n${command}`)
    )
      return;
    try {
      await api.terminalInput(session.id, `${command}\n`);
      publishRecordableCommand(session.connectionId, command);
      await api.addHistory(session.connectionId, command);
      setDraft("");
      setHistory(await api.listHistorySummary(session.connectionId));
    } catch (error) {
      onError(String(error));
    }
  };
  const run = (command: string) => {
    const normalized = command.trim();
    if (!normalized) return;
    if (templateParameters(normalized).length) {
      setPendingTemplate(normalized);
      return;
    }
    void execute(normalized);
  };
  const saveSnippet = async (value: CommandSnippetDraft) => {
    if (!editingSnippet) return;
    try {
      await api.saveSnippet({
        ...editingSnippet,
        id: editingSnippet.id || crypto.randomUUID(),
        name: value.name,
        command: value.command,
        tags: snippetTags(value.group, value.pinned),
      });
      setDraft("");
      setEditingSnippet(null);
      load();
    } catch (error) {
      onError(String(error));
    }
  };
  const openNewSnippet = () => {
    if (!draft.trim()) return;
    setEditingSnippet({
      id: "",
      name: "",
      command: draft.trim(),
      description: "",
      tags: [],
      sortOrder: snippets.length,
    });
  };
  const togglePinned = async (item: CommandSnippet) => {
    try {
      await api.saveSnippet({
        ...item,
        tags: snippetTags(snippetGroup(item), !isSnippetPinned(item)),
      });
      load();
    } catch (error) {
      onError(String(error));
    }
  };
  const remove = async (id: string, name: string) => {
    if (!confirm(`删除快捷命令“${name}”？`)) return;
    try {
      await api.deleteSnippet(id);
      load();
    } catch (error) {
      onError(String(error));
    }
  };
  return (
    <div className="commands-panel">
      <SmartCommandEntry
        session={session}
        snippets={snippets}
        history={history.map((item) => item.command)}
        draft={draft}
        setDraft={setDraft}
        onRun={run}
        onSave={openNewSnippet}
      />
      <div className="command-groups">
        {snippetGroups.map((group) => (
          <section key={group.name} className="command-group">
            <header>
              <strong>{group.name}</strong>
              <span>{group.items.length}</span>
            </header>
            <div className="command-grid">
              {group.items.map((item) => (
                <div key={item.id} className="command-card">
                  <button
                    className="command-card-fill"
                    aria-label={`${item.name}，填入命令`}
                    onClick={() => setDraft(item.command)}
                  >
                    <PencilLine size={16} />
                    <strong>{item.name}</strong>
                    <code>{item.command}</code>
                  </button>
                  <div className="command-card-actions">
                    <IconButton
                      icon={Play}
                      label={`执行快捷命令 ${item.name}`}
                      className="command-run-action"
                      onClick={() => run(item.command)}
                    />
                    {!item.builtIn && (
                      <>
                        <IconButton
                          icon={Pin}
                          label={`${isSnippetPinned(item) ? "取消置顶" : "置顶"}快捷命令 ${item.name}`}
                          active={isSnippetPinned(item)}
                          onClick={() => void togglePinned(item)}
                        />
                        <IconButton
                          icon={Pencil}
                          label={`编辑快捷命令 ${item.name}`}
                          onClick={() => setEditingSnippet(item)}
                        />
                        <IconButton
                          icon={X}
                          label={`删除快捷命令 ${item.name}`}
                          className="command-remove-action"
                          onClick={() => void remove(item.id, item.name)}
                        />
                      </>
                    )}
                  </div>
                </div>
              ))}
            </div>
          </section>
        ))}
      </div>
      {history.length > 0 && (
        <div className="history-list">
          <h3>最近命令</h3>
          {history.map(({ command, count, lastUsedAt }) => (
            <button key={command} onClick={() => setDraft(command)}>
              <History size={13} />
              <code>{command}</code>
              <span title={`最近记录 ${count} 次`}>×{count}</span>
              <time dateTime={lastUsedAt}>{formatHistoryTime(lastUsedAt)}</time>
            </button>
          ))}
        </div>
      )}
      {pendingTemplate && (
        <CommandTemplateDialog
          template={pendingTemplate}
          onClose={() => setPendingTemplate(null)}
          onRun={(command) => {
            setPendingTemplate(null);
            void execute(command);
          }}
        />
      )}
      {editingSnippet && (
        <CommandSnippetDialog
          editing={Boolean(editingSnippet.id)}
          initial={{
            name: editingSnippet.name,
            command: editingSnippet.command,
            group: snippetGroup(editingSnippet),
            pinned: isSnippetPinned(editingSnippet),
          }}
          onClose={() => setEditingSnippet(null)}
          onSave={saveSnippet}
        />
      )}
    </div>
  );
}
