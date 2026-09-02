import { useEffect, useState } from "react";

export const PANEL_FONT_SIZE_MIN = 10;
export const PANEL_FONT_SIZE_MAX = 16;

export const panelFontSizeStorageKeys = {
  monitor: "cnshell-monitor-font-size",
  files: "cnshell-files-font-size",
} as const;

export function clampPanelFontSize(value: number, fallback = 11): number {
  const safeFallback = Number.isFinite(fallback) ? fallback : 11;
  const normalizedFallback = Math.round(Math.min(PANEL_FONT_SIZE_MAX, Math.max(PANEL_FONT_SIZE_MIN, safeFallback)));
  if (!Number.isFinite(value)) return normalizedFallback;
  return Math.round(Math.min(PANEL_FONT_SIZE_MAX, Math.max(PANEL_FONT_SIZE_MIN, value)));
}

export function readPanelFontSize(storageKey: string, fallback = 11): number {
  try {
    const stored = localStorage.getItem(storageKey);
    return stored === null ? clampPanelFontSize(fallback, fallback) : clampPanelFontSize(Number(stored), fallback);
  } catch {
    return clampPanelFontSize(fallback, fallback);
  }
}

export function usePanelFontSize(storageKey: string, fallback = 11) {
  const [size, setSize] = useState(() => readPanelFontSize(storageKey, fallback));
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, String(size));
    } catch {
      // Local storage can be disabled by a browser policy; the control still works for this session.
    }
  }, [size, storageKey]);
  return [size, (value: number) => setSize(clampPanelFontSize(value, fallback))] as const;
}
