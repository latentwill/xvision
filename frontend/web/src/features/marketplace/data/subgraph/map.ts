// src/features/marketplace/data/subgraph/map.ts
//
// Projects RAW subgraph entities (./client.ts) onto the view types (../types.ts).
//
// The marketplace data model is a THREE-source join:
//   1. subgraph        — on-chain facts: a listing exists, its price, tier,
//                        owner, sales, attestations, reputation. (this file)
//   2. manifest (CID)  — name / description / model, resolved from the listing's
//                        manifestCid (IPFS). Wired via ManifestResolver below;
//                        the publish/pin path (PinataDriver) is still deploy-
//                        gated, so today this resolves to null → safe defaults.
//   3. dashboard/eval  — performance metrics (sharpe, return %, equity, trades).
//                        Not yet linked on-chain; these stay zero/empty here.
//
// We populate (1) faithfully and (2) when a resolver is supplied. Fields owned
// by (3) are left at honest zero/empty defaults — NOT fabricated — so the UI
// shows real on-chain listings without inventing analytics. Each such field is
// flagged `// off-chain` below.

import type {
  ListingDetail,
  ListingRow,
  MarketplaceStats,
  Tier,
  Verification,
} from "../types";
import type {
  SgAgent,
  SgListing,
  SgSaleLite,
  SgStatsResponse,
} from "./client";
import { activeNetworkSlug } from "../../lib/chain";

// --- manifest seam --------------------------------------------------------

/** Off-chain metadata resolved from a listing's manifestCid (source 2). */
export interface ListingManifestMeta {
  name?: string;
  description?: string;
  model?: string;
  assets?: string[];
  style?: string;
}

export interface ManifestResolver {
  resolve(manifestCid: string): Promise<ListingManifestMeta | null>;
}

/** Default resolver until IPFS/manifest pinning is wired — yields no metadata. */
export const nullManifestResolver: ManifestResolver = {
  async resolve() {
    return null;
  },
};

// --- raw detail shapes (the richer Q_LISTING selection) -------------------

export interface SgValidationFull {
  id: string;
  validator: string;
  resultHash: string;
  tag: string;
  blockTimestamp: string;
}
export interface SgFeedbackFull {
  id: string;
  rater: string;
  value: string;
  tag1: string;
  revoked: boolean;
  blockTimestamp: string;
}
export interface SgSaleFull {
  id: string;
  buyer: string;
  priceUSDC: string;
  sellerProceeds: string;
  protocolProceeds: string;
  purchasePath: number;
  blockTimestamp: string;
}
export interface SgAttestationFull {
  id: string;
  attester: string;
  evalResultHash: string;
  schema: string;
  postedAt: string;
}
export interface SgListingFull {
  id: string;
  seller: string;
  contentHash: string;
  tier: number;
  priceUSDC: string;
  protocolFeeBps: number;
  revoked: boolean;
  agent: SgAgent & {
    validations?: SgValidationFull[];
    reputation?: SgFeedbackFull[];
  };
  sales?: SgSaleFull[];
  attestations?: SgAttestationFull[];
}

// --- primitives -----------------------------------------------------------

const USDC_UNIT = 1_000_000; // 6-dp

export function tierLabel(tier: number): Tier {
  return tier === 1 ? "sealed" : "open";
}

/** null => Tier-A open/free (price 0); otherwise USDC float. */
export function priceUsdcOrNull(priceUSDC: string, tier: number): number | null {
  const units = Number(priceUSDC);
  if (!Number.isFinite(units) || units <= 0) {
    return tierLabel(tier) === "open" ? null : 0;
  }
  return units / USDC_UNIT;
}

/** Buyer split from sales by purchasePath (0 = direct ≈ human, 1 = x402 ≈ agent).
 *  Approximate per the schema's payerKind caveat — counts paths, not identities. */
export function buyerCounts(sales: SgSaleLite[] | undefined): {
  humans: number;
  agents: number;
} {
  let humans = 0;
  let agents = 0;
  for (const s of sales ?? []) {
    if (s.purchasePath === 1) agents += 1;
    else humans += 1;
  }
  return { humans, agents };
}

function verificationOf(agent: { validations?: { id: string }[] }): Verification {
  return (agent.validations?.length ?? 0) > 0 ? "verified" : "unverified";
}

/** Extract the tx hash from a "txHash-logIndex" entity id. */
export function txHashFromId(id: string): string {
  const dash = id.lastIndexOf("-");
  return dash > 0 ? id.slice(0, dash) : id;
}

const LICENSE_CONTRACT_FALLBACK = "marketplace";
const NETWORK = activeNetworkSlug;

// --- row ------------------------------------------------------------------

