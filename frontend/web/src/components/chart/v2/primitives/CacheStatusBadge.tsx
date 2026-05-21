type CacheState = "fresh" | "cached" | "stale";

type Props = {
  state: CacheState;
  fetchedAt?: number;
};

const STATE_CONFIG: Record<CacheState, { label: string; dotClass: string }> = {
  fresh: { label: "Fresh", dotClass: "bg-green-500" },
  cached: { label: "Cached", dotClass: "bg-amber-500" },
  stale: { label: "Stale", dotClass: "bg-red-500" },
};

export function CacheStatusBadge({ state, fetchedAt }: Props) {
  const config = STATE_CONFIG[state];
  const timeStr =
    fetchedAt != null
      ? new Date(fetchedAt).toLocaleTimeString()
      : null;

  return (
    <span
      className="inline-flex items-center gap-1.5 text-[11px] text-text-3"
      aria-label={`Cache: ${config.label}`}
    >
      <span
        className={`inline-block w-1.5 h-1.5 rounded-full shrink-0 ${config.dotClass}`}
        aria-hidden
      />
      <span className="text-text-2">{config.label}</span>
      {timeStr && (
        <span className="text-[10px] text-text-3">{timeStr}</span>
      )}
    </span>
  );
}
