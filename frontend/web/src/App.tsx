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

export function App() {
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
