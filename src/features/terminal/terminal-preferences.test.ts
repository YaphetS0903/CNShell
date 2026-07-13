import { describe, expect, it } from "vitest";
import { defaultSettings } from "../../types";
import { resolveTerminalPreferences, terminalFontFamilies, terminalThemes, withTerminalFontSize } from "./terminal-preferences";

describe("terminal preferences",()=>{
  it("uses a connection override without changing the global default",()=>{const override={...defaultSettings.terminal,fontSize:18,colorScheme:"solarizedDark" as const};const settings={...defaultSettings,terminalOverrides:{connection:override}};expect(resolveTerminalPreferences(settings,"connection")).toEqual(override);expect(resolveTerminalPreferences(settings,"other")).toEqual(defaultSettings.terminal);});
  it("maps every supported font and color scheme to xterm values",()=>{expect(terminalFontFamilies.system).toContain("SFMono");expect(terminalThemes.cnshell.background).toBe("#07101d");expect(terminalThemes.light.foreground).toBe("#1f2937");});
  it("zooms an override independently and clamps the supported range",()=>{const settings={...defaultSettings,terminalOverrides:{connection:{...defaultSettings.terminal,fontSize:18}}};expect(withTerminalFontSize(settings,"connection",30).terminalOverrides.connection.fontSize).toBe(24);expect(withTerminalFontSize(settings,"other",9).terminal.fontSize).toBe(10);});
});
