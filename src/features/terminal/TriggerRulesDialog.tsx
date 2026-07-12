import { Bell, Check, Highlighter, Plus, Trash2 } from "lucide-react";
import { useState } from "react";
import { Modal } from "../../components/Modal";
import { IconButton } from "../../components/IconButton";
import type { TerminalSession } from "../../types";
import { workspaceRuntime } from "../../lib/workspace-runtime";
import {
  defaultTriggerConfig,
  ensureNotificationPermission,
  loadTriggerConfig,
  saveTriggerConfig,
  type TriggerConfig,
  type TriggerRule,
  validateTriggerPattern,
} from "./terminal-triggers";

const emptyRule = (): TriggerRule => ({
  id: crypto.randomUUID(),
  name: "",
  pattern: "",
  enabled: true,
  caseSensitive: false,
  foreground: "#ffffff",
  background: "#7f1d36",
  bold: true,
  notify: false,
  recordEvent: true,
  cooldownSeconds: 30,
});

export function TriggerRulesDialog({
  session,
  onClose,
  onError,
}: {
  session: TerminalSession;
  onClose: () => void;
  onError: (message: string) => void;
}) {
  const [config, setConfig] = useState<TriggerConfig>(loadTriggerConfig);
  const [editing, setEditing] = useState<TriggerRule | null>(null);
  const [error, setError] = useState<string | null>(null);
  const events = workspaceRuntime.triggerEventsBySession.get(session.id) ?? [];
  const commit = (next: TriggerConfig) => {
    setConfig(next);
    saveTriggerConfig(next);
  };
  const toggleNotifications = async (enabled: boolean) => {
    if (enabled && !(await ensureNotificationPermission())) {
      onError("macOS 通知权限未授权，请在系统设置中允许 CNshell 通知");
      return;
    }
    commit({ ...config, notificationsEnabled: enabled });
  };
  const saveRule = () => {
    if (!editing) return;
    const problem = validateTriggerPattern(editing.pattern);
    if (!editing.name.trim()) {
      setError("请输入规则名称");
      return;
    }
    if (problem) {
      setError(problem);
      return;
    }
    commit({
      ...config,
      rules: [
        ...config.rules.filter((rule) => rule.id !== editing.id),
        { ...editing, name: editing.name.trim() },
      ],
    });
    setEditing(null);
    setError(null);
  };
  return (
    <Modal title={`${session.title} · 高亮与通知`} onClose={onClose} wide>
      <div className="trigger-dialog">
    <section className="trigger-options">
      <label className="check-row">
        <input
          type="checkbox"
          checked={config.enforceContrast}
          onChange={(event) =>
            commit({ ...config, enforceContrast: event.target.checked })
          }
        />
        <span>自动保证高亮文字至少 4.5:1 对比度</span>
      </label>
      <label className="check-row">
        <input
          type="checkbox"
          checked={config.enhancedCursor}
          onChange={(event) =>
            commit({ ...config, enhancedCursor: event.target.checked })
          }
        />
        <span>增强块状光标</span>
      </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={config.notificationsEnabled}
              onChange={(event) =>
                void toggleNotifications(event.target.checked)
              }
            />
            <span>允许此功能发送 macOS 通知</span>
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={config.bellNotifications}
              disabled={!config.notificationsEnabled}
              onChange={(event) =>
                commit({ ...config, bellNotifications: event.target.checked })
              }
            />
            <span>终端 Bell</span>
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={config.backgroundNotifications}
              disabled={!config.notificationsEnabled}
              onChange={(event) =>
                commit({
                  ...config,
                  backgroundNotifications: event.target.checked,
                })
              }
            />
            <span>后台标签活动</span>
          </label>
          <label className="check-row">
            <input
              type="checkbox"
              checked={config.longTaskNotifications}
              disabled={!config.notificationsEnabled}
              onChange={(event) =>
                commit({
                  ...config,
                  longTaskNotifications: event.target.checked,
                })
              }
            />
            <span>长任务完成</span>
          </label>
          <label className="trigger-seconds">
            <span>阈值</span>
            <input
              type="number"
              min={5}
              max={3600}
              value={config.longTaskSeconds}
              onChange={(event) =>
                commit({
                  ...config,
                  longTaskSeconds: Math.min(
                    3600,
                    Math.max(5, Number(event.target.value) || 10),
                  ),
                })
              }
            />
            <small>秒</small>
          </label>
        </section>
        <div className="section-heading">
          <div>
            <h3>匹配规则</h3>
            <p>每行最多扫描 4096 字符，复杂正则会被拒绝。</p>
          </div>
          <button
            className="button secondary"
            onClick={() => setEditing(emptyRule())}
          >
            <Plus size={14} />
            新建规则
          </button>
        </div>
        <div className="trigger-rules">
          {config.rules.map((rule) => (
            <div key={rule.id} className="trigger-rule">
              <input
                type="checkbox"
                checked={rule.enabled}
                aria-label={`启用 ${rule.name}`}
                onChange={(event) =>
                  commit({
                    ...config,
                    rules: config.rules.map((item) =>
                      item.id === rule.id
                        ? { ...item, enabled: event.target.checked }
                        : item,
                    ),
                  })
                }
              />
              <i
                style={{ background: rule.background, color: rule.foreground }}
              >
                Aa
              </i>
              <button
                className="trigger-rule-main"
                onClick={() => setEditing({ ...rule })}
              >
                <strong>{rule.name}</strong>
                <code>{rule.pattern}</code>
              </button>
              <span>
                {rule.notify && <Bell size={13} />}{" "}
                {rule.recordEvent ? "记录" : ""}
              </span>
              {!rule.builtIn && (
                <IconButton
                  icon={Trash2}
                  label={`删除规则 ${rule.name}`}
                  onClick={() =>
                    commit({
                      ...config,
                      rules: config.rules.filter((item) => item.id !== rule.id),
                    })
                  }
                />
              )}
            </div>
          ))}
        </div>
        {editing && (
          <div className="trigger-editor">
            <label>
              <span>名称</span>
              <input
                value={editing.name}
                onChange={(event) =>
                  setEditing({ ...editing, name: event.target.value })
                }
              />
            </label>
            <label className="span-2">
              <span>正则表达式</span>
              <input
                value={editing.pattern}
                spellCheck={false}
                onChange={(event) =>
                  setEditing({ ...editing, pattern: event.target.value })
                }
              />
            </label>
            <label>
              <span>前景色</span>
              <input
                type="color"
                value={editing.foreground}
                onChange={(event) =>
                  setEditing({ ...editing, foreground: event.target.value })
                }
              />
            </label>
            <label>
              <span>背景色</span>
              <input
                type="color"
                value={editing.background}
                onChange={(event) =>
                  setEditing({ ...editing, background: event.target.value })
                }
              />
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={editing.bold}
                onChange={(event) =>
                  setEditing({ ...editing, bold: event.target.checked })
                }
              />
              <span>粗体</span>
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={editing.caseSensitive}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    caseSensitive: event.target.checked,
                  })
                }
              />
              <span>区分大小写</span>
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={editing.notify}
                onChange={(event) =>
                  setEditing({ ...editing, notify: event.target.checked })
                }
              />
              <span>匹配后通知</span>
            </label>
            <label className="check-row">
              <input
                type="checkbox"
                checked={editing.recordEvent}
                onChange={(event) =>
                  setEditing({ ...editing, recordEvent: event.target.checked })
                }
              />
              <span>记录事件</span>
            </label>
            <label>
              <span>通知冷却（秒）</span>
              <input
                type="number"
                min={1}
                max={3600}
                value={editing.cooldownSeconds}
                onChange={(event) =>
                  setEditing({
                    ...editing,
                    cooldownSeconds: Number(event.target.value),
                  })
                }
              />
            </label>
            {error && <div className="inline-error span-2">{error}</div>}
            <footer className="form-actions span-2">
              <button
                className="button secondary"
                onClick={() => setEditing(null)}
              >
                取消
              </button>
              <button className="button primary" onClick={saveRule}>
                <Check size={14} />
                保存规则
              </button>
            </footer>
          </div>
        )}
        <div className="trigger-events">
          <h3>
            <Highlighter size={14} />
            最近匹配事件
          </h3>
          {events.length ? (
            <div>
              {events.slice(0, 20).map((event) => (
                <p key={event.id}>
                  <time>{new Date(event.timestamp).toLocaleTimeString()}</time>
                  <strong>{event.ruleName}</strong>
                  <code>{event.text}</code>
                </p>
              ))}
            </div>
          ) : (
            <span>本次会话暂无记录事件</span>
          )}
        </div>
        <footer className="form-actions">
          <button
            className="button secondary"
            onClick={() => {
              commit(structuredClone(defaultTriggerConfig));
              setEditing(null);
            }}
          >
            恢复默认
          </button>
          <button className="button primary" onClick={onClose}>
            完成
          </button>
        </footer>
      </div>
    </Modal>
  );
}
