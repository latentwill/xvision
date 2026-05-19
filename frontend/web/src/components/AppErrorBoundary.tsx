import { Component, type ErrorInfo, type ReactNode } from "react";
import { attemptChunkReload, isChunkLoadError } from "@/lib/chunk-reload";
import { errorSummary, logError, logInfo } from "@/lib/logger";

interface AppErrorBoundaryProps {
  children: ReactNode;
}

interface AppErrorBoundaryState {
  error: unknown;
  /** Set when a reload was successfully triggered; we render the
   * "Updating..." placeholder while the browser navigates away. */
  reloading: boolean;
}

/**
 * Top-level error boundary that catches errors bubbling from the
 * `Suspense` children in `routes.tsx`. When the error matches the
 * chunk-load shape, the boundary asks `attemptChunkReload` to either
 * trigger a one-time `window.location.reload()` (showing the
 * "Updating..." placeholder while the new HTML loads) or — if a
 * reload was already attempted this session — fall through to the
 * existing error render with a manual-refresh hint.
 *
 * Non-chunk errors fall through unchanged: this boundary deliberately
 * does NOT swallow unrelated errors. See `feedback_alpha_root_cause` —
 * silencing real bugs is a regression we don't ship.
 */
export class AppErrorBoundary extends Component<
  AppErrorBoundaryProps,
  AppErrorBoundaryState
> {
  state: AppErrorBoundaryState = { error: null, reloading: false };

  static getDerivedStateFromError(error: unknown): Partial<AppErrorBoundaryState> {
    return { error };
  }

  componentDidCatch(error: unknown, info: ErrorInfo) {
    if (isChunkLoadError(error)) {
      const reloaded = attemptChunkReload(error);
      if (reloaded) {
        logInfo("app", "chunk_reload.triggered", {
          error: errorSummary(error),
        });
        this.setState({ reloading: true });
        return;
      }
      logError("app", "chunk_reload.exhausted", {
        error: errorSummary(error),
        component_stack: info.componentStack ?? null,
      });
      // Fall through with `reloading: false` — the render path will
      // show the manual-refresh hint.
      return;
    }

    // Non-chunk error: log and let the existing error render handle it.
    logError("app", "app_error_boundary.caught", {
      error: errorSummary(error),
      component_stack: info.componentStack ?? null,
    });
  }

  render() {
    const { error, reloading } = this.state;

    if (reloading) {
      return (
        <div
          className="flex min-h-[40vh] items-center justify-center px-4 py-6 text-[13px] text-text-3"
          role="status"
          aria-live="polite"
        >
          Updating to the latest version…
        </div>
      );
    }

    if (error) {
      const chunkError = isChunkLoadError(error);
      return (
        <div
          className="flex min-h-[40vh] flex-col items-start justify-center gap-2 px-4 py-6 text-[13px] text-text-2"
          role="alert"
        >
          <div className="font-medium text-text-1">
            {chunkError
              ? "Couldn’t load the latest app bundle."
              : "Something went wrong."}
          </div>
          <div className="text-text-3">
            {chunkError
              ? "Reload didn’t recover — please refresh the page manually or contact support."
              : "Please refresh the page. If the problem persists, contact support."}
          </div>
        </div>
      );
    }

    return this.props.children;
  }
}

export default AppErrorBoundary;
