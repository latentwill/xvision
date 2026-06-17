// src/features/marketplace/data/types.ts
// The MarketplaceData seam types. Typed/numeric (not display strings);
// components format. This is the draft data contract Phase 1 formalizes.

export type Id = string;            // listing slug, e.g. "btc-momentum-v3"
export type LineageId = string;     // "btc-momentum"
export type GenArtSeed = string;    // placeholder seed until Phase 4
export type IsoDateTime = string;
export type PayerKind = "human" | "agent";
export type Tier = "open" | "sealed";          // Tier A | Tier B
export type Verification = "verified" | "unverified";
export type IngredientKind = "model" | "mcp" | "skill";
export type Verdict = "endorse" | "question" | "reject";

export interface Creator {
  address: string;
  handle?: string;
  ens?: string;
}
export interface BuyerCounts {
  humans: number;
  agents: number;
}
export interface Ingredient {
  name: string;
  kind: IngredientKind;
  installed: boolean;
}
export interface TxRef {
  txHash: string;
  network: string; // drives the [Testnet] label on tx chips + chain-bound CTAs
}

export interface ListingRow {
  id: Id;
  lineageId: LineageId;
  version: string;
  name?: string;  // human-readable display name (from manifest meta / IndexedListing.name)
  creator: Creator;
  model: string;
  style: string;
  assets: string[];
  return30dPct: number;
  sharpe: number;
  buyers: BuyerCounts;
  priceUsdc: number | null; // null => Tier A open/free
  tier: Tier;
  transferableLicense: boolean; // default false (direction); opt-in per listing
  verification: Verification;
  acceptsX402: boolean;
  genArtSeed: GenArtSeed;
}

export interface MetricSet {
  return30dPct: number;
  sharpe: number;
  winRatePct: number;
  maxDrawdownPct: number;
  avgDurationDays: number;
}
export interface Variant {
  version: string;
  parent: string | null;
  genArtSeed: GenArtSeed;
  sharpe: number;
  current: boolean;
}
export interface RecentBuyer {
  label: string;
  payerKind: PayerKind;
  outcome: string;
  at: IsoDateTime;
}
export interface EquityCurve {
  base: number;
  points: { value: number; phase: "backtest" | "live" }[];
}
export interface TradeRecord {
  at: IsoDateTime;
  action: "buy" | "sell" | "close";
  symbol: string;
  qty: string;
  entry: number | null;
  exit: number | null;
  pnlUsd: number | null;
  pnlPct: number | null;
  runner: string;
  runnerKind: PayerKind;
  tx: string;
  anchorTx: string;
}
export interface OnChainReceipts {
  nft: {
    tokenId: string;
    lineageId: LineageId;
    agentURI: string;
    manifestHash: string;
    parentLineage: string | null;
    bornAt: IsoDateTime;
    operatorSig: string;
    contract: string;
    network: string;
  };
  attestations: { attester: string; verdict: Verdict; targetVersion: string; at: IsoDateTime }[];
  anchors: { kind: "merkle" | "mint" | "commit"; label: string; tx: string; at: IsoDateTime; gasEth: string }[];
  trades: TradeRecord[];
  tradesMeta: {
    totalOnChain: number;
    lastAnchorAt: IsoDateTime;
    receiptKind: string;
    netPnlUsd: number;
    window: string;
    anchorTx: string;
  };
}
export interface ListingDetail extends ListingRow {
  promise: string;
  metrics: MetricSet;
  paidToCreatorUsd: number;
  platformFeeBps: number;
  ingredients: Ingredient[];
  variants: Variant[];
  clonesOfYours?: { count: number; upstreamEarningsUsd: number };
  recentBuyers: RecentBuyer[];
  creatorOther: ListingRow[];
  equityCurve: EquityCurve;
  whatYouGet: string[];
  whatYouDont: string[];
  onChain: OnChainReceipts;
}

