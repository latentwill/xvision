// Full-width safety-pause banner rendered at the top of HomeRoute.
//
// Polls /api/safety/state every 30s. When paused, renders a red danger strip
// with the reason (if any) and a link to the /safety route. When running,
// loading, or error: renders nothing.
//
// The Topbar's compact SafetyPauseBadge covers the nav chrome; this banner
// covers the page-level home content area — they serve different contexts.

import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { getSafetyState, safetyKeys } from "@/api/safety";

export function SafetyPauseBanner() {
  const q = useQuery({
    queryKey: safetyKeys.state(),
    queryFn: getSafetyState,
    refetchInterval: 30_000,
    refetchOnWindowFocus: true,
  });

  // Hide while loading, on error, or when safety is not paused.
  if (q.isPending || q.isError || !q.data || !q.data.paused) return null;

  return (
    <div
      role="alert"
      className="w-full flex items-center justify-between gap-4 rounded-md px-4 py-3 bg-red-600/10 border border-red-600/30 text-red-700 dark:text-red-400"
    >
      <div className="flex items-center gap-3 min-w-0">
        <span className="shrink-0 w-2 h-2 rounded-full bg-red-600 dark:bg-red-400" />
        <span className="text-[13px] font-medium">Safety paused</span>
        {q.data.reason ? (
          <span className="text-[13px] text-red-700/80 dark:text-red-400/80 truncate">
            — {q.data.reason}
          </span>
        ) : null}
      </div>
      <Link
        to="/safety"
        className="shrink-0 text-[12px] font-medium text-red-700 dark:text-red-400 hover:underline whitespace-nowrap"
      >
        Go to Safety →
      </Link>
    </div>
  );
}
