import { useEffect } from "react";
import { X } from "lucide-react";
import { IconButton } from "./IconButton";

export function ErrorToast({ message, onClose, durationMs = 5_000 }: { message: string; onClose: () => void; durationMs?: number }) {
  useEffect(() => {
    const timer = window.setTimeout(onClose, durationMs);
    return () => window.clearTimeout(timer);
  }, [durationMs, message, onClose]);
  return <div className="toast error-toast" role="alert"><span>{message}</span><IconButton icon={X} label="关闭错误" onClick={onClose}/></div>;
}
