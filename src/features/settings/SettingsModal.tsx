import { useEffect, useMemo, useRef, useState } from "react";
import {
  Cloud,
  Info,
  Moon,
  Palette,
  Radio,
  Save,
  Settings2,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Workflow,
} from "lucide-react";
import { Modal } from "../../components/Modal";
import { useAppStore } from "../../store/app-store";
import type { AppSettings } from "../../types";
import { AdvancedSettings } from "./AdvancedSettings";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { UpdateSettings } from "./UpdateSettings";
import { OpenSshTools } from "./OpenSshTools";
import { ProtocolSettings } from "./ProtocolSettings";
import { AutomationSettings } from "./AutomationSettings";
import { EncryptedSyncSettings } from "./EncryptedSyncSettings";
import { WebDavSyncSettings } from "./WebDavSyncSettings";
import { AiSettings } from "./AiSettings";
import { PluginSettings } from "./PluginSettings";
import { TeamSettings } from "./TeamSettings";
import { TerminalPreferencesFields } from "./TerminalPreferencesFields";
import { usePlatformCapabilities } from "../../lib/platform";
import { McpSettings } from "./McpSettings";
import { FeedbackSettings } from "./FeedbackSettings";

type SettingsCategory = "basic" | "connection" | "automation" | "team" | "support";

const categories: Array<{
  id: SettingsCategory;
  label: string;
  description: string;
  icon: typeof Palette;
}> = [
  { id: "basic", label: "基础设置", description: "外观、终端和会话的常用选项", icon: Palette },
  { id: "connection", label: "连接与安全", description: "OpenSSH、协议转发、代理和备份", icon: ShieldCheck },
  { id: "automation", label: "自动化与集成", description: "自动化、AI、MCP 和插件能力", icon: Workflow },
  { id: "team", label: "团队与云端", description: "加密同步、WebDAV 和团队工作区", icon: Cloud },
  { id: "support", label: "关于与支持", description: "更新、反馈和脱敏诊断", icon: Info },
];

function sameSettings(left: AppSettings, right: AppSettings) {
  return JSON.stringify(left) === JSON.stringify(right);
}

