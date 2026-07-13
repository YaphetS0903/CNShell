import type { TerminalPreferences } from "../../types";

const schemes: { value: TerminalPreferences["colorScheme"]; label: string; colors: [string, string, string] }[] = [
  { value: "cnshell", label: "CNshell", colors: ["#07101d", "#60a5fa", "#4ade80"] },
  { value: "classic", label: "经典", colors: ["#000000", "#e5e5e5", "#cd3131"] },
  { value: "solarizedDark", label: "Solarized", colors: ["#002b36", "#268bd2", "#b58900"] },
  { value: "light", label: "浅色", colors: ["#ffffff", "#1d4ed8", "#047857"] },
];

export function TerminalPreferencesFields({ value, onChange, idPrefix }: { value: TerminalPreferences; onChange: (value: TerminalPreferences) => void; idPrefix: string }) {
  const change = <K extends keyof TerminalPreferences>(key: K, next: TerminalPreferences[K]) => onChange({ ...value, [key]: next });
  return <div className="terminal-preferences-grid">
    <label><span>字体</span><select value={value.fontFamily} onChange={(event)=>change("fontFamily",event.target.value as TerminalPreferences["fontFamily"])}><option value="system">系统等宽字体</option><option value="menlo">Menlo</option><option value="monaco">Monaco</option><option value="courier">Courier New</option></select></label>
    <label><span>回滚行数</span><select value={value.scrollback} onChange={(event)=>change("scrollback",Number(event.target.value))}><option value={1000}>1,000 行</option><option value={10000}>10,000 行（推荐）</option><option value={50000}>50,000 行</option><option value={100000}>100,000 行</option></select></label>
    <label className="range-setting" htmlFor={`${idPrefix}-font-size`}><span>字号</span><div><input id={`${idPrefix}-font-size`} type="range" min={10} max={24} step={1} value={value.fontSize} onChange={(event)=>change("fontSize",Number(event.target.value))}/><output htmlFor={`${idPrefix}-font-size`}>{value.fontSize} px</output></div></label>
    <label className="range-setting" htmlFor={`${idPrefix}-line-height`}><span>行高</span><div><input id={`${idPrefix}-line-height`} type="range" min={1} max={2} step={0.05} value={value.lineHeight} onChange={(event)=>change("lineHeight",Number(Number(event.target.value).toFixed(2)))}/><output htmlFor={`${idPrefix}-line-height`}>{value.lineHeight.toFixed(2)}</output></div></label>
    <fieldset className="preference-segment"><legend>光标形状</legend><div>{([['bar','竖线'],['block','方块'],['underline','下划线']] as const).map(([cursor,label])=><button key={cursor} type="button" role="radio" aria-checked={value.cursorStyle===cursor} className={value.cursorStyle===cursor?"active":""} onClick={()=>change("cursorStyle",cursor)}>{label}</button>)}</div></fieldset>
    <label className="check-row terminal-cursor-blink"><input type="checkbox" checked={value.cursorBlink} onChange={(event)=>change("cursorBlink",event.target.checked)}/><span>光标闪烁</span></label>
    <fieldset className="terminal-scheme-picker"><legend>终端配色</legend><div>{schemes.map((scheme)=><button key={scheme.value} type="button" role="radio" aria-checked={value.colorScheme===scheme.value} className={value.colorScheme===scheme.value?"active":""} onClick={()=>change("colorScheme",scheme.value)}><span className="scheme-swatches" aria-hidden="true">{scheme.colors.map((color)=><i key={color} style={{background:color}}/>)}</span><span>{scheme.label}</span></button>)}</div></fieldset>
  </div>;
}
