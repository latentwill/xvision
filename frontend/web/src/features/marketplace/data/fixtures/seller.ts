// src/features/marketplace/data/fixtures/seller.ts
import type { ListableStrategy, PublishDraft } from "../types";

export const LISTABLE_STRATEGIES: ListableStrategy[] = [
  { id: "local-btc-momentum", name: "btc-momentum", version: "v3.0", assets: ["BTC"] },
  { id: "local-eth-mr", name: "eth-mr", version: "v2.0", assets: ["ETH"] },
  { id: "local-wip-draft", name: "wip-draft", version: "v0.1", assets: [] },
];

export function buildPublishDraft(strategyId: string): PublishDraft {
  const s = LISTABLE_STRATEGIES.find((x) => x.id === strategyId);
  const hasAssets = !!s && s.assets.length > 0;
  return {
    strategyId,
    name: s?.name ?? strategyId,
    listable: [
      { ok: !!s, label: "Strategy exists in your XVN", reason: s ? undefined : "Strategy not found" },
      { ok: hasAssets, label: "Declares an asset universe", reason: hasAssets ? undefined : "No assets configured" },
      { ok: true, label: "Has a backtest on record" },
    ],
    tier: "sealed",
    priceUsdc: 49,
    acceptedPayers: { humans: true, agents: true },
    ingredients: [],
    preview: {
      id: s?.name ?? strategyId, lineageId: s?.name ?? strategyId, version: s?.version ?? "v0.1",
      creator: { address: "0xa83e", handle: "@ed" }, model: "Claude · Haiku 4.5", style: "Day",
      assets: s?.assets ?? [], return30dPct: 0, sharpe: 0, buyers: { humans: 0, agents: 0 },
      priceUsdc: 49, tier: "sealed", verification: "unverified", acceptsX402: true, clones: 0,
      transferableLicense: false, genArtSeed: strategyId,
    },
  };
}
