// src/features/marketplace/routes/browse/LeaderboardRail.tsx
import { useQuery } from "@tanstack/react-query";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import type { Slice, SliceId } from "@/features/marketplace/data/types";

interface LeaderboardRailProps {
  activeSliceId: SliceId | undefined;
  onSliceClick: (id: SliceId) => void;
}

export function LeaderboardRail({ activeSliceId, onSliceClick }: LeaderboardRailProps) {
  const mp = useMarketplaceData();
  const { data: slices = [] } = useQuery<Slice[]>({
    queryKey: ["marketplace", "slices"],
    queryFn: () => mp.getSlices(),
  });

  return (
    <aside className="border-r border-border px-3.5 py-4 flex flex-col gap-3.5 overflow-hidden w-[232px] shrink-0">
      <div>
        <div className="flex items-center justify-between mb-2">
          <span className="font-mono text-[9.5px] tracking-[0.18em] uppercase text-text-3">
            LEADERBOARDS
          </span>
          <span className="font-mono text-[10px] text-text-4">shareable URLs</span>
        </div>
        <div className="flex flex-col">
          {slices.map((s) => {
            const isActive = s.id === activeSliceId;
            return (
              <div
                key={s.id}
                data-testid={`slice-${s.id}`}
                role="button"
                tabIndex={0}
                onClick={() => onSliceClick(s.id)}
                onKeyDown={(e) => e.key === "Enter" && onSliceClick(s.id)}
                className={[
                  "px-2.5 py-2 -mx-2 rounded cursor-pointer",
                  isActive
                    ? "bg-gold/10 border border-gold/30 text-gold"
                    : "bg-transparent border border-transparent text-text hover:bg-surface-elev",
                ].join(" ")}
              >
                <div className="flex items-center gap-2">
                  <span className={`text-[12.5px] font-${isActive ? "semibold" : "medium"}`}>
                    {s.label}
                  </span>
                  <span className="font-mono ml-auto text-[10px] text-text-3">
                    {s.count.toLocaleString()}
                  </span>
                </div>
                <div className="font-mono text-[9.5px] text-text-3 mt-0.5 tracking-[0.02em]">
                  {s.hint}
                </div>
              </div>
            );
          })}
        </div>
      </div>

      {/* Chain ops callout */}
      <div className="mt-auto px-3 py-2.5 border border-dashed border-border-strong rounded-[5px]">
        <div className="font-mono text-[9.5px] tracking-[0.18em] uppercase text-text-3 mb-1.5">
          CHAIN OPS
        </div>
        <div className="font-mono text-[10.5px] text-text-3 leading-[1.5]">
          Anchor · mint missing · attesters → in{" "}
          <span className="text-text-2">Settings → Chain ops</span>
        </div>
      </div>
    </aside>
  );
}
