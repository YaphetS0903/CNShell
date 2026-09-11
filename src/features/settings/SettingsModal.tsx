import { useShallow } from "zustand/react/shallow";
import {
  lazy,
  useCallback,
  useDeferredValue,
  useEffect,
  useMemo,
  useRef,
  useState,
} from "react";
import {
  Blocks,
  Bot,
  Braces,
  Cloud,
  CloudCog,
  DownloadCloud,
  FileKey2,
  Info,
  KeyRound,
  MessageSquareText,
  Moon,
  Network,
  Palette,
  Radio,
  RotateCcw,
  Save,
  Search,
  Settings2,
  ShieldAlert,
  ShieldCheck,
  Sparkles,
  TerminalSquare,
  Users,
  Workflow,
} from "lucide-react";
import { Modal } from "../../components/Modal";
import { api } from "../../lib/api";
import { errorMessage } from "../../lib/format";
import { usePlatformCapabilities } from "../../lib/platform";
import { useAppStore } from "../../store/app-store";
import { defaultSettings, type AppSettings } from "../../types";
import {
  SettingsBasicCard,
  SettingsInlineState,
  SettingsModule,
  type SettingsModuleLevel,
} from "./SettingsModule";
import { TerminalPreferencesFields } from "./TerminalPreferencesFields";
import { AboutSettings } from "./AboutSettings";

const LazyOpenSshTools = lazy(() =>
  import("./OpenSshTools").then((module) => ({ default: module.OpenSshTools })),
);
const LazyProtocolSettings = lazy(() =>
  import("./ProtocolSettings").then((module) => ({
    default: module.ProtocolSettings,
  })),
);
const LazyProxySettings = lazy(() =>
  import("./ProxySettings").then((module) => ({
    default: module.ProxySettings,
  })),
);
const LazyConnectionBackupSettings = lazy(() =>
  import("./ConnectionBackupSettings").then((module) => ({
    default: module.ConnectionBackupSettings,
  })),
);
const LazyAiSettings = lazy(() =>
  import("./AiSettings").then((module) => ({ default: module.AiSettings })),
);
const LazyMcpSettings = lazy(() =>
  import("./McpSettings").then((module) => ({ default: module.McpSettings })),
);
const LazyPluginSettings = lazy(() =>
  import("./PluginSettings").then((module) => ({
    default: module.PluginSettings,
  })),
);
const LazyEncryptedSyncSettings = lazy(() =>
  import("./EncryptedSyncSettings").then((module) => ({
    default: module.EncryptedSyncSettings,
  })),
);
const LazyWebDavSyncSettings = lazy(() =>
  import("./WebDavSyncSettings").then((module) => ({
    default: module.WebDavSyncSettings,
  })),
);
const LazyTeamSettings = lazy(() =>
  import("./TeamSettings").then((module) => ({ default: module.TeamSettings })),
);
const LazyUpdateSettings = lazy(() =>
  import("./UpdateSettings").then((module) => ({
    default: module.UpdateSettings,
  })),
);
const LazyFeedbackSettings = lazy(() =>
  import("./FeedbackSettings").then((module) => ({
    default: module.FeedbackSettings,
  })),
);

type SettingsCategory =
  "basic" | "connection" | "automation" | "team" | "support";

const categories: Array<{
  id: SettingsCategory;
  label: string;
  description: string;
  icon: typeof Palette;
}> = [
  {
    id: "basic",
    label: "基础设置",
    description: "外观、终端和会话的常用选项",
    icon: Palette,
  },
  {
    id: "connection",
    label: "连接与安全",
    description: "OpenSSH、协议转发、代理和备份",
    icon: ShieldCheck,
  },
  {
    id: "automation",
    label: "自动化与集成",
    description: "自动化、AI、MCP 和插件能力",
    icon: Workflow,
  },
  {
    id: "team",
    label: "团队与云端",
    description: "加密同步、WebDAV 和团队工作区",
    icon: Cloud,
  },
  {
    id: "support",
    label: "关于与支持",
    description: "更新、反馈和脱敏诊断",
    icon: Info,
  },
];

