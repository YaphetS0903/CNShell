import { useSyncExternalStore } from "react";

const DARK_MODE_QUERY = "(prefers-color-scheme: dark)";

function subscribeToSystemTheme(onStoreChange: () => void) {
  if (typeof window === "undefined" || !window.matchMedia)
    return () => undefined;

  const media = window.matchMedia(DARK_MODE_QUERY);
  media.addEventListener("change", onStoreChange);
  return () => media.removeEventListener("change", onStoreChange);
}

function systemPrefersDark() {
  if (typeof window === "undefined" || !window.matchMedia) return true;
  return window.matchMedia(DARK_MODE_QUERY).matches;
}

export function useSystemPrefersDark() {
  return useSyncExternalStore(
    subscribeToSystemTheme,
    systemPrefersDark,
    () => true,
  );
}
