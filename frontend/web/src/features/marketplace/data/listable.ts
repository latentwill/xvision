// listable.ts — operator-local "what can I list?" data for the sell flow.
//
// Listing your OWN strategies (the Step-1 picker + Step-2 draft) is an
// operator-local concern served by `/api/strategies` — it does NOT depend on
// the on-chain marketplace indexer being live. Both the indexer-backed
// `ApiMarketplaceData` and the offline `FixtureMarketplaceData` route the sell
// flow through these helpers so the picker always shows the operator's REAL
// strategy library, never placeholder rows, regardless of indexer status.
import {
  getStrategy,
  listStrategiesPaged,
  type Strategy,
  type StrategyListItem,
} from "@/api/strategies";
import type { ListableStrategy, PublishDraft } from "./types";

function assetSymbols(assetUniverse: string[] | undefined): string[] {
  const symbols = new Set<string>();
  for (const raw of assetUniverse ?? []) {
    const symbol = raw.split("/")[0]?.trim().toUpperCase();
    if (symbol) symbols.add(symbol);
  }
  return [...symbols];
}

function listableStrategyVersion(strategy: StrategyListItem): string {
  if (!strategy.last_eval_completed_at) return "not evaluated";
  const date = strategy.last_eval_completed_at.slice(0, 10);
  return date ? `evaluated ${date}` : "evaluated";
}

export function toListableStrategy(strategy: StrategyListItem): ListableStrategy {
  return {
    id: strategy.agent_id,
    name: strategy.display_name?.trim() || strategy.agent_id,
    version: listableStrategyVersion(strategy),
    assets: assetSymbols(strategy.asset_universe),
  };
}

function ingredientsForStrategy(strategy: Strategy) {
  return [
    ...strategy.manifest.attested_with.map((name) => ({
      name,
      kind: "model" as const,
      installed: true,
    })),
    ...strategy.manifest.required_tools.map((name) => ({
      name,
      kind: name.toLowerCase().includes("mcp") ? ("mcp" as const) : ("skill" as const),
      installed: true,
    })),
  ];
}

export function buildApiPublishDraft(
  strategy: Strategy,
  row: StrategyListItem | undefined,
): PublishDraft {
  const assets = assetSymbols(strategy.manifest.asset_universe);
  const strategyId = strategy.manifest.id;
  const name = strategy.manifest.display_name?.trim() || row?.display_name?.trim() || strategyId;
  const hasBacktest = !!row?.last_eval_completed_at;
  return {
    strategyId,
    // The listing name defaults to the strategy's own display name; the seller
    // can override it in Step 2 before minting (QA: listings must inherit a
    // real name, not render "Strategy #N").
    name,
    listable: [
      { ok: true, label: "Strategy exists in your XVN" },
      {
        ok: assets.length > 0,
        label: "Declares an asset universe",
        reason: assets.length > 0 ? undefined : "No assets configured",
      },
      {
        ok: hasBacktest,
        label: "Has a backtest on record",
        reason: hasBacktest ? undefined : "No completed eval on record",
      },
    ],
    tier: "sealed",
    priceUsdc: 49,
    acceptedPayers: { humans: true, agents: true },
    ingredients: ingredientsForStrategy(strategy),
    preview: {
      id: name,
      lineageId: strategyId,
      version: row ? listableStrategyVersion(row) : "not evaluated",
      creator: { address: strategy.manifest.creator || "" },
      model: strategy.manifest.attested_with[0] ?? "Unknown model",
      style: "Strategy",
      assets,
      return30dPct: 0,
      sharpe: 0,
      buyers: { humans: 0, agents: 0 },
      priceUsdc: 49,
      tier: "sealed",
      transferableLicense: false,
      verification: "unverified",
      acceptsX402: true,
      genArtSeed: strategyId,
    },
  };
}

/** Fetch the operator's real strategy library, mapped to picker rows. */
export async function fetchListableStrategies(): Promise<ListableStrategy[]> {
  const page = await listStrategiesPaged({ limit: 200 });
  return page.items.map(toListableStrategy);
}

/** Build a publish draft from the operator's real stored strategy. */
export async function fetchPublishDraft(strategyId: string): Promise<PublishDraft> {
  const [strategy, page] = await Promise.all([
    getStrategy(strategyId),
    listStrategiesPaged({ limit: 200 }),
  ]);
  const row = page.items.find((item) => item.agent_id === strategyId);
  return buildApiPublishDraft(strategy, row);
}
