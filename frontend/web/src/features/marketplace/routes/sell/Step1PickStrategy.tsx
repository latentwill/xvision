// src/features/marketplace/routes/sell/Step1PickStrategy.tsx
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { AssetPill } from "@/features/marketplace/components/AssetPill";
import type { ListableStrategy } from "@/features/marketplace/data/types";

export function Step1PickStrategy({
  onSelect,
}: {
  onSelect: (strategy: ListableStrategy) => void;
}) {
  const mp = useMarketplaceData();
  const { data: strategies, isLoading, isError, refetch } = useQuery({
    queryKey: ["marketplace", "listable"],
    queryFn: () => mp.listListableStrategies(),
  });

  if (isError) {
    return (
      <div className="text-[13px] text-text-2">
        Couldn&apos;t load your strategies.{" "}
        <button
          type="button"
          onClick={() => refetch()}
          className="text-gold hover:underline underline-offset-2"
        >
          Retry
        </button>
      </div>
    );
  }

  if (isLoading || !strategies) {
    return <p className="text-[13px] text-text-3">Loading strategies…</p>;
  }

  if (strategies.length === 0) {
    return (
      <p className="text-[13px] text-text-2">
        No listable strategies found. Run at least one backtest first.
      </p>
    );
  }

  return (
    <ul data-testid="sell-step-1-body" className="flex flex-col gap-2">
      {strategies.map((s) => (
        <li key={s.id}>
          <button
            onClick={() => onSelect(s)}
            aria-label={s.name}
            className="w-full flex items-center gap-3 px-4 py-3 rounded-md bg-surface-elev border border-border hover:border-gold/40 text-left transition-colors"
          >
            <div className="flex-1 min-w-0">
              <p className="text-[13px] font-medium truncate">{s.name}</p>
              <p className="text-[11px] text-text-3 font-mono">{s.version}</p>
            </div>
            <div className="flex gap-1 flex-wrap justify-end">
              {s.assets.length > 0 ? (
                s.assets.map((a) => <AssetPill key={a} asset={a} />)
              ) : (
                <span className="text-[11px] text-text-3 italic">no assets</span>
              )}
            </div>
            <svg
              width="14"
              height="14"
              viewBox="0 0 14 14"
              fill="none"
              stroke="currentColor"
              strokeWidth="1.5"
              className="text-text-3 shrink-0"
              aria-hidden="true"
            >
              <path d="M5 3l4 4-4 4" strokeLinecap="round" strokeLinejoin="round" />
            </svg>
          </button>
        </li>
      ))}
    </ul>
  );
}
