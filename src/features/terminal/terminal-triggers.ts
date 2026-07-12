import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";

export type TriggerRule = {
  id: string;
  name: string;
  pattern: string;
  enabled: boolean;
  caseSensitive: boolean;
  foreground: string;
  background: string;
  bold: boolean;
  notify: boolean;
  recordEvent: boolean;
  cooldownSeconds: number;
  builtIn?: boolean;
};
export type TriggerConfig = {
  rules: TriggerRule[];
  notificationsEnabled: boolean;
  bellNotifications: boolean;
  backgroundNotifications: boolean;
  longTaskNotifications: boolean;
  longTaskSeconds: number;
  enforceContrast: boolean;
  enhancedCursor: boolean;
};
export type TriggerMatch = {
  rule: TriggerRule;
  index: number;
  length: number;
  text: string;
};
export type TriggerEvent = {
  id: string;
  sessionId: string;
  ruleId: string;
  ruleName: string;
  text: string;
  timestamp: string;
};

const storageKey = "cnshell-terminal-triggers-v1";
export const defaultTriggerConfig: TriggerConfig = {
  notificationsEnabled: false,
  bellNotifications: false,
  backgroundNotifications: false,
  longTaskNotifications: false,
  longTaskSeconds: 10,
  enforceContrast: true,
  enhancedCursor: false,
  rules: [
    {
      id: "builtin-error",
      name: "错误",
      pattern: "\\b(error|failed|failure|fatal|panic)\\b",
      enabled: true,
      caseSensitive: false,
      foreground: "#fecdd3",
      background: "#7f1d36",
      bold: true,
      notify: false,
      recordEvent: true,
      cooldownSeconds: 30,
      builtIn: true,
    },
    {
      id: "builtin-warning",
      name: "警告",
      pattern: "\\b(warn|warning|deprecated)\\b",
      enabled: true,
      caseSensitive: false,
      foreground: "#fef3c7",
      background: "#854d0e",
      bold: true,
      notify: false,
      recordEvent: false,
      cooldownSeconds: 30,
      builtIn: true,
    },
    {
      id: "builtin-ipv4",
      name: "IPv4",
      pattern: "\\b[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\.[0-9]{1,3}\\b",
      enabled: false,
      caseSensitive: true,
      foreground: "#bae6fd",
      background: "#075985",
      bold: false,
      notify: false,
      recordEvent: false,
      cooldownSeconds: 60,
      builtIn: true,
    },
    {
      id: "builtin-ipv6",
      name: "IPv6",
      pattern: "\\b[0-9a-fA-F:]{2,39}\\b",
      enabled: false,
      caseSensitive: true,
      foreground: "#cffafe",
      background: "#155e75",
      bold: false,
      notify: false,
      recordEvent: false,
      cooldownSeconds: 60,
      builtIn: true,
    },
  ],
};

export function validateTriggerPattern(pattern: string): string | null {
  if (!pattern) return "正则表达式不能为空";
  if (pattern.length > 256) return "正则表达式不能超过 256 个字符";
  if (/\(\?[=!<:>]/.test(pattern) || /\\[1-9]|\\k</.test(pattern))
    return "为保证终端性能，不支持环视、非捕获组或反向引用";
  if (
    /\([^)]*(?:[+*]|\{\d+(?:,\d*)?\})[^)]*\)(?:[+*]|\{\d+(?:,\d*)?\})/.test(
      pattern,
    ) ||
    /\([^)]*\|[^)]*\)(?:[+*]|\{\d+(?:,\d*)?\})/.test(pattern) ||
    /(?:[+*]|\{\d+(?:,\d*)?\}){2}/.test(pattern)
  )
    return "正则包含可能造成性能问题的嵌套或重复量词";
  try {
    new RegExp(pattern, "u");
    return null;
  } catch (error) {
    return `正则表达式无效：${String(error)}`;
  }
}

export function loadTriggerConfig(): TriggerConfig {
  try {
    const stored = JSON.parse(
      localStorage.getItem(storageKey) ?? "null",
    ) as Partial<TriggerConfig> | null;
    if (!stored) return structuredClone(defaultTriggerConfig);
    const rules = Array.isArray(stored.rules)
      ? stored.rules
          .filter(
            (rule) =>
              rule &&
              typeof rule.pattern === "string" &&
              !validateTriggerPattern(rule.pattern),
          )
          .map(normalizeRule)
      : defaultTriggerConfig.rules;
    return { ...defaultTriggerConfig, ...stored, rules };
  } catch {
    return structuredClone(defaultTriggerConfig);
  }
}

