/** Shared time-formatting helpers for the autooptimizer feature. */

/** Format an ISO timestamp as a relative past label ("just now", "3m ago", "2h ago", "1d ago"). */
export function formatRelativeTime(iso?: string): string {
  if (!iso) return "";
  try {
    const diffMs = Date.now() - new Date(iso).getTime();
    if (!Number.isFinite(diffMs)) return iso;
    const diffMin = Math.floor(diffMs / 60_000);
    if (diffMin < 1) return "just now";
    if (diffMin < 60) return `${diffMin}m ago`;
    const diffHr = Math.floor(diffMin / 60);
    if (diffHr < 24) return `${diffHr}h ago`;
    return `${Math.floor(diffHr / 24)}d ago`;
  } catch {
    return iso;
  }
}

/** Format a positive duration (ms) as a compact elapsed label for a live run:
 * "7s", "4m 03s", "1h 12m". Seconds are dropped past the hour mark. Returns
 * null for negative / non-finite input so callers can omit the segment. */
export function formatElapsed(ms: number): string | null {
  if (!Number.isFinite(ms) || ms < 0) return null;
  const totalSec = Math.floor(ms / 1000);
  const hrs = Math.floor(totalSec / 3600);
  const mins = Math.floor((totalSec % 3600) / 60);
  const secs = totalSec % 60;
  if (hrs > 0) return `${hrs}h ${String(mins).padStart(2, "0")}m`;
  if (mins > 0) return `${mins}m ${String(secs).padStart(2, "0")}s`;
  return `${secs}s`;
}

/** Format an ISO timestamp as a relative future label ("in 3m", "in 2h", "in 1d").
 * Returns null if the timestamp is in the past or unparseable. */
export function formatUntil(iso: string): string | null {
  try {
    const diffMs = new Date(iso).getTime() - Date.now();
    if (!Number.isFinite(diffMs) || diffMs <= 0) return null;
    const diffMin = Math.round(diffMs / 60_000);
    if (diffMin < 60) return `in ${Math.max(diffMin, 1)}m`;
    const diffHr = Math.round(diffMin / 60);
    if (diffHr < 24) return `in ${diffHr}h`;
    return `in ${Math.round(diffHr / 24)}d`;
  } catch {
    return null;
  }
}
