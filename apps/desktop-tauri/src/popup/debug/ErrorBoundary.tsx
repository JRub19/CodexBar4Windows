// Catches render-time errors in the popup tree. Without this, a
// throw inside any descendant unmounts the whole React subtree and
// leaves an empty `<div id="root">` — visually identical to the
// blank-popup symptom.
//
// When an error fires we render a high-contrast fallback panel with
// the message + stack so the user can screenshot it directly.

import { Component, type ErrorInfo, type ReactNode } from "react";
import { debugLog } from "./logger";

interface Props {
  children: ReactNode;
}

interface State {
  error: Error | null;
  componentStack: string | null;
}

export class ErrorBoundary extends Component<Props, State> {
  state: State = { error: null, componentStack: null };

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { error };
  }

  componentDidCatch(error: Error, info: ErrorInfo): void {
    this.setState({ componentStack: info.componentStack ?? null });
    debugLog.error(
      "ErrorBoundary",
      `${error.message}\n${error.stack ?? ""}\n${info.componentStack ?? ""}`,
    );
  }

  render() {
    if (this.state.error) {
      return (
        <div
          style={{
            position: "fixed",
            inset: 0,
            background: "#220000",
            color: "#fff",
            fontFamily: "Consolas, monospace",
            fontSize: 11,
            padding: 12,
            overflowY: "auto",
            zIndex: 2147483646,
          }}
        >
          <div
            style={{
              color: "#f88",
              fontWeight: 700,
              fontSize: 13,
              marginBottom: 6,
            }}
          >
            React render error
          </div>
          <pre style={{ whiteSpace: "pre-wrap", wordBreak: "break-all" }}>
            {this.state.error.message}
            {"\n\n"}
            {this.state.error.stack ?? ""}
            {this.state.componentStack
              ? `\n\nComponent stack:${this.state.componentStack}`
              : ""}
          </pre>
        </div>
      );
    }
    return this.props.children;
  }
}