export function saveTriggerConfig(config: TriggerConfig) {
  localStorage.setItem(storageKey, JSON.stringify(config));
  window.dispatchEvent(
    new CustomEvent("cnshell-trigger-config", { detail: config }),
  );
}

export function findTriggerMatches(
  line: string,
  rules: TriggerRule[],
  budgetMs = 5,
): TriggerMatch[] {
  const input = line.slice(0, 4096);
  const started = performance.now();
  const matches: TriggerMatch[] = [];
  for (const rule of rules) {
    if (!rule.enabled || validateTriggerPattern(rule.pattern)) continue;
    const expression = new RegExp(
      rule.pattern,
      rule.caseSensitive ? "gu" : "giu",
    );
    for (
      let match = expression.exec(input);
      match && matches.length < 100;
      match = expression.exec(input)
    ) {
      if (match[0].length) {
        matches.push({
          rule,
          index: match.index,
          length: match[0].length,
          text: match[0],
        });
      } else expression.lastIndex += 1;
      if (performance.now() - started > budgetMs) return matches;
    }
  }
  return matches;
}

export function terminalCellWidth(value: string): number {
  let width = 0;
  for (const character of value) {
    const point = character.codePointAt(0) ?? 0;
    if (point === 0 || point < 32 || (point >= 0x7f && point < 0xa0)) continue;
    width += isWide(point) ? 2 : 1;
  }
  return width;
}

export function accessibleRuleColors(
  foreground: string,
  background: string,
  enforce: boolean,
): { foreground: string; background: string } {
  if (!enforce || contrastRatio(foreground, background) >= 4.5)
    return { foreground, background };
  const black = "#000000";
  const white = "#ffffff";
  return {
    foreground:
      contrastRatio(white, background) >= contrastRatio(black, background)
        ? white
        : black,
    background,
  };
}

function contrastRatio(firstColor: string, secondColor: string) {
  const luminance = (color: string) => {
    const values = [1, 3, 5]
      .map((index) => parseInt(color.slice(index, index + 2), 16) / 255)
      .map((value) =>
        value <= 0.04045 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4,
      );
    return 0.2126 * values[0] + 0.7152 * values[1] + 0.0722 * values[2];
  };
  const first = luminance(firstColor);
  const second = luminance(secondColor);
  return (Math.max(first, second) + 0.05) / (Math.min(first, second) + 0.05);
}

export async function ensureNotificationPermission(): Promise<boolean> {
  if (await isPermissionGranted()) return true;
  return (await requestPermission()) === "granted";
}
export function notifyTerminal(title: string, body: string) {
  try {
    sendNotification({ title, body });
  } catch {
    /* notification failures never affect the session */
  }
}

function normalizeRule(value: Partial<TriggerRule>): TriggerRule {
  return {
    id: String(value.id ?? crypto.randomUUID()),
    name: String(value.name ?? "规则"),
    pattern: String(value.pattern ?? ""),
    enabled: value.enabled !== false,
    caseSensitive: Boolean(value.caseSensitive),
    foreground: validColor(value.foreground) ? value.foreground! : "#ffffff",
    background: validColor(value.background) ? value.background! : "#7f1d36",
    bold: Boolean(value.bold),
    notify: Boolean(value.notify),
    recordEvent: Boolean(value.recordEvent),
    cooldownSeconds: Math.min(
      3600,
      Math.max(1, Number(value.cooldownSeconds) || 30),
    ),
    builtIn: Boolean(value.builtIn),
  };
}
function validColor(value: unknown): value is string {
  return typeof value === "string" && /^#[0-9a-f]{6}$/i.test(value);
}
function isWide(point: number) {
  return (
    point >= 0x1100 &&
    (point <= 0x115f ||
      point === 0x2329 ||
      point === 0x232a ||
      (point >= 0x2e80 && point <= 0xa4cf && point !== 0x303f) ||
      (point >= 0xac00 && point <= 0xd7a3) ||
      (point >= 0xf900 && point <= 0xfaff) ||
      (point >= 0xfe10 && point <= 0xfe19) ||
      (point >= 0xfe30 && point <= 0xfe6f) ||
      (point >= 0xff00 && point <= 0xff60) ||
      (point >= 0xffe0 && point <= 0xffe6) ||
      (point >= 0x1f300 && point <= 0x1faff) ||
      (point >= 0x20000 && point <= 0x3fffd))
  );
}
