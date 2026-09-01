import {
  CheckCircle2,
  ChevronDown,
  CircleAlert,
  CircleHelp,
  Inbox,
  LoaderCircle,
  RotateCw,
  type LucideIcon,
} from "lucide-react";
import {
  Component,
  Suspense,
  useEffect,
  useState,
  type ErrorInfo,
  type ReactNode,
} from "react";

export type SettingsModuleLevel = "standard" | "advanced" | "danger";

export function SettingsInlineState({
  status,
  message,
  detail,
  actionLabel,
  onAction,
}: {
  status: "loading" | "empty" | "error" | "success";
  message: string;
  detail?: string;
  actionLabel?: string;
  onAction?: () => void;
}) {
  const Icon = status === "loading"
    ? LoaderCircle
    : status === "empty"
      ? Inbox
      : status === "error"
        ? CircleAlert
        : CheckCircle2;

  return (
    <div
      className={`settings-inline-state ${status}`}
      role={status === "error" ? "alert" : "status"}
      aria-live={status === "error" ? "assertive" : "polite"}
    >
      <Icon size={18} className={status === "loading" ? "spin" : undefined} />
      <span>
        <strong>{message}</strong>
        {detail && <small>{detail}</small>}
      </span>
      {actionLabel && onAction && (
        <button type="button" className="button secondary" onClick={onAction}>
          <RotateCw size={14} />
          {actionLabel}
        </button>
      )}
    </div>
  );
}

class SettingsModuleErrorBoundary extends Component<
  { children: ReactNode; moduleTitle: string },
  { error: Error | null }
> {
  state: { error: Error | null } = { error: null };

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(`设置模块“${this.props.moduleTitle}”加载失败`, error, info);
  }

  render() {
    if (!this.state.error) return this.props.children;
    return (
      <SettingsInlineState
        status="error"
        message={`${this.props.moduleTitle}加载失败`}
        detail="模块没有修改任何设置。可重新载入应用后重试。"
        actionLabel="重新载入"
        onAction={() => window.location.reload()}
      />
    );
  }
}

export function SettingsModule({
  id,
  title,
  description,
  icon: Icon,
  level = "advanced",
  help,
  expanded,
  onExpandedChange,
  children,
}: {
  id: string;
  title: string;
  description: string;
  icon: LucideIcon;
  level?: SettingsModuleLevel;
  help: string;
  expanded: boolean;
  onExpandedChange: (expanded: boolean) => void;
  children: ReactNode;
}) {
  const [mounted, setMounted] = useState(expanded);
  const [helpOpen, setHelpOpen] = useState(false);
  const contentId = `settings-module-${id}-content`;
  const helpId = `settings-module-${id}-help`;
  const toggleId = `settings-module-${id}-toggle`;

  useEffect(() => {
    if (expanded) setMounted(true);
  }, [expanded]);

  return (
    <section
      className={`settings-module${expanded ? " expanded" : ""}`}
      data-settings-module={id}
    >
      <div className="settings-module-header">
        <button
          id={toggleId}
          type="button"
          className="settings-module-toggle"
          aria-expanded={expanded}
          aria-controls={contentId}
          onClick={() => onExpandedChange(!expanded)}
        >
          <span className="settings-module-icon"><Icon size={17} /></span>
          <span className="settings-module-copy">
            <span className="settings-module-title-row">
              <span className="settings-module-title">{title}</span>
              {level !== "standard" && (
                <span className={`settings-module-level ${level}`}>
                  {level === "danger" ? "敏感操作" : "高级"}
                </span>
              )}
            </span>
            <span className="settings-module-description">{description}</span>
          </span>
          <ChevronDown size={17} className="settings-module-chevron" />
        </button>
        <button
          type="button"
          className="settings-help-button"
          aria-label={`${helpOpen ? "收起" : "查看"}${title}帮助`}
          aria-expanded={helpOpen}
          aria-controls={helpId}
          title={`${title}帮助`}
          onClick={() => setHelpOpen((current) => !current)}
        >
          <CircleHelp size={16} />
        </button>
      </div>
      <p id={helpId} className="settings-module-help" hidden={!helpOpen}>
        <CircleHelp size={14} />
        <span>{help}</span>
      </p>
      <div
        id={contentId}
        className="settings-module-content"
        role="region"
        aria-labelledby={toggleId}
        hidden={!expanded}
      >
        {mounted && (
          <SettingsModuleErrorBoundary moduleTitle={title}>
            <Suspense
              fallback={(
                <SettingsInlineState
                  status="loading"
                  message={`正在加载${title}…`}
                  detail="首次展开时才加载，减少设置页启动开销。"
                />
              )}
            >
              {children}
            </Suspense>
          </SettingsModuleErrorBoundary>
        )}
      </div>
    </section>
  );
}

export function SettingsBasicCard({
  id,
  title,
  icon: Icon,
  help,
  children,
}: {
  id: string;
  title: string;
  icon: LucideIcon;
  help: string;
  children: ReactNode;
}) {
  const [helpOpen, setHelpOpen] = useState(false);
  const helpId = `settings-basic-${id}-help`;

  return (
    <section className="settings-card" data-settings-basic={id}>
      <div className="settings-card-heading">
        <h3 id={`settings-basic-${id}`} tabIndex={-1}><Icon size={16} />{title}</h3>
        <button
          type="button"
          className="settings-help-button"
          aria-label={`${helpOpen ? "收起" : "查看"}${title}帮助`}
          aria-expanded={helpOpen}
          aria-controls={helpId}
          title={`${title}帮助`}
          onClick={() => setHelpOpen((current) => !current)}
        >
          <CircleHelp size={16} />
        </button>
      </div>
      <p id={helpId} className="settings-module-help" hidden={!helpOpen}>
        <CircleHelp size={14} />
        <span>{help}</span>
      </p>
      {children}
    </section>
  );
}
