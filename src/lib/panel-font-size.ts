import { useEffect, useState } from "react";

export const PANEL_FONT_SIZE_MIN = 10;
export const PANEL_FONT_SIZE_MAX = 16;

export const panelFontSizeStorageKeys = {
  monitor: "cnshell-monitor-font-size",
  files: "cnshell-files-font-size",
} as const;

type DisplayMetrics = {
  screenWidth: number;
  screenHeight: number;
  devicePixelRatio: number;
};

type PanelFontSizeState = {
  size: number;
  automatic: boolean;
};

export function clampPanelFontSize(value: number, fallback = 11): number {
  const safeFallback = Number.isFinite(fallback) ? fallback : 11;
  const normalizedFallback = Math.round(Math.min(PANEL_FONT_SIZE_MAX, Math.max(PANEL_FONT_SIZE_MIN, safeFallback)));
  if (!Number.isFinite(value)) return normalizedFallback;
  return Math.round(Math.min(PANEL_FONT_SIZE_MAX, Math.max(PANEL_FONT_SIZE_MIN, value)));
}

export function readPanelFontSize(storageKey: string, fallback = 11): number {
  try {
    const stored = localStorage.getItem(storageKey);
    return stored === null || stored === "auto" ? automaticPanelFontSize(fallback) : clampPanelFontSize(Number(stored), fallback);
  } catch {
    return automaticPanelFontSize(fallback);
  }
}

export function automaticPanelFontSize(fallback = 11, metrics = currentDisplayMetrics()): number {
  const ratio = Number.isFinite(metrics.devicePixelRatio) && metrics.devicePixelRatio > 0 ? metrics.devicePixelRatio : 1;
  const physicalWidth = metrics.screenWidth * ratio;
  const physicalHeight = metrics.screenHeight * ratio;
  if (ratio <= 1.25 && (physicalWidth >= 3_840 || physicalHeight >= 2_160)) return 13;
  if (ratio <= 1.25 && (physicalWidth >= 2_560 || physicalHeight >= 1_440)) return 12;
  return clampPanelFontSize(fallback, fallback);
}

export function usePanelFontSize(storageKey: string, fallback = 11) {
  const [state, setState] = useState<PanelFontSizeState>(() => readPanelFontSizeState(storageKey, fallback));
  useEffect(() => {
    try {
      localStorage.setItem(storageKey, state.automatic ? "auto" : String(state.size));
    } catch {
      // Local storage can be disabled by a browser policy; the control still works for this session.
    }
  }, [state, storageKey]);
  useEffect(() => {
    if (!state.automatic) return;
    const update = () => setState((current) => current.automatic ? { ...current, size: automaticPanelFontSize(fallback) } : current);
    window.addEventListener("resize", update);
    window.visualViewport?.addEventListener("resize", update);
    return () => {
      window.removeEventListener("resize", update);
      window.visualViewport?.removeEventListener("resize", update);
    };
  }, [fallback, state.automatic]);
  const setManualSize = (value: number) => setState({ size: clampPanelFontSize(value, fallback), automatic: false });
  const useAutomaticSize = () => setState({ size: automaticPanelFontSize(fallback), automatic: true });
  return [state.size, setManualSize, useAutomaticSize, state.automatic] as const;
}

function readPanelFontSizeState(storageKey: string, fallback: number): PanelFontSizeState {
  try {
    const stored = localStorage.getItem(storageKey);
    if (stored !== null && stored !== "auto") return { size: clampPanelFontSize(Number(stored), fallback), automatic: false };
  } catch {
    // Fall through to the automatic display-aware default.
  }
  return { size: automaticPanelFontSize(fallback), automatic: true };
}

function currentDisplayMetrics(): DisplayMetrics {
  if (typeof window === "undefined") return { screenWidth: 0, screenHeight: 0, devicePixelRatio: 1 };
  return {
    screenWidth: window.screen?.width ?? 0,
    screenHeight: window.screen?.height ?? 0,
    devicePixelRatio: window.devicePixelRatio || 1,
  };
}
