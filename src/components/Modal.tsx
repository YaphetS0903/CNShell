import { X } from "lucide-react";
import { useEffect, useRef, type ReactNode } from "react";
import { IconButton } from "./IconButton";

export function Modal({ title, children, onClose, wide = false }: { title: string; children: ReactNode; onClose: () => void; wide?: boolean }) {
  const dialogRef=useRef<HTMLElement>(null);
  useEffect(()=>{const previous=document.activeElement as HTMLElement|null;const dialog=dialogRef.current;const focusable=()=>Array.from(dialog?.querySelectorAll<HTMLElement>('button:not([disabled]),input:not([disabled]),select:not([disabled]),textarea:not([disabled]),[tabindex]:not([tabindex="-1"])')??[]);focusable()[0]?.focus();const keydown=(event:KeyboardEvent)=>{if(event.key==="Escape"){event.preventDefault();onClose();return;}if(event.key!=="Tab")return;const items=focusable();if(!items.length){event.preventDefault();return;}const first=items[0],last=items[items.length-1];if(event.shiftKey&&document.activeElement===first){event.preventDefault();last.focus();}else if(!event.shiftKey&&document.activeElement===last){event.preventDefault();first.focus();}};document.addEventListener("keydown",keydown);return()=>{document.removeEventListener("keydown",keydown);previous?.focus();};},[onClose]);
  return <div className="modal-backdrop" role="presentation" onMouseDown={(event) => event.target === event.currentTarget && onClose()}>
    <section ref={dialogRef} className={`modal ${wide ? "modal-wide" : ""}`} role="dialog" aria-modal="true" aria-label={title}>
      <header className="modal-header"><h2>{title}</h2><IconButton icon={X} label="关闭" onClick={onClose} /></header>
      <div className="modal-body">{children}</div>
    </section>
  </div>;
}
