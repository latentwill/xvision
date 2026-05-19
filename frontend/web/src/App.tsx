import { useEffect, useState } from "react";
import { RouterProvider } from "react-router-dom";
import {
  MutationCache,
  QueryCache,
  QueryClient,
  QueryClientProvider,
} from "@tanstack/react-query";
import { router } from "./routes";
import { ThemeProvider } from "./theme/ThemeProvider";
import { errorSummary, logDebug, logError, logInfo } from "./lib/logger";
import { AppErrorBoundary } from "./components/AppErrorBoundary";
import { consumePostReloadNotice } from "./lib/chunk-reload";

const queryClient = new QueryClient({
  queryCache: new QueryCache({
    onError: (error, query) => {
      logError("query", "query.error", {
        query_key: query.queryKey,
        error: errorSummary(error),
      });
    },
    onSuccess: (_data, query) => {
      logDebug("query", "query.ok", {
        query_key: query.queryKey,
      });
    },
  }),
  mutationCache: new MutationCache({
    onError: (error, _variables, _context, mutation) => {
      logError("mutation", "mutation.error", {
        mutation_key: mutation.options.mutationKey,
        error: errorSummary(error),
      });
    },
  }),
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

logInfo("app", "app.query_client.created");

router.subscribe((state) => {
  logInfo("route", "route.navigate", {
    pathname: state.location.pathname,
    search: state.location.search,
    navigation_state: state.navigation.state,
  });
});

/**
 * Lightweight post-reload notification. The project has no toast
 * library and the contract bans adding one. This component is the
 * one allowed exception to the no-popup rule (toast = transient,
 * non-focus-stealing feedback). It surfaces only when the previous
 * page-lifecycle performed a chunk-reload, then auto-dismisses.
 */
function ChunkReloadToast() {
  const [visible, setVisible] = useState(() => consumePostReloadNotice());

  useEffect(() => {
    if (!visible) return;
    logInfo("app", "chunk_reload.toast_shown");
    const id = window.setTimeout(() => setVisible(false), 6000);
    return () => window.clearTimeout(id);
  }, [visible]);

  if (!visible) return null;
  return (
    <div
      role="status"
      aria-live="polite"
      data-testid="chunk-reload-toast"
      className="fixed bottom-4 right-4 z-50 max-w-sm rounded-md border border-border bg-bg px-3 py-2 text-[13px] text-text-1 shadow-lg"
    >
      App was updated — reloaded to the latest version.
      <button
        type="button"
        onClick={() => setVisible(false)}
        className="ml-3 text-text-3 hover:text-text-1"
        aria-label="Dismiss notification"
      >
        ×
      </button>
    </div>
  );
}

export function App() {
  // The reload-attempted flag is cleared by `RouteLoaded` inside each
  // route's Suspense (see `routes.tsx`) once the lazy chunk has
  // actually resolved — not on App's first commit, which can happen
  // while Suspense is still showing the fallback. Clearing too early
  // would let a second still-failing chunk-load trigger a fresh reload
  // and loop. See PR #317 review (P1) for the failure mode.
  //
  // Outer `AppErrorBoundary` catches chunk failures from layout-level
  // lazy chunks (`LazyChatRail`, `StripDockSlot`) that render as
  // siblings/ancestors of the per-route boundary in `routes.tsx`. The
  // inner per-route boundary still runs first for route page errors;
  // this one is the safety net for everything above the page Outlet.
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <AppErrorBoundary>
          <RouterProvider router={router} />
        </AppErrorBoundary>
        <ChunkReloadToast />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