export function mapListingRow(
  l: SgListing,
  meta: ListingManifestMeta | null,
): ListingRow {
  const price = priceUsdcOrNull(l.priceUSDC, l.tier);
  const acceptsX402 =
    (price ?? 0) > 0 || (l.sales ?? []).some((s) => s.purchasePath === 1);
  // QA11: agent.id is always present (it is the on-chain entity id); use it
  // as the seed with a defensive fallback to the listing id in case of an
  // empty value from a future schema change.
  const genArtSeed = l.agent.id || l.id;
  return {
    id: l.id,
    lineageId: l.agent.id,
    version: "v1", // off-chain: no version on-chain (manifest may refine)
    // QA9: populate name from the manifest when present.
    name: meta?.name,
    creator: { address: l.agent.owner },
    model: meta?.model ?? "—", // manifest
    style: meta?.style ?? "—", // manifest
    assets: meta?.assets ?? [], // manifest
    return30dPct: 0, // off-chain (eval)
    sharpe: 0, // off-chain (eval)
    buyers: buyerCounts(l.sales),
    priceUsdc: price,
    tier: tierLabel(l.tier),
    transferableLicense: false, // off-chain: not in the index (contract view)
    verification: verificationOf(l.agent),
    acceptsX402,
    clones: 0, // off-chain: no clone event indexed
    genArtSeed, // deterministic seed from the agent id
  };
}

// --- detail ---------------------------------------------------------------

export function mapListingDetail(
  l: SgListingFull,
  meta: ListingManifestMeta | null,
): ListingDetail {
  const row = mapListingRow(l as unknown as SgListing, meta);
  const sales = l.sales ?? [];
  const attestations = l.attestations ?? [];

  const recentBuyers = sales
    .slice()
    .sort((a, b) => Number(b.blockTimestamp) - Number(a.blockTimestamp))
    .slice(0, 10)
    .map((s) => ({
      label: `${s.buyer.slice(0, 6)}…${s.buyer.slice(-4)}`,
      payerKind: (s.purchasePath === 1 ? "agent" : "human") as
        | "agent"
        | "human",
      outcome: "—", // off-chain (eval)
      at: tsToIso(s.blockTimestamp),
    }));

  // Each on-chain attestation is a factual anchor. We do NOT synthesize a
  // verdict label (endorse/question/reject) — that lives in the eval result
  // the hash commits to, not in the index — so `attestations` stays empty and
  // the facts surface as anchors instead.
  const anchors = attestations.map((a) => ({
    kind: "commit" as const,
    label: "eval attestation",
    tx: txHashFromId(a.id),
    at: tsToIso(a.postedAt),
    gasEth: "0",
  }));

  const paidToCreatorUsd =
    sales.reduce((acc, s) => acc + Number(s.sellerProceeds || 0), 0) / USDC_UNIT;

  return {
    ...row,
    promise: meta?.description ?? "", // manifest
    metrics: {
      return30dPct: 0,
      sharpe: 0,
      winRatePct: 0,
      maxDrawdownPct: 0,
      avgDurationDays: 0,
    }, // off-chain (eval)
    paidToCreatorUsd,
    platformFeeBps: l.protocolFeeBps,
    ingredients: [], // off-chain (manifest)
    variants: [],
    recentBuyers,
    creatorOther: [],
    equityCurve: { base: 0, points: [] }, // off-chain (eval)
    whatYouGet: [],
    whatYouDont: [],
    onChain: {
      nft: {
        tokenId: l.agent.id,
        lineageId: l.agent.id,
        agentURI: l.agent.manifestCid,
        manifestHash: l.contentHash,
        parentLineage: null,
        bornAt: "",
        operatorSig: "",
        contract: LICENSE_CONTRACT_FALLBACK,
        network: NETWORK,
      },
      attestations: [], // verdict label not recoverable from the index (see above)
      anchors,
      trades: [], // off-chain (eval)
      tradesMeta: {
        totalOnChain: 0,
        lastAnchorAt: anchors.length ? anchors[anchors.length - 1].at : "",
        receiptKind: "eval-attestation",
        netPnlUsd: 0,
        window: "",
        anchorTx: anchors.length ? anchors[anchors.length - 1].tx : "",
      },
    },
  };
}

// --- stats ----------------------------------------------------------------

export function mapStats(
  r: SgStatsResponse,
  nowSecs: number,
): MarketplaceStats {
  const weekAgo = nowSecs - 7 * 24 * 3600;
  const dayAgo = nowSecs - 24 * 3600;
  const sales = r.sales ?? [];
  const agentPurchases = sales.filter((s) => s.purchasePath === 1).length;
  // mintedLast24h: count agents created in the last 24h is not directly indexed
  // (no createdAt on Agent); approximate with sales in the window as activity.
  const mintedLast24h = sales.filter(
    (s) => Number(s.blockTimestamp) >= dayAgo,
  ).length;
  // paidThisWeekUsd is not derivable without per-sale price in this light query;
  // left 0 (the detailed value comes from the eval/receipt seam).
  void weekAgo;
  return {
    totalStrategies: (r.listings ?? []).length,
    paidThisWeekUsd: 0, // off-chain (needs per-sale price aggregation)
    agentPurchases,
    mintedLast24h,
  };
}

// --- creator (factual subset) --------------------------------------------

export function mapCreatorListings(
  agents: (SgAgent & { listings?: SgListing[] })[],
  meta: ListingManifestMeta | null,
): { address: string; rows: ListingRow[] } | null {
  if (agents.length === 0) return null;
  const address = agents[0].owner;
  const rows: ListingRow[] = [];
  for (const a of agents) {
    for (const l of a.listings ?? []) {
      rows.push(mapListingRow({ ...l, agent: a }, meta));
    }
  }
  return { address, rows };
}

function tsToIso(secs: string): string {
  const n = Number(secs);
  if (!Number.isFinite(n) || n <= 0) return "";
  return new Date(n * 1000).toISOString();
}
