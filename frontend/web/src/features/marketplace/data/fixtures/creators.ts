// src/features/marketplace/data/fixtures/creators.ts
import type { CreatorProfile } from "../types";
import { NAMED_LISTINGS } from "./listings";

export const ED_CREATOR = { address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4", handle: "@ed", ens: "ed.xvn" };
const ed = ED_CREATOR;

export const CREATORS: Record<string, CreatorProfile> = {
  "@ed": {
    creator: ed, joinedAt: "2025-08-12T00:00:00Z", reputation: 4.8, notableTag: "agent #0 contributor",
    counters: { strategies: 3, lifetimeEarnedUsd: 4820, totalBuyers: { humans: 469, agents: 27 }, clonesSpawned: 11, clonesUpstreamUsd: 2100, attestationsIssued: 14 },
    strategies: [
      { ...NAMED_LISTINGS[3], status: "live" },
      { ...NAMED_LISTINGS[4], status: "live" },
      { ...NAMED_LISTINGS[5], status: "live" },
    ],
    earningsWeekly: Array.from({ length: 32 }, (_, i) => 40 + i * i * 4),
    earningsSummary: { last7dUsd: 420, last30dUsd: 1180 },
    forest: {
      nodes: [
        { id: "bm-v1", x: 60, y: 50, label: "v1.0", strategy: "btc-momentum", genArtSeed: "btc-momentum-7a91-v1" },
        { id: "bm-v2", x: 160, y: 50, label: "v2.1", strategy: "btc-momentum", genArtSeed: "btc-momentum-7a91-v2" },
        { id: "bm-v3", x: 260, y: 50, label: "v3.0", strategy: "btc-momentum", current: true, genArtSeed: "btc-momentum-7a91-v3" },
        { id: "cb-1", x: 380, y: 30, label: "@solyana", strategy: "clone-by", external: true },
        { id: "cb-more", x: 380, y: 90, label: "+6 more", strategy: "clone-by", external: true, more: true },
      ],
      edges: [
        { from: "bm-v1", to: "bm-v2" },
        { from: "bm-v2", to: "bm-v3" },
        { from: "bm-v3", to: "cb-1", kind: "clone" },
      ],
    },
    reputationFeed: [
      { direction: "received", verdict: "endorse", attester: "regime-verifier", on: "btc-momentum-v3", at: "2026-05-26T13:30:00Z" },
      { direction: "issued", verdict: "endorse", attester: "@ed", on: "sol-strategist-pro", at: "2026-05-26T06:00:00Z" },
      { direction: "received", verdict: "question", attester: "diversity-check", on: "btc-momentum-v3.1", at: "2026-05-26T10:30:00Z" },
    ],
    clonedBy: [
      { handle: "@solyana", from: "btc-momentum-v3", made: "sol-momentum-v1", earnedUsd: 680, at: "2026-05-24T00:00:00Z" },
      { handle: "@quantnext", from: "btc-momentum-v3", made: "multi-asset-rotation", earnedUsd: 420, at: "2026-05-21T00:00:00Z" },
      { handle: "+7 more", from: "…", made: "—", earnedUsd: 310, at: "2026-04-26T00:00:00Z", more: true },
    ],
  },
};

// Alias lookups by address and ENS so getCreator(<address>) and getCreator(<ens>) resolve correctly.
CREATORS[ed.address] = CREATORS["@ed"];
CREATORS[ed.ens] = CREATORS["@ed"];
