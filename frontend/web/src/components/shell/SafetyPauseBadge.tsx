// Inline pause-state indicator displayed in the Topbar.
//
// Polls /api/safety/state every 15s. When paused, renders a red pill with
// the text "paused" — linking to /safety for details. When running, renders
// nothing. No popups: the pill itself IS the indicator; detail lives on the
// /safety route.

import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Pill } from "@/components/primitives/Pill";
import { getSafetyState, safetyKeys } from "@/api/safety";

export function SafetyPauseBadge() {
  const q = useQuery({
    queryKey: safetyKeys.state(),
    queryFn: getSafetyState,
    refetchInterval: 15_000,
    refetchOnWindowFocus: true,
  });

  // Hide while loading or on error — don't distract with a badge flicker.
  if (q.isPending || q.isError || !q.data) return null;

  // Running state — render nothing. No clutter when all is well.
  if (!q.data.paused) return null;

  return (
    <Link
      to="/safety"
      data-testid="safety-pause-badge"
      aria-label="Safety paused — view audit log"
      className="no-underline"
    >
      <Pill tone="danger" title={q.data.reason ?? "Safety paused"}>
        <span className="w-1.5 h-1.5 rounded-full bg-danger" />
        paused
      </Pill>
    </Link>
  );
}
