import { lazy, Suspense } from "react";
import { LoaderCircle } from "lucide-react";
import { useAppStore } from "../../store/app-store";

const TextEditor = lazy(() =>
  import("./TextEditor").then((module) => ({ default: module.TextEditor })),
);

// Keep the document mounted independently of the active session and tool panel.
export function RemoteEditorHost() {
  const editor = useAppStore((state) => state.remoteEditor);
  const connectionName = useAppStore(
    (state) =>
      state.sessions.find((session) => session.id === editor?.sessionId)?.title,
  );
  if (!editor) return null;
  return (
    <Suspense
      fallback={
        <div className="connection-overlay">
          <LoaderCircle className="spin" />
          <span>加载远端编辑器…</span>
        </div>
      }
    >
      <TextEditor
        key={JSON.stringify([
          editor.sessionId,
          editor.connectionId,
          editor.path,
        ])}
        {...editor}
        connectionName={connectionName}
        onClose={() => useAppStore.getState().closeTextEditor()}
      />
    </Suspense>
  );
}
