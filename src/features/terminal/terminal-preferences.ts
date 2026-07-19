import type { ITheme } from "@xterm/xterm";
import type { AppSettings, TerminalPreferences } from "../../types";

export const terminalFontFamilies: Record<TerminalPreferences["fontFamily"], string> = {
  system: "SFMono-Regular, Menlo, Monaco, Consolas, monospace",
  menlo: "Menlo, Monaco, Consolas, monospace",
  monaco: "Monaco, Menlo, Consolas, monospace",
  courier: '"Courier New", Courier, monospace',
};

export const terminalThemes: Record<TerminalPreferences["colorScheme"], ITheme> = {
  cnshell: {
    background: "#07101d", foreground: "#dce6f4", cursor: "#4ade80", cursorAccent: "#07101d", selectionBackground: "#315b8e88",
    black: "#111827", red: "#fb7185", green: "#4ade80", yellow: "#facc15", blue: "#60a5fa", magenta: "#c084fc", cyan: "#22d3ee", white: "#e5e7eb", brightBlack: "#64748b",
  },
  classic: {
    background: "#000000", foreground: "#f2f2f2", cursor: "#ffffff", cursorAccent: "#000000", selectionBackground: "#ffffff44",
    black: "#000000", red: "#cd3131", green: "#0dbc79", yellow: "#e5e510", blue: "#2472c8", magenta: "#bc3fbc", cyan: "#11a8cd", white: "#e5e5e5", brightBlack: "#666666",
  },
  solarizedDark: {
    background: "#002b36", foreground: "#93a1a1", cursor: "#fdf6e3", cursorAccent: "#002b36", selectionBackground: "#586e7555",
    black: "#073642", red: "#dc322f", green: "#859900", yellow: "#b58900", blue: "#268bd2", magenta: "#d33682", cyan: "#2aa198", white: "#eee8d5", brightBlack: "#657b83",
  },
  light: {
    background: "#ffffff", foreground: "#1f2937", cursor: "#111827", cursorAccent: "#ffffff", selectionBackground: "#2563eb33",
    black: "#111827", red: "#b91c1c", green: "#047857", yellow: "#a16207", blue: "#1d4ed8", magenta: "#a21caf", cyan: "#0e7490", white: "#e5e7eb", brightBlack: "#6b7280",
  },
};

export function resolveTerminalPreferences(settings: AppSettings, connectionId: string): TerminalPreferences {
  return settings.terminalOverrides[connectionId] ?? settings.terminal;
}

export function resolveTerminalTheme(
  settings: Pick<AppSettings, "theme">,
  preferences: TerminalPreferences,
  systemPrefersDark: boolean,
): ITheme {
  const followsLightAppTheme =
    preferences.colorScheme === "cnshell" &&
    (settings.theme === "light" ||
      (settings.theme === "system" && !systemPrefersDark));

  return terminalThemes[followsLightAppTheme ? "light" : preferences.colorScheme];
}

export function withTerminalFontSize(settings: AppSettings, connectionId: string, fontSize: number): AppSettings {
  const normalized = Math.min(24, Math.max(10, Math.round(fontSize)));
  const override = settings.terminalOverrides[connectionId];
  return override
    ? { ...settings, terminalOverrides: { ...settings.terminalOverrides, [connectionId]: { ...override, fontSize: normalized } } }
    : { ...settings, terminal: { ...settings.terminal, fontSize: normalized } };
}