type SettingsSearchTarget = {
  id: string;
  category: SettingsCategory;
  moduleId?: string;
  basicId?: string;
  label: string;
  description: string;
  keywords: string[];
};

const searchTargets: SettingsSearchTarget[] = [
  {
    id: "appearance",
    category: "basic",
    basicId: "appearance",
    label: "外观与主题",
    description: "主题、对比度和界面缩放",
    keywords: [
      "主题",
      "外观",
      "界面缩放",
      "缩放",
      "大小",
      "浅色",
      "深色",
      "高对比度",
      "system",
      "dark",
      "light",
    ],
  },
  {
    id: "terminal",
    category: "basic",
    basicId: "terminal",
    label: "终端显示",
    description: "字体、字号、行高、回滚、光标和配色",
    keywords: [
      "终端",
      "字体",
      "字号",
      "行高",
      "回滚",
      "光标",
      "配色",
      "terminal",
    ],
  },
  {
    id: "session",
    category: "basic",
    basicId: "session",
    label: "会话与文件",
    description: "监控刷新、历史、关闭确认和隐藏文件",
    keywords: [
      "会话",
      "文件",
      "监控",
      "历史",
      "隐藏文件",
      "关闭确认",
      "启动帮助",
    ],
  },
  {
    id: "openssh",
    category: "connection",
    moduleId: "openssh",
    label: "OpenSSH 配置与密钥",
    description: "导入 config、生成 Ed25519 和部署公钥",
    keywords: ["openssh", "ssh", "config", "ed25519", "密钥", "公钥", "导入"],
  },
  {
    id: "protocol",
    category: "connection",
    moduleId: "protocol",
    label: "高级协议与转发",
    description: "Agent 转发、X11、Mosh 和能力探测",
    keywords: ["agent", "x11", "mosh", "协议", "转发", "漫游"],
  },
  {
    id: "proxy",
    category: "connection",
    moduleId: "proxy",
    label: "代理与跳板机",
    description: "SOCKS5、HTTP 和 SSH Jump",
    keywords: ["代理", "跳板", "socks5", "http", "ssh jump", "proxy"],
  },
  {
    id: "backup",
    category: "connection",
    moduleId: "backup",
    label: "连接库备份",
    description: "安全导出、加密凭据和导入备份",
    keywords: ["连接", "备份", "导入", "导出", "argon2id", "aes", "凭据"],
  },
  {
    id: "automation",
    category: "automation",
    moduleId: "automation",
    label: "自动化与定时任务",
    description: "任务编排、Python、录制和计划执行",
    keywords: ["自动化", "定时", "任务", "python", "录制", "cron", "编排"],
  },
  {
    id: "ai",
    category: "automation",
    moduleId: "ai",
    label: "AI Provider",
    description: "兼容服务、模型和 API Key",
    keywords: ["ai", "provider", "模型", "api key"],
  },
  {
    id: "mcp",
    category: "automation",
    moduleId: "mcp",
    label: "MCP 服务",
    description: "客户端授权、工具权限、本地文件和审计",
    keywords: ["mcp", "broker", "客户端", "授权", "审批", "审计", "本地文件"],
  },
  {
    id: "plugin",
    category: "automation",
    moduleId: "plugin",
    label: "插件",
    description: "插件发现、信任、权限和沙箱审计",
    keywords: ["插件", "plugin", "权限", "沙箱", "签名", "审计"],
  },
  {
    id: "encrypted-sync",
    category: "team",
    moduleId: "encrypted-sync",
    label: "加密同步",
    description: "本地、iCloud、WebDAV、Git 包和 Touch ID",
    keywords: ["加密同步", "icloud", "git", "touch id", "导入", "导出"],
  },
  {
    id: "webdav",
    category: "team",
    moduleId: "webdav",
    label: "WebDAV 同步",
    description: "WebDAV 配置、同步范围、上传和下载",
    keywords: ["webdav", "云端", "同步", "上传", "下载"],
  },
  {
    id: "team",
    category: "team",
    moduleId: "team",
    label: "团队工作区与在线团队服务",
    description: "成员、设备、RBAC、Relay、共享和审计",
    keywords: [
      "团队",
      "工作区",
      "成员",
      "设备",
      "rbac",
      "relay",
      "共享",
      "审计",
    ],
  },
  {
    id: "update",
    category: "support",
    moduleId: "update",
    label: "软件更新",
    description: "检查、下载和安装签名更新",
    keywords: ["更新", "版本", "下载", "安装", "发布说明"],
  },
  {
    id: "feedback",
    category: "support",
    moduleId: "feedback",
    label: "反馈与诊断",
    description: "报告问题、功能建议和脱敏诊断",
    keywords: ["反馈", "诊断", "bug", "问题", "建议", "版本"],
  },
];

