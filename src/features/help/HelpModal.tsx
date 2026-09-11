import { useDeferredValue, useState } from "react";
import { useShallow } from "zustand/react/shallow";
import {
  FileUp,
  Plus,
  Search,
  ShieldCheck,
  TerminalSquare,
} from "lucide-react";
import { Modal } from "../../components/Modal";
import { useAppStore } from "../../store/app-store";
import { usePlatformCapabilities } from "../../lib/platform";
import "./HelpModal.css";

export default function HelpModal() {
  const platform = usePlatformCapabilities();
  const [query, setQuery] = useState("");
  const normalizedQuery = useDeferredValue(query.trim().toLowerCase());
  const state = useAppStore(
    useShallow((item) => ({
      helpOpen: item.helpOpen,
      setHelpOpen: item.setHelpOpen,
      settings: item.settings,
      saveSettings: item.saveSettings,
      openConnectionEditor: item.openConnectionEditor,
    })),
  );
  if (!state.helpOpen) return null;
  const shortcut = (key: string) =>
    platform.shortcutModifier === "⌘"
      ? `⌘${key.replace("Shift+", "⇧")}`
      : `Ctrl+${key}`;
  const rows = [
    [shortcut("N"), "新建连接", "全局"],
    [shortcut("T"), "打开新终端", "终端"],
    [shortcut("W"), "关闭当前会话", "终端"],
    [shortcut("F"), "搜索当前终端输出", "终端"],
    [shortcut("Shift+F"), "跨标签搜索", "终端"],
    [shortcut("K"), "清空当前终端", "终端"],
    [shortcut("J"), "显示或隐藏底部工具", "工作区"],
    [`${shortcut("+")} / ${shortcut("-")}`, "调整终端字号", "终端"],
    [shortcut("0"), "恢复终端默认字号", "终端"],
    [shortcut("1…9"), "切换会话标签", "终端"],
    [shortcut("?"), "打开使用帮助", "全局"],
  ];
  const filtered = rows.filter((row) =>
    normalizedQuery
      ? row.some((value) => value.toLowerCase().includes(normalizedQuery))
      : true,
  );
  const close = () => state.setHelpOpen(false);

  return (
    <Modal title="CNshell 使用帮助" onClose={close} wide>
      <div className="help-content">
        <div className="help-hero">
          <TerminalSquare size={34} />
          <div>
            <h3>连接、操作、传输，一处完成</h3>
            <p>
              CNshell 使用{platform.credentialStoreName}
              保护凭据，并严格校验每台服务器的主机指纹。
            </p>
          </div>
          <button
            type="button"
            className="button primary"
            onClick={() => {
              close();
              state.openConnectionEditor();
            }}
          >
            <Plus size={14} />
            新建第一个连接
          </button>
        </div>
        <section className="help-shortcuts" aria-labelledby="shortcut-heading">
          <header>
            <div>
              <h3 id="shortcut-heading">快捷键</h3>
              <p>
                {filtered.length}/{rows.length} 项
              </p>
            </div>
            <label>
              <Search size={14} />
              <input
                data-modal-initial-focus
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="搜索操作或按键"
                aria-label="搜索帮助与快捷键"
              />
            </label>
          </header>
          {filtered.length ? (
            <div
              className="help-shortcut-table"
              role="table"
              aria-label="快捷键表"
            >
              <div role="row">
                <span role="columnheader">按键</span>
                <span role="columnheader">操作</span>
                <span role="columnheader">范围</span>
              </div>
              {filtered.map(([keys, action, area]) => (
                <div role="row" key={`${keys}-${action}`}>
                  <span role="cell">
                    <kbd>{keys}</kbd>
                  </span>
                  <span role="cell">{action}</span>
                  <span role="cell">{area}</span>
                </div>
              ))}
            </div>
          ) : (
            <p className="help-empty" role="status">
              没有匹配的快捷键
            </p>
          )}
        </section>
        <div className="help-grid">
          <Help
            icon={FileUp}
            title="远程文件"
            lines={[
              "终端右键菜单也可显示或隐藏工具面板",
              "双击文件夹进入，双击文本打开编辑器",
              "上传下载进入后台队列",
              "保存时检查远端冲突",
            ]}
          />
          <Help
            icon={Search}
            title="诊断连接"
            lines={[
              "连接测试区分 TCP、指纹和认证错误",
              "首次连接请在其他可信渠道核对指纹",
              "指纹变化时 CNshell 会阻止连接",
            ]}
          />
          <Help
            icon={ShieldCheck}
            title="隐私与安全"
            lines={[
              "密码不写入数据库和日志",
              "遥测默认关闭",
              "下载先写入 .cnshell-part",
              "RDP 已内置 FreeRDP 组件",
            ]}
          />
        </div>
        <footer className="help-footer">
          <label>
            <input
              type="checkbox"
              checked={!state.settings.showWelcomeHelp}
              onChange={(event) =>
                void state.saveSettings({
                  ...state.settings,
                  showWelcomeHelp: !event.target.checked,
                })
              }
            />
            不再自动显示
          </label>
          <button className="button secondary" onClick={close}>
            关闭
          </button>
        </footer>
      </div>
    </Modal>
  );
}

function Help({
  icon: Icon,
  title,
  lines,
}: {
  icon: typeof FileUp;
  title: string;
  lines: string[];
}) {
  return (
    <section>
      <h3>
        <Icon size={17} />
        {title}
      </h3>
      {lines.map((line) => (
        <p key={line}>{line}</p>
      ))}
    </section>
  );
}
