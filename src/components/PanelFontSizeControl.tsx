import { Minus, Plus, Type } from "lucide-react";
import { IconButton } from "./IconButton";
import { PANEL_FONT_SIZE_MAX, PANEL_FONT_SIZE_MIN } from "../lib/panel-font-size";

export function PanelFontSizeControl({ value, onChange, onAutomatic, automatic, label }: { value: number; onChange: (value: number) => void; onAutomatic: () => void; automatic: boolean; label: string }) {
  const adjust = (delta: number) => onChange(value + delta);
  return <div className="panel-font-size-control" role="group" aria-label={`${label}字号`} title={`${label}字号：${automatic ? "自动 " : ""}${value}px`}>
    <IconButton className="panel-font-size-button" icon={Type} label={`${automatic ? "已使用" : "恢复"}${label}自动字号`} active={automatic} onClick={onAutomatic} />
    <IconButton className="panel-font-size-button" icon={Minus} label={`减小${label}字号`} onClick={() => adjust(-1)} disabled={value <= PANEL_FONT_SIZE_MIN} />
    <output aria-live="polite" title={`${label}当前字号`}>{automatic ? "自" : ""}{value}px</output>
    <IconButton className="panel-font-size-button" icon={Plus} label={`增大${label}字号`} onClick={() => adjust(1)} disabled={value >= PANEL_FONT_SIZE_MAX} />
  </div>;
}
