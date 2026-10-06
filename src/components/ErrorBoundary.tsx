import { Component, type ErrorInfo, type ReactNode } from "react";

interface State {
  error: Error | null;
}

/**
 * Last line of defense: a rendering bug shows a short message with a reload
 * button instead of a blank window. Only the error's type and message are
 * shown, never component props, so no assignment data ends up on screen.
 */
export class ErrorBoundary extends Component<{ children: ReactNode }, State> {
  override state: State = { error: null };

  static getDerivedStateFromError(error: Error): State {
    return { error };
  }

  override componentDidCatch(error: Error, info: ErrorInfo) {
    console.error(error, info.componentStack);
  }

  override render() {
    const { error } = this.state;
    if (!error) return this.props.children;
    // Bilingual because the translation context may be what failed.
    return (
      <div className="fatal" role="alert">
        <p>Something went wrong / 出现了错误</p>
        <p className="fatal__detail">
          {error.name}: {error.message}
        </p>
        <button type="button" className="primary-button" onClick={() => window.location.reload()}>
          Reload / 重新加载
        </button>
      </div>
    );
  }
}