export default function SettingsModal() {
  const platform = usePlatformCapabilities();
  const {
    settingsOpen,
    setSettingsOpen,
    settings,
    saveSettings,
    connections,
    refreshConnections,
    setError,
  } = useAppStore();
  const [draft, setDraft] = useState(settings);
  const [activeCategory, setActiveCategory] = useState<SettingsCategory>("basic");
  const [mountedCategories, setMountedCategories] = useState<Set<SettingsCategory>>(
    () => new Set(["basic"]),
  );
  const [saving, setSaving] = useState(false);
  const headingRef = useRef<HTMLHeadingElement>(null);

  useEffect(() => {
    if (!settingsOpen) return;
    setDraft(settings);
    setActiveCategory("basic");
    setMountedCategories(new Set(["basic"]));
  }, [settingsOpen, settings]);

  useEffect(() => {
    headingRef.current?.focus();
  }, [activeCategory]);

  const dirty = useMemo(() => !sameSettings(draft, settings), [draft, settings]);
  const change = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));

  const clearHistory = async () => {
    if (!confirm(`清空这台${platform.displayName}电脑上的全部命令历史？此操作无法撤销。`)) return;
    try {
      await api.clearHistory();
    } catch (error) {
      setError(errorMessage(error));
    }
  };

  const selectCategory = (category: SettingsCategory) => {
    setMountedCategories((current) => {
      if (current.has(category)) return current;
      const next = new Set(current);
      next.add(category);
      return next;
    });
    setActiveCategory(category);
  };

  const moveCategory = (index: number, direction: -1 | 1) => {
    const nextIndex = (index + direction + categories.length) % categories.length;
    const nextCategory = categories[nextIndex];
    selectCategory(nextCategory.id);
    document.getElementById(nextCategory.id)?.focus();
  };

  const requestClose = () => {
    if (saving) return;
    if (dirty && !confirm("设置尚未保存，确定放弃修改并关闭吗？")) return;
    setSettingsOpen(false);
  };

  const discardAndClose = () => {
    if (saving) return;
    setDraft(settings);
    setSettingsOpen(false);
  };

  const persistSettings = async () => {
    if (!dirty || saving) return;
    setSaving(true);
    try {
      await saveSettings(draft);
      setSettingsOpen(false);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  if (!settingsOpen) return null;

  const renderBasic = () => (
    <div className="settings-panel-content">
      <section className="settings-card">
        <h3><Moon size={16} />外观</h3>
        <label>
          <span>主题</span>
          <select value={draft.theme} onChange={(event) => change("theme", event.target.value as AppSettings["theme"])}>
            <option value="system">跟随系统</option>
            <option value="dark">深色</option>
            <option value="light">浅色</option>
            <option value="highContrast">高对比度</option>
          </select>
        </label>
      </section>
      <section className="settings-card">
        <h3><TerminalSquare size={16} />终端</h3>
        <TerminalPreferencesFields idPrefix="global-terminal" value={draft.terminal} onChange={(terminal) => change("terminal", terminal)} />
      </section>
      <section className="settings-card">
        <h3><Settings2 size={16} />会话与文件</h3>
        <label>
          <span>监控刷新</span>
          <select value={draft.monitorIntervalMs} onChange={(event) => change("monitorIntervalMs", Number(event.target.value))}>
            <option value={1000}>1 秒</option>
            <option value={2000}>2 秒（推荐）</option>
            <option value={5000}>5 秒</option>
          </select>
        </label>
        <label className="check-row"><input type="checkbox" checked={draft.rememberCommandHistory} onChange={(event) => change("rememberCommandHistory", event.target.checked)} /><span>保存非敏感命令历史</span></label>
        <button className="button secondary" onClick={() => void clearHistory()}>清空全部命令历史</button>
        <label className="check-row"><input type="checkbox" checked={draft.confirmCloseActiveSession} onChange={(event) => change("confirmCloseActiveSession", event.target.checked)} /><span>关闭活动会话前确认</span></label>
        <label className="check-row"><input type="checkbox" checked={draft.showHiddenFiles} onChange={(event) => change("showHiddenFiles", event.target.checked)} /><span>显示远端隐藏文件</span></label>
        <label className="check-row"><input type="checkbox" checked={draft.showWelcomeHelp} onChange={(event) => change("showWelcomeHelp", event.target.checked)} /><span>启动时显示使用帮助</span></label>
      </section>
    </div>
  );

  const renderConnection = () => (
    <div className="settings-panel-content">
      <OpenSshTools connections={connections} onChanged={refreshConnections} onError={(message) => setError(message)} />
      <ProtocolSettings connections={connections} onError={(message) => setError(message)} />
      <AdvancedSettings connections={connections} onChanged={refreshConnections} onError={(message) => setError(message)} includeFeedback={false} />
    </div>
  );

  const renderAutomation = () => (
    <div className="settings-panel-content">
      <AutomationSettings connections={connections} onError={(message) => setError(message)} />
      <AiSettings onError={(message) => setError(message)} />
      <McpSettings connections={connections} onError={(message) => setError(message)} />
      <PluginSettings connections={connections} onError={(message) => setError(message)} />
    </div>
  );

  const renderTeam = () => (
    <div className="settings-panel-content">
      <EncryptedSyncSettings onError={(message) => setError(message)} />
      <WebDavSyncSettings onError={(message) => setError(message)} />
      <TeamSettings connections={connections} onConnectionImported={refreshConnections} onError={(message) => setError(message)} />
    </div>
  );

  const renderSupport = () => (
    <div className="settings-panel-content">
      <UpdateSettings onError={(message) => setError(message)} />
      <FeedbackSettings onError={(message) => setError(message)} />
    </div>
  );

  const renderCategory = (category: SettingsCategory) => {
    switch (category) {
      case "basic": return renderBasic();
      case "connection": return renderConnection();
      case "automation": return renderAutomation();
      case "team": return renderTeam();
      case "support": return renderSupport();
    }
  };

  return (
    <Modal title="设置" onClose={requestClose} wide bodyClassName="settings-modal-body">
      <div className="settings-shell">
        <div className="settings-layout">
          <nav className="settings-navigation" aria-label="设置分类" role="tablist" aria-orientation="vertical">
            <div className="settings-navigation-heading">设置</div>
            {categories.map((category) => {
              const Icon = category.icon;
              return (
                <button
                  key={category.id}
                  id={category.id}
                  type="button"
                  role="tab"
                  aria-selected={activeCategory === category.id}
                  aria-controls={`settings-panel-${category.id}`}
                  className={`settings-nav-button${activeCategory === category.id ? " active" : ""}`}
                  onClick={() => selectCategory(category.id)}
                  onKeyDown={(event) => {
                    if (event.key === "ArrowDown" || event.key === "ArrowRight") {
                      event.preventDefault();
                      moveCategory(categories.findIndex((item) => item.id === category.id), 1);
                    } else if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
                      event.preventDefault();
                      moveCategory(categories.findIndex((item) => item.id === category.id), -1);
                    }
                  }}
                >
                  <Icon size={16} />
                  <span>{category.label}</span>
                </button>
              );
            })}
            <div className="settings-navigation-hint"><Sparkles size={14} />高级能力按需加载</div>
          </nav>
          <main className="settings-main" aria-live="polite">
            {categories.map((category) => {
              if (!mountedCategories.has(category.id)) return null;
              return (
                <section
                  key={category.id}
                  id={`settings-panel-${category.id}`}
                  role="tabpanel"
                  aria-labelledby={category.id}
                  hidden={activeCategory !== category.id}
                  className="settings-panel"
                >
                  <header className="settings-category-header">
                    <div>
                      <h2 ref={activeCategory === category.id ? headingRef : undefined} tabIndex={-1}>{category.label}</h2>
                      <p>{category.description}</p>
                    </div>
                    {category.id === "basic" && <span className="settings-badge"><Radio size={12} />常用</span>}
                  </header>
                  {renderCategory(category.id)}
                </section>
              );
            })}
          </main>
        </div>
        <footer className="settings-footer">
          <div className="settings-footer-status" role="status" aria-live="polite">
            {dirty ? <><span className="settings-dirty-dot" />有未保存的修改</> : "所有设置已保存"}
          </div>
          <div className="form-actions">
            <button className="button secondary" disabled={saving} onClick={discardAndClose}>取消</button>
            <button className="button primary" disabled={saving || !dirty} onClick={() => void persistSettings()}><Save size={14} />{saving ? "保存中…" : "保存设置"}</button>
          </div>
        </footer>
      </div>
    </Modal>
  );
}
