import type { LucideIcon } from "lucide-react";
import type { ButtonHTMLAttributes } from "react";

interface Props extends ButtonHTMLAttributes<HTMLButtonElement> { icon: LucideIcon; label: string; active?: boolean }

export function IconButton({ icon: Icon, label, active, className = "", ...props }: Props) {
  return <button type="button" className={`icon-button ${active ? "is-active" : ""} ${className}`} aria-label={label} title={label} {...props}><Icon size={17} strokeWidth={1.8} /></button>;
}
