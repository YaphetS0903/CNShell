import { useShallow } from "zustand/react/shallow";
import { Command, FileUp, Search, ShieldCheck, TerminalSquare } from "lucide-react";
import { Modal } from "../../components/Modal";
import { useAppStore } from "../../store/app-store";
import { usePlatformCapabilities } from "../../lib/platform";

export default function HelpModal() {
  const platform = usePlatformCapabilities();
  const { helpOpen, setHelpOpen, settings, saveSettings } = useAppStore(
    useShallow((state) => ({
      helpOpen: state.helpOpen,
      setHelpOpen: state.setHelpOpen,
      settings: state.settings,
      saveSettings: state.saveSettings,
    })),
  );
  if (!helpOpen) return null;
  const modifier = platform.shortcutModifier;
  const shortcut = (key: string) =>
    modifier === "⌘"
      ? `⌘${key.replace("Shift+", "⇧")}`
      : `Ctrl+${key}`;
  return (
    <Modal
      title="CNshell 使用帮助"
      onClose={() => setHelpOpen(false)}
      wide
    >
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
        </div>
        <div className="help-grid">
          <Help
            icon={Command}
            title="终端快捷键"
            lines={[
              `${shortcut("T")} 新终端`,
              `${shortcut("W")} 关闭会话`,
              `${shortcut("F")} 搜索输出`,
              `${shortcut("Shift+F")} 跨标签搜索`,
              `${shortcut("K")} 清屏`,
              `${shortcut("+")} / ${shortcut("-")} 调整终端字号`,
              `${shortcut("1…9")} 切换标签`,
            ]}
          />
          <Help
            icon={FileUp}
            title="底部工具与远程文件"
            lines={[
              `${shortcut("J")} 显示或隐藏底部工具面板`,
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
              checked={!settings.showWelcomeHelp}
              onChange={(event) =>
                void saveSettings({
                  ...settings,
                  showWelcomeHelp: !event.target.checked,
                })
              }
            />
            不再自动显示
          </label>
          <button
            className="button primary"
            onClick={() => setHelpOpen(false)}
          >
            知道了
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
  icon: typeof Command;
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
