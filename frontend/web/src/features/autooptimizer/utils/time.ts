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