function sameSettings(left: AppSettings, right: AppSettings) {
  return JSON.stringify(left) === JSON.stringify(right);
}

function matchesSearch(target: SettingsSearchTarget, query: string) {
  const normalized = query.trim().toLocaleLowerCase();
  if (!normalized) return false;
  return [target.label, target.description, ...target.keywords].some((value) =>
    value.toLocaleLowerCase().includes(normalized),
  );
}

export default function SettingsModal() {
  const platform = usePlatformCapabilities();
  const {
    settingsOpen,
    settingsTarget,
    setSettingsOpen,
    settings,
    saveSettings,
    connections,
    refreshConnections,
    setError,
  } = useAppStore(
    useShallow((state) => ({
      settingsOpen: state.settingsOpen,
      settingsTarget: state.settingsTarget,
      setSettingsOpen: state.setSettingsOpen,
      settings: state.settings,
      saveSettings: state.saveSettings,
      connections: state.connections,
      refreshConnections: state.refreshConnections,
      setError: state.setError,
    })),
  );
  const [draft, setDraft] = useState(settings);
  const [activeCategory, setActiveCategory] =
    useState<SettingsCategory>("basic");
  const [mountedCategories, setMountedCategories] = useState<
    Set<SettingsCategory>
  >(() => new Set(["basic"]));
  const [expandedModules, setExpandedModules] = useState<Set<string>>(
    () => new Set(),
  );
  const [searchQuery, setSearchQuery] = useState("");
  const deferredSearchQuery = useDeferredValue(searchQuery);
  const [saving, setSaving] = useState(false);
  const [dirtyModules, setDirtyModules] = useState<Set<string>>(
    () => new Set(),
  );
  const wasOpen = useRef(false);

  useEffect(() => {
    if (!settingsOpen) return;
    const root = document.documentElement;
    root.dataset.interfaceScale = String(draft.interfaceScalePercent);
    root.style.setProperty(
      "--interface-scale",
      String(draft.interfaceScalePercent / 100),
    );
    return () => {
      root.dataset.interfaceScale = String(settings.interfaceScalePercent);
      root.style.setProperty(
        "--interface-scale",
        String(settings.interfaceScalePercent / 100),
      );
    };
  }, [
    draft.interfaceScalePercent,
    settings.interfaceScalePercent,
    settingsOpen,
  ]);

  useEffect(() => {
    if (settingsOpen && !wasOpen.current) {
      const initialCategory = settingsTarget?.category ?? "basic";
      setDraft(settings);
      setActiveCategory(initialCategory);
      setMountedCategories(new Set([initialCategory]));
      setExpandedModules(
        settingsTarget?.moduleId
          ? new Set([settingsTarget.moduleId])
          : new Set(),
      );
      setDirtyModules(new Set());
      setSearchQuery("");
      if (settingsTarget) {
        window.setTimeout(() => {
          const element = settingsTarget.moduleId
            ? document.getElementById(
                `settings-module-${settingsTarget.moduleId}-toggle`,
              )
            : document.getElementById(
                `settings-basic-${settingsTarget.basicId}`,
              );
          element?.focus();
          element?.scrollIntoView?.({ block: "center" });
        }, 0);
      }
    }
    wasOpen.current = settingsOpen;
  }, [settingsOpen, settings, settingsTarget]);

  const dirty = useMemo(
    () => !sameSettings(draft, settings),
    [draft, settings],
  );
  const searchResults = useMemo(
    () =>
      searchTargets.filter((target) =>
        matchesSearch(target, deferredSearchQuery),
      ),
    [deferredSearchQuery],
  );
  const panelCategories = useMemo(() => {
    const active = categories.find(
      (category) => category.id === activeCategory,
    );
    if (!active) return categories;
    return [
      active,
      ...categories.filter((category) => category.id !== activeCategory),
    ];
  }, [activeCategory]);
  const change = <K extends keyof AppSettings>(key: K, value: AppSettings[K]) =>
    setDraft((current) => ({ ...current, [key]: value }));
  const reportModuleDirty = useCallback((moduleId: string, value: boolean) => {
    setDirtyModules((current) => {
      if (current.has(moduleId) === value) return current;
      const next = new Set(current);
      if (value) next.add(moduleId);
      else next.delete(moduleId);
      return next;
    });
  }, []);

  const clearHistory = async () => {
    if (
      !confirm(
        `清空这台${platform.displayName}电脑上的全部命令历史？此操作无法撤销。`,
      )
    )
      return;
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

  const toggleModule = (moduleId: string, expanded: boolean) => {
    setExpandedModules((current) => {
      const next = new Set(current);
      if (expanded) {
        const category = searchTargets.find(
          (target) => target.moduleId === moduleId,
        )?.category;
        if (category) {
          searchTargets.forEach((target) => {
            if (target.category === category && target.moduleId) {
              next.delete(target.moduleId);
            }
          });
        }
        next.add(moduleId);
      } else {
        next.delete(moduleId);
      }
      return next;
    });
  };

  const moveCategory = (index: number, direction: -1 | 1) => {
    const nextIndex =
      (index + direction + categories.length) % categories.length;
    const nextCategory = categories[nextIndex];
    selectCategory(nextCategory.id);
    document.getElementById(`settings-category-${nextCategory.id}`)?.focus();
  };

  const openSearchTarget = (target: SettingsSearchTarget) => {
    selectCategory(target.category);
    if (target.moduleId) toggleModule(target.moduleId, true);
    setSearchQuery("");
    window.setTimeout(() => {
      const element = target.moduleId
        ? document.getElementById(`settings-module-${target.moduleId}-toggle`)
        : document.getElementById(`settings-basic-${target.basicId}`);
      element?.focus();
      element?.scrollIntoView?.({ block: "center" });
    }, 0);
  };

  const requestClose = () => {
    if (saving) return;
    if (
      (dirty || dirtyModules.size > 0) &&
      !confirm("设置中有尚未保存的修改，确定放弃并关闭吗？")
    )
      return;
    setSettingsOpen(false);
  };
  const discardAndClose = () => {
    if (!saving) requestClose();
  };
  const openAutomationCenter = () => {
    if (saving) return;
    if (
      (dirty || dirtyModules.size > 0) &&
      !confirm("设置中有尚未保存的修改，确定放弃并打开自动化中心吗？")
    )
      return;
    setSettingsOpen(false);
    window.dispatchEvent(new Event("cnshell-open-automation-center"));
  };
  const persistSettings = async () => {
    if (!dirty || saving) return;
    setSaving(true);
    try {
      await saveSettings(draft);
    } catch (error) {
      setError(errorMessage(error));
    } finally {
      setSaving(false);
    }
  };

  const restoreBasicDefaults = () => {
    if (
      !confirm(
        "恢复主题、终端、监控和常用会话设置的默认值？连接专属终端配置、凭据和高级模块配置不会被清除。",
      )
    )
      return;
    setDraft((current) => ({
      ...current,
      theme: defaultSettings.theme,
      interfaceScalePercent: defaultSettings.interfaceScalePercent,
      monitorIntervalMs: defaultSettings.monitorIntervalMs,
      rememberCommandHistory: defaultSettings.rememberCommandHistory,
      confirmCloseActiveSession: defaultSettings.confirmCloseActiveSession,
      showHiddenFiles: defaultSettings.showHiddenFiles,
      showWelcomeHelp: defaultSettings.showWelcomeHelp,
      terminal: { ...defaultSettings.terminal },
    }));
  };

  if (!settingsOpen) return null;

  const unsavedLabels = [
    ...(dirty ? ["基础设置"] : []),
    ...searchTargets
      .filter((target) => target.moduleId && dirtyModules.has(target.moduleId))
      .map((target) => target.label),
  ];

  const module = (
    id: string,
    title: string,
    description: string,
    icon: typeof Palette,
    help: string,
    content: React.ReactNode,
    level: SettingsModuleLevel = "advanced",
  ) => (
    <SettingsModule
      id={id}
      title={title}
      description={description}
      icon={icon}
      help={help}
      level={level}
      expanded={expandedModules.has(id)}
      onExpandedChange={(expanded) => toggleModule(id, expanded)}
      dirty={dirtyModules.has(id)}
      onDirtyChange={reportModuleDirty}
    >
      {content}
    </SettingsModule>
  );

  const renderBasic = () => (
    <div className="settings-panel-content">
      <SettingsBasicCard
        id="appearance"
        title="外观"
        icon={Moon}
        help="跟随系统会自动使用操作系统的深浅色外观；界面缩放会立即预览，保存后应用到后续启动。"
      >
        <label>
          <span>主题</span>
          <select
            value={draft.theme}
            onChange={(event) =>
              change("theme", event.target.value as AppSettings["theme"])
            }
          >
            <option value="system">跟随系统</option>
            <option value="dark">深色</option>
            <option value="light">浅色</option>
            <option value="highContrast">高对比度</option>
          </select>
        </label>
        <label>
          <span>界面缩放</span>
          <select
            value={draft.interfaceScalePercent}
            onChange={(event) =>
              change("interfaceScalePercent", Number(event.target.value))
            }
          >
            <option value={90}>90% · 显示更多内容</option>
            <option value={100}>100% · 默认</option>
            <option value={110}>110% · 较大</option>
            <option value={125}>125% · 最大</option>
          </select>
          <small>当前窗口会实时预览。</small>
        </label>
      </SettingsBasicCard>
      <SettingsBasicCard
        id="terminal"
        title="终端"
        icon={TerminalSquare}
        help="这些是全局终端偏好。已在某个连接中设置的专属终端偏好不会在这里被覆盖。"
      >
        <TerminalPreferencesFields
          idPrefix="global-terminal"
          value={draft.terminal}
          onChange={(terminal) => change("terminal", terminal)}
        />
      </SettingsBasicCard>
      <SettingsBasicCard
        id="session"
        title="会话与文件"
        icon={Settings2}
        help="监控刷新间隔越短，远端命令执行越频繁。命令历史只保存通过敏感命令过滤的内容。"
      >
        <label>
          <span>监控刷新</span>
          <select
            value={draft.monitorIntervalMs}
            onChange={(event) =>
              change("monitorIntervalMs", Number(event.target.value))
            }
          >
            <option value={1000}>1 秒</option>
            <option value={2000}>2 秒（推荐）</option>
            <option value={5000}>5 秒</option>
          </select>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.rememberCommandHistory}
            onChange={(event) =>
              change("rememberCommandHistory", event.target.checked)
            }
          />
          <span>保存非敏感命令历史</span>
        </label>
        <button
          className="button secondary"
          onClick={() => void clearHistory()}
        >
          清空全部命令历史
        </button>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.confirmCloseActiveSession}
            onChange={(event) =>
              change("confirmCloseActiveSession", event.target.checked)
            }
          />
          <span>关闭活动会话前确认</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.showHiddenFiles}
            onChange={(event) =>
              change("showHiddenFiles", event.target.checked)
            }
          />
          <span>显示远端隐藏文件</span>
        </label>
        <label className="check-row">
          <input
            type="checkbox"
            checked={draft.showWelcomeHelp}
            onChange={(event) =>
              change("showWelcomeHelp", event.target.checked)
            }
          />
          <span>启动时显示使用帮助</span>
        </label>
      </SettingsBasicCard>
    </div>
  );

  const renderConnection = () => (
    <div className="settings-panel-content settings-module-list">
      {module(
        "openssh",
        "OpenSSH 配置与密钥",
        "导入 config、生成 Ed25519 密钥并部署公钥",
        FileKey2,
        "私钥内容不会写入 CNshell 数据库或日志。部署前请核对目标连接和公钥指纹。",
        <LazyOpenSshTools
          connections={connections}
          onChanged={refreshConnections}
          onError={setError}
        />,
      )}
      {module(
        "protocol",
        "高级协议与转发",
        "Agent 转发、X11、Mosh 和本机能力探测",
        Network,
        "Agent 与 X11 转发会把本机能力暴露给远端，只应对完全可信的服务器启用；Mosh 需要客户端和服务端支持。",
        <LazyProtocolSettings connections={connections} onError={setError} />,
        "danger",
      )}
      {module(
        "proxy",
        "代理与跳板机",
        "配置 SOCKS5、HTTP 或 SSH Jump 访问路径",
        KeyRound,
        "代理密码保存在系统凭据库。SSH 跳板应选择已有且可信的 SSH 连接。",
        <LazyProxySettings connections={connections} onError={setError} />,
      )}
      {module(
        "backup",
        "连接库备份",
        "安全导出、加密导出凭据或导入备份",
        ShieldAlert,
        "普通备份不含密码；包含凭据的备份必须使用至少 8 位口令加密，遗失口令后无法恢复。",
        <LazyConnectionBackupSettings
          onChanged={refreshConnections}
          onError={setError}
        />,
        "danger",
      )}
    </div>
  );

  const renderAutomation = () => (
    <div className="settings-panel-content settings-module-list">
      {module(
        "automation",
        "自动化与定时任务",
        "编排命令、传输、Python、录制和计划任务",
        Braces,
        "自动化已移到独立中心。运行前会显示预览并要求确认，请先在非生产连接验证脚本和时间表达式。",
        <div className="automation-settings-entry">
          <p>
            任务编排、定时计划、Python、操作录制和运行记录集中在独立工作区中。
          </p>
          <button className="button primary" onClick={openAutomationCenter}>
            <Workflow size={14} /> 打开自动化中心
          </button>
        </div>,
        "danger",
      )}
      {module(
        "ai",
        "AI Provider",
        "配置兼容服务、模型和 API Key",
        Sparkles,
        "API Key 保存在系统凭据库；实际请求从终端工具栏进入，并始终先生成脱敏预览。",
        <LazyAiSettings onError={setError} />,
      )}
      {module(
        "mcp",
        "MCP 服务",
        "管理客户端授权、工具权限、本地文件和审计",
        Bot,
        "MCP 默认仅监听本机。敏感操作必须经过明确授权或审批，持久规则可随时撤销。",
        <LazyMcpSettings connections={connections} onError={setError} />,
        "danger",
      )}
      {module(
        "plugin",
        "插件",
        "管理插件信任、权限、沙箱和审计记录",
        Blocks,
        "只安装来源可信且签名验证通过的插件。敏感权限默认不授予，启用前应查看权限报告。",
        <LazyPluginSettings connections={connections} onError={setError} />,
        "danger",
      )}
    </div>
  );

  const renderTeam = () => (
    <div className="settings-panel-content settings-module-list">
      {module(
        "encrypted-sync",
        "加密同步",
        "创建或导入本地、iCloud、WebDAV、Git 加密包",
        CloudCog,
        "同步包在本机加密后写入所选目录。包含凭据时需要额外确认，口令不会自动上传。",
        <LazyEncryptedSyncSettings onError={setError} />,
        "danger",
      )}
      {module(
        "webdav",
        "WebDAV 同步",
        "配置服务器、同步范围、上传和下载",
        Cloud,
        "推荐使用 HTTPS。启动同步默认关闭；只有明确保存独立同步口令后才会在启动时访问远端。",
        <LazyWebDavSyncSettings onError={setError} />,
        "danger",
      )}
      {module(
        "team",
        "团队工作区与在线团队服务",
        "管理成员、设备、角色、Relay、共享和审计",
        Users,
        "角色与设备密钥决定共享权限。移除成员或撤销设备会轮换密钥 epoch，执行前请确认对象。",
        <LazyTeamSettings
          connections={connections}
          onConnectionImported={refreshConnections}
          onError={setError}
        />,
        "danger",
      )}
    </div>
  );

  const renderSupport = () => (
    <div className="settings-panel-content settings-module-list">
      <AboutSettings onError={setError} />
      {module(
        "update",
        "软件更新",
        "检查、下载并安装签名更新",
        DownloadCloud,
        "候选版可能没有正式更新通道。正式包只从签名 HTTPS endpoint 下载更新。",
        <LazyUpdateSettings onError={setError} />,
        "standard",
      )}
      {module(
        "feedback",
        "反馈与诊断",
        "报告问题、提出建议并导出脱敏诊断",
        MessageSquareText,
        "诊断包不会包含主机、用户名、路径或命令，也不会自动上传；导出后仍建议在发送前自行检查。",
        <LazyFeedbackSettings onError={setError} />,
        "standard",
      )}
    </div>
  );

  const renderCategory = (category: SettingsCategory) => {
    switch (category) {
      case "basic":
        return renderBasic();
      case "connection":
        return renderConnection();
      case "automation":
        return renderAutomation();
      case "team":
        return renderTeam();
      case "support":
        return renderSupport();
    }
  };

  return (
    <Modal
      title="设置"
      onClose={requestClose}
      wide
      bodyClassName="settings-modal-body"
    >
      <div className="settings-shell">
        <div className="settings-layout">
          <nav
            className="settings-navigation"
            aria-label="设置分类"
            role="tablist"
            aria-orientation="vertical"
          >
            <div className="settings-navigation-heading">设置</div>
            {categories.map((category, index) => {
              const Icon = category.icon;
              return (
                <button
                  key={category.id}
                  id={`settings-category-${category.id}`}
                  type="button"
                  role="tab"
                  aria-current={
                    activeCategory === category.id ? "page" : undefined
                  }
                  aria-selected={activeCategory === category.id}
                  aria-controls={`settings-panel-${category.id}`}
                  tabIndex={activeCategory === category.id ? 0 : -1}
                  className={`settings-nav-button${activeCategory === category.id ? " active" : ""}`}
                  onClick={() => selectCategory(category.id)}
                  onKeyDown={(event) => {
                    if (
                      event.key === "ArrowDown" ||
                      event.key === "ArrowRight"
                    ) {
                      event.preventDefault();
                      moveCategory(index, 1);
                    } else if (
                      event.key === "ArrowUp" ||
                      event.key === "ArrowLeft"
                    ) {
                      event.preventDefault();
                      moveCategory(index, -1);
                    } else if (event.key === "Home") {
                      event.preventDefault();
                      selectCategory(categories[0].id);
                      document
                        .getElementById(`settings-category-${categories[0].id}`)
                        ?.focus();
                    } else if (event.key === "End") {
                      event.preventDefault();
                      const last = categories.at(-1)!;
                      selectCategory(last.id);
                      document
                        .getElementById(`settings-category-${last.id}`)
                        ?.focus();
                    }
                  }}
                >
                  <Icon size={16} />
                  <span>{category.label}</span>
                </button>
              );
            })}
            <div className="settings-navigation-hint">
              <Sparkles size={14} />
              高级模块展开时才加载
            </div>
          </nav>
          <main className="settings-main">
            <div className="settings-search-region">
              <div className="settings-search-box">
                <Search size={15} />
                <input
                  data-modal-initial-focus
                  type="search"
                  aria-label="搜索设置"
                  placeholder="搜索设置，例如：Mosh、主题、MCP"
                  value={searchQuery}
                  onChange={(event) => setSearchQuery(event.target.value)}
                  onKeyDown={(event) => {
                    if (event.key === "Escape" && searchQuery) {
                      event.preventDefault();
                      event.stopPropagation();
                      setSearchQuery("");
                    }
                    if (event.key === "ArrowDown" && searchResults[0]) {
                      event.preventDefault();
                      document
                        .getElementById(
                          `settings-search-result-${searchResults[0].id}`,
                        )
                        ?.focus();
                    }
                  }}
                />
              </div>
              {searchQuery.trim() && (
                <div
                  className="settings-search-results"
                  role="region"
                  aria-label="设置搜索结果"
                >
                  {searchResults.length ? (
                    searchResults.map((target, index) => (
                      <button
                        key={target.id}
                        id={`settings-search-result-${target.id}`}
                        type="button"
                        onClick={() => openSearchTarget(target)}
                        onKeyDown={(event) => {
                          if (
                            event.key === "ArrowDown" ||
                            event.key === "ArrowUp"
                          ) {
                            event.preventDefault();
                            const direction =
                              event.key === "ArrowDown" ? 1 : -1;
                            const next =
                              (index + direction + searchResults.length) %
                              searchResults.length;
                            document
                              .getElementById(
                                `settings-search-result-${searchResults[next].id}`,
                              )
                              ?.focus();
                          }
                        }}
                      >
                        <span>
                          <strong>{target.label}</strong>
                          <small>{target.description}</small>
                        </span>
                        <em>
                          {
                            categories.find(
                              (category) => category.id === target.category,
                            )?.label
                          }
                        </em>
                      </button>
                    ))
                  ) : (
                    <SettingsInlineState
                      status="empty"
                      message="没有匹配的设置"
                      detail="请尝试“主题”“MCP”或“备份”等关键词。"
                    />
                  )}
                </div>
              )}
            </div>
            <div className="settings-panels">
              {panelCategories.map((category) => {
                if (!mountedCategories.has(category.id)) return null;
                const active = activeCategory === category.id;
                return (
                  <section
                    key={category.id}
                    id={`settings-panel-${category.id}`}
                    role={active ? "tabpanel" : undefined}
                    aria-labelledby={
                      active ? `settings-category-${category.id}` : undefined
                    }
                    aria-hidden={active ? undefined : true}
                    inert={active ? undefined : true}
                    hidden={!active}
                    className="settings-panel"
                  >
                    <header className="settings-category-header">
                      <div>
                        <h2 tabIndex={-1}>{category.label}</h2>
                        <p>{category.description}</p>
                      </div>
                      <div className="settings-category-actions">
                        {category.id === "basic" && (
                          <>
                            <span className="settings-badge">
                              <Radio size={12} />
                              常用
                            </span>
                            <button
                              type="button"
                              className="settings-reset-button"
                              onClick={restoreBasicDefaults}
                            >
                              <RotateCcw size={14} />
                              恢复基础默认值
                            </button>
                          </>
                        )}
                      </div>
                    </header>
                    {renderCategory(category.id)}
                  </section>
                );
              })}
            </div>
          </main>
        </div>
        <footer className="settings-footer">
          <div
            className="settings-footer-status"
            role="status"
            aria-live="polite"
          >
            {unsavedLabels.length ? (
              <>
                <span className="settings-dirty-dot" />
                {unsavedLabels.join("、")}有未保存的修改
              </>
            ) : activeCategory === "basic" ? (
              "基础设置已保存"
            ) : (
              "此页模块分别保存"
            )}
          </div>
          <div className="form-actions">
            <button
              className="button secondary"
              disabled={saving}
              onClick={discardAndClose}
            >
              {dirty || dirtyModules.size > 0 ? "取消" : "完成"}
            </button>
            {(activeCategory === "basic" || dirty) && (
              <button
                className="button primary"
                disabled={saving || !dirty}
                onClick={() => void persistSettings()}
              >
                <Save size={14} />
                {saving ? "保存中…" : "保存基础设置"}
              </button>
            )}
          </div>
        </footer>
      </div>
    </Modal>
  );
}
