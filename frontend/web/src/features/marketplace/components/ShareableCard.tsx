// src/features/marketplace/components/ShareableCard.tsx
// 1200x630 OG composition — no app chrome. SSR/PNG generation is deferred
// to Phase 6 (A6); this renders the same composition in-app for preview.
import { GenArtPlaceholder } from "./GenArtPlaceholder";
import { AgentIcon } from "./AgentIcon";
import { VerifiedBadge } from "./VerifiedBadge";
import type { ShareableCardData } from "../data/types";

export function ShareableCard({ data }: { data: ShareableCardData }) {
  const pos = data.return30dPct >= 0;
  return (
    <div style={{ width: "1200px", height: "630px" }} className="flex bg-bg text-text overflow-hidden">
      <div className="relative w-[600px] h-full">
        <GenArtPlaceholder seed={data.genArtSeed} size={600} className="!rounded-none" />
        <div className="absolute bottom-4 left-4 px-2 py-1 rounded-sm bg-black/40 backdrop-blur text-[12px] font-mono">
          NFT · MANTLE
        </div>
      </div>
      <div className="w-[600px] h-full p-[38px_44px] flex flex-col justify-between">
        <div className="flex items-center gap-2">
          {data.verification === "verified" ? <VerifiedBadge /> : null}
        </div>
        <div>
          <h1 className="font-mono text-[44px] font-semibold leading-none">{data.id}</h1>
          <p className="mt-2 text-text-2 text-[15px]">by {data.creator.handle ?? data.creator.address} · {data.version}</p>
          {data.promise ? <p className="mt-3 text-[15px] leading-snug">{data.promise}</p> : null}
        </div>
        <div className="flex items-end justify-between border-t border-border pt-4">
          <div>
            <div className="text-text-3 text-[11px] uppercase tracking-wide">{data.return30dLabel ?? "30D"} RETURN</div>
            <div className={`font-mono text-[64px] font-semibold leading-none ${pos ? "text-gold" : "text-danger"}`}>
              {pos ? "+" : ""}{data.return30dPct}%
            </div>
          </div>
          <div className="text-right">
            <div className="text-text-3 text-[11px] uppercase tracking-wide">Run by</div>
            <div className="inline-flex items-center gap-1 text-[15px]">
              {data.buyers.humans} humans + <AgentIcon /> {data.buyers.agents} agents
            </div>
          </div>
        </div>
        <div className="flex items-center justify-between text-[13px]">
          <span>{data.priceUsdc} USDC · perpetual · ${data.paidToCreatorUsd} paid to creator</span>
          <span className="text-gold font-mono">{data.url}</span>
        </div>
      </div>
    </div>
  );
}
