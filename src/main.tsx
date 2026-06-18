import React from "react";
import ReactDOM from "react-dom/client";
import "@xterm/xterm/css/xterm.css";
import "./styles.css";
import { App } from "./ui/App";

class RootErrorBoundary extends React.Component<{ children: React.ReactNode }, { error?: Error }> {
  state: { error?: Error } = {};

  static getDerivedStateFromError(error: Error) {
    return { error };
  }

  componentDidCatch(error: Error, info: React.ErrorInfo) {
    void window.cnshell?.logs.reportRendererError({
      message: error.message,
      stack: error.stack,
      componentStack: info.componentStack ?? undefined
    });
  }

  render() {
    if (this.state.error) {
      return (
        <main className="boot-error-shell" role="alert">
          <section>
            <div className="brand-mark" aria-hidden="true">
              CN
            </div>
            <strong>CNshell 界面已恢复到安全模式</strong>
            <span>{this.state.error.message}</span>
            <button type="button" onClick={() => window.location.reload()}>
              重新加载工作台
            </button>
          </section>
        </main>
      );
    }

    return this.props.children;
  }
}

ReactDOM.createRoot(document.getElementById("root")!).render(
  <React.StrictMode>
    <RootErrorBoundary>
      <App />
    </RootErrorBoundary>
  </React.StrictMode>
);
