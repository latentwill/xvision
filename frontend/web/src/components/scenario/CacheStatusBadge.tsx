import type { CacheStatus } from "@/api/types.gen/CacheStatus";

export function CacheStatusBadge({
  status,
  onFetch,
  fetchStatus,
  disabled = false,
}: {
  status: CacheStatus;
  onFetch?: () => void;
  fetchStatus?: string | null;
  disabled?: boolean;
}) {
  if (status.type === "FullyCached") {
    return (
      <span className="px-2 py-0.5 rounded text-[11px] bg-green-500/15 text-green-400 border border-green-500/30">
        cached: {status.bar_count} bars
      </span>
    );
  }
  if (status.type === "PartiallyCached") {
    return (
      <span className="inline-flex items-center gap-2 px-2 py-0.5 rounded text-[11px] bg-amber-500/15 text-amber-300 border border-amber-500/30">
        partial: {status.fetched_count}/{status.expected_count}
        <FetchButton
          onFetch={onFetch}
          disabled={disabled}
          fetchStatus={fetchStatus}
        />
      </span>
    );
  }
  // NotCached
  return (
    <span className="inline-flex items-center gap-2 px-2 py-0.5 rounded text-[11px] bg-amber-500/15 text-amber-300 border border-amber-500/30">
      not cached ({status.expected_count} bars on first run)
      <FetchButton
        onFetch={onFetch}
        disabled={disabled}
        fetchStatus={fetchStatus}
      />
    </span>
  );
}

function FetchButton({
  onFetch,
  disabled,
  fetchStatus,
}: {
  onFetch?: () => void;
  disabled: boolean;
  fetchStatus?: string | null;
}) {
  if (!onFetch) return fetchStatus ? <span>{fetchStatus}</span> : null;
  return (
    <button
      type="button"
      onClick={onFetch}
      disabled={disabled}
      className="underline hover:no-underline disabled:no-underline disabled:opacity-70"
    >
      {fetchStatus ?? "Fetch bars"}
    </button>
  );
}