export interface ForestNode {
  id: string;
  x: number;
  y: number;
  label: string;
  strategy: string;
  current?: boolean;
  genArtSeed?: string;
  external?: boolean;
  more?: boolean;
}
export interface ForestEdge {
  from: string;
  to: string;
  kind?: "clone";
}
export interface AttestationActivity {
  direction: "received" | "issued";
  verdict: Verdict;
  attester: string;
  on: string;
  at: IsoDateTime;
}
export interface CloneByEntry {
  handle: string;
  from: string;
  made: string;
  earnedUsd: number;
  at: IsoDateTime;
  more?: boolean;
}
export interface CreatorProfile {
  creator: Creator;
  joinedAt: IsoDateTime;
  reputation: number;
  notableTag?: string;
  counters: {
    strategies: number;
    lifetimeEarnedUsd: number;
    totalBuyers: BuyerCounts;
    clonesSpawned: number;
    clonesUpstreamUsd: number;
    attestationsIssued: number;
  };
  strategies: (ListingRow & { status: "live" | "archived" })[];
  earningsWeekly: number[];
  earningsSummary: { last7dUsd: number; last30dUsd: number };
  forest: { nodes: ForestNode[]; edges: ForestEdge[] };
  reputationFeed: AttestationActivity[];
  clonedBy: CloneByEntry[];
}

export interface ShareableCardData {
  id: Id;
  version: string;
  creator: Creator;
  genArtSeed: GenArtSeed;
  return30dPct: number;
  return30dLabel?: string;
  buyers: BuyerCounts;
  paidToCreatorUsd: number;
  priceUsdc: number;
  verification: Verification;
  acceptsX402: boolean;
  promise?: string;
  url: string;
}
export interface ShareComposerData {
  ogCard: ShareableCardData;
  buyerStamp: string;
  caption: string;
  variants: string[];
  notificationHint: string;
}
export interface Receipt {
  txHash: string;
  network: string; // drives [Testnet] label on the receipt
  at: IsoDateTime;
  buyer: string;
  listing: { id: Id; version: string; creator: Creator; genArtSeed: GenArtSeed; return30dPct: number; buyers: BuyerCounts };
  license: {
    tokenId: string; // ERC-1155 (H6); prototype "ERC-721" label is the documented slip
    contract: string;
    manifestHash: string;
    bundleCid: string;
    pricePaidUsdc: number;
    feeUsdc: number;
    netToCreatorUsdc: number;
    mintedAt: IsoDateTime;
  };
  install: { xvnDetected: boolean; xvnEndpoint: string; ingredients: Ingredient[] };
  share: ShareComposerData;
}

export type SortKey = "return30d" | "sharpe" | "buyers" | "newest";
export interface FilterState {
  segment: "trending" | "new" | "mine";
  search: string;
  sort: SortKey;
  assets: string[];
  models: string[];
  styles: string[];
  tier: Tier[];
  trust: { verifiedOnly: boolean; acceptsAgents: boolean; auditedOnly: boolean };
  priceUsdc: { from: number; to: number };
  minBuyers: number;
  slice?: SliceId;
}
export type SliceId = string;
export interface Slice {
  id: SliceId;
  label: string;
  hint: string;
  count: number;
  filter: Partial<FilterState>;
}

export interface ListableStrategy {
  id: string;
  name: string;
  version: string;
  assets: string[];
}
export interface ListabilityCheck {
  ok: boolean;
  label: string;
  reason?: string;
}
export interface PublishDraft {
  strategyId: string;
  /**
   * Listing name shown on the marketplace. Defaults to the underlying xvn
   * strategy's display name; the seller may override it in the configure step
   * before minting. Sent to the publish endpoint so the listing inherits a real
   * name instead of rendering a generic "Strategy #N".
   */
  name: string;
  listable: ListabilityCheck[];
  tier: Tier;
  priceUsdc: number | null;
  acceptedPayers: { humans: boolean; agents: boolean };
  ingredients: Ingredient[];
  preview: ListingRow;
}

export interface MarketplaceStats {
  totalStrategies: number;
  paidThisWeekUsd: number;
  agentPurchases: number;
  mintedLast24h: number;
}
export interface PurchaseEvent {
  listingId: Id;
  version: string;
  buyer: string;
  payerKind: PayerKind;
  amountUsdc: number;
  netToCreatorUsdc: number;
  at: IsoDateTime;
}

// Fixture viewer/account state. Wallet-connect is Phase 6 (A5). Components
// derive: canClone = tier === "open" || ownedListingIds.includes(id) (A10);
// "Mine" segment rows = createdListingIds.
export interface Viewer {
  isConnected: boolean;
  address?: string;
  handle?: string;
  createdListingIds: Id[];
  ownedListingIds: Id[];
}
