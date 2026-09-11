import { createContext, useContext, useEffect } from "react";

export const SettingsModuleDraftContext = createContext<
  ((dirty: boolean) => void) | null
>(null);

export function useSettingsModuleDraftState(dirty: boolean) {
  const reportDirty = useContext(SettingsModuleDraftContext);
  useEffect(() => {
    reportDirty?.(dirty);
    return () => reportDirty?.(false);
  }, [dirty, reportDirty]);
}
