// src/features/marketplace/data/fixtures/listings.ts
import type { ListingDetail, ListingRow } from "../types";

const ed = { address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4", handle: "@ed", ens: "ed.xvn" };
const vibe = { address: "0x9f12aa55bb77cc88dd99ee00ff11223344556677", handle: "@vibesharpe" };

export const NAMED_LISTINGS: ListingRow[] = [
  {
    id: "sol-strategist-pro", lineageId: "sol-strategist", version: "v4.2", name: "SOL Strategist Pro", creator: vibe,
    model: "Claude · Haiku 4.5", style: "Day", assets: ["SOL"], return30dPct: 89.4, sharpe: 1.84,
    buyers: { humans: 412, agents: 38 }, priceUsdc: 79, tier: "sealed", verification: "verified",
    acceptsX402: true, transferableLicense: false, genArtSeed: "sol-strategist-12fa",
  },
  {
    id: "meme-radar", lineageId: "meme-radar", version: "v1.0", name: "Meme Radar", creator: { address: "0xdead00beef", handle: "@degenray" },
    model: "GPT-5", style: "Momentum", assets: ["DOGE", "SOL"], return30dPct: 124.8, sharpe: 0.92,
    buyers: { humans: 88, agents: 12 }, priceUsdc: null, tier: "open", verification: "unverified",
    acceptsX402: true, transferableLicense: true, genArtSeed: "meme-radar-77aa",
  },
  {
    id: "doge-vol", lineageId: "doge-vol", version: "v1.1", name: "DOGE Volatility", creator: { address: "0xc0a4f3b2", handle: "@quantnext" },
    model: "Gemini 3 Pro", style: "Swing", assets: ["DOGE"], return30dPct: -2.3, sharpe: -0.18,
    buyers: { humans: 12, agents: 0 }, priceUsdc: 29, tier: "sealed", verification: "unverified",
    acceptsX402: false, transferableLicense: false, genArtSeed: "doge-vol-3b22",
  },
  {
    id: "btc-momentum-v3", lineageId: "btc-momentum", version: "v3.0", name: "BTC Momentum v3", creator: ed,
    model: "Claude · Haiku 4.5", style: "Day", assets: ["BTC"], return30dPct: 47.2, sharpe: 1.31,
    buyers: { humans: 247, agents: 14 }, priceUsdc: 49, tier: "sealed", verification: "verified",
    acceptsX402: true, transferableLicense: false, genArtSeed: "btc-momentum-7a91-v3",
  },
  {
    id: "btc-grid-v2", lineageId: "btc-grid", version: "v2.0", name: "BTC Grid v2", creator: ed,
    model: "Claude · Haiku 4.5", style: "Mean-reversion", assets: ["BTC"], return30dPct: 31.4, sharpe: 1.12,
    buyers: { humans: 134, agents: 9 }, priceUsdc: 39, tier: "sealed", verification: "verified",
    acceptsX402: false, transferableLicense: false, genArtSeed: "btc-grid-6f5b",
  },
  {
    id: "eth-mr-v2", lineageId: "eth-mr", version: "v2.0", name: "ETH Mean-Reversion v2", creator: ed,
    model: "Claude · Haiku 4.5", style: "Mean-reversion", assets: ["ETH"], return30dPct: 12.8, sharpe: 0.74,
    buyers: { humans: 88, agents: 3 }, priceUsdc: 0, tier: "open", verification: "unverified",
    acceptsX402: false, transferableLicense: true, genArtSeed: "eth-mr-3b22",
  },
];

// The curated demo collection — the small set the fixture client actually
// serves to browse/stats/slices/leaderboard. Wall strats (below) are at-scale
// validation fixtures only; they are NEVER part of the browse pool, because
// every wall-strat slug has no detail fixture and would link to the designed
// not-found page, undermining "Inspect freely".
export const DEMO_LISTINGS: ListingRow[] = NAMED_LISTINGS;

const WALL_ASSETS = ["BTC", "ETH", "SOL", "DOGE", "MNT", "AVAX"];
const WALL_MODELS = ["Claude · Haiku 4.5", "GPT-5", "Gemini 3 Pro", "Llama 4"];
const WALL_STYLES = ["Day", "Swing", "Momentum", "Mean-reversion", "Long/Short"];

// Deterministic 200-row set for the gen-art wall + at-scale validation.
export function makeWallListings(n = 200): ListingRow[] {
  const out: ListingRow[] = [];
  for (let i = 0; i < n; i++) {
    const ret = ((i * 37) % 220) - 20; // -20..+199
    out.push({
      id: `wall-strat-${i}`, lineageId: `wall-${i % 40}`, version: `v${(i % 4) + 1}.0`,
      creator: { address: `0x${(i * 2654435761 >>> 0).toString(16)}`, handle: `@maker${i % 30}` },
      model: WALL_MODELS[i % WALL_MODELS.length], style: WALL_STYLES[i % WALL_STYLES.length],
      assets: [WALL_ASSETS[i % WALL_ASSETS.length]], return30dPct: ret, sharpe: ((i % 30) - 5) / 10,
      buyers: { humans: (i * 7) % 500, agents: i % 25 },
      priceUsdc: i % 5 === 0 ? null : 10 + (i % 20) * 5,
      tier: i % 5 === 0 ? "open" : "sealed", verification: i % 3 === 0 ? "verified" : "unverified",
      acceptsX402: i % 2 === 0, transferableLicense: i % 7 === 0, genArtSeed: `wall-${i}-${(i * 2246822507 >>> 0).toString(36)}`,
    });
  }
  return out;
}

// QA1: wall fixtures (200 seeded rows) are only included in dev builds so the
// production client never shows placeholder data. In production, ALL_LISTINGS
// is just NAMED_LISTINGS. Keep makeWallListings exported for dev use.
export const ALL_LISTINGS: ListingRow[] = import.meta.env.DEV
  ? [...NAMED_LISTINGS, ...makeWallListings()]
  : [...NAMED_LISTINGS];

export const LISTING_DETAILS: Record<string, ListingDetail> = {
  "btc-momentum-v3": {
    ...NAMED_LISTINGS[3],
    promise: "BTC momentum with Claude regime detection. Holds 1–3 days, 2% risk cap.",
    metrics: { return30dPct: 47.2, sharpe: 1.31, winRatePct: 62, maxDrawdownPct: -8.4, avgDurationDays: 1.8 },
    paidToCreatorUsd: 1240, platformFeeBps: 500,
    ingredients: [
      { name: "Claude Haiku 4.5", kind: "model", installed: true },
      { name: "Birdeye MCP", kind: "mcp", installed: false },
      { name: "SOL Strategist skill", kind: "skill", installed: false },
      { name: "Mantlescan MCP", kind: "mcp", installed: true },
    ],
    variants: [
      { version: "v1.0", parent: null, genArtSeed: "btc-momentum-7a91-v1", sharpe: 0.9, current: false },
      { version: "v2.1", parent: "v1.0", genArtSeed: "btc-momentum-7a91-v2", sharpe: 1.1, current: false },
      { version: "v3.0", parent: "v2.1", genArtSeed: "btc-momentum-7a91-v3", sharpe: 1.31, current: true },
    ],
    clonesOfYours: { count: 8, upstreamEarningsUsd: 2100 },
    recentBuyers: [
      { label: "0x7c2e…aa07", payerKind: "human", outcome: "+12.4% · 6d", at: "2026-05-26T14:39:00Z" },
      { label: "agent #14", payerKind: "agent", outcome: "running · 2 trades", at: "2026-05-26T14:20:00Z" },
      { label: "0xc0a4…f3b2", payerKind: "human", outcome: "-0.6% · 1d", at: "2026-05-26T05:42:00Z" },
    ],
    creatorOther: [NAMED_LISTINGS[4], NAMED_LISTINGS[5]],
    equityCurve: {
      base: 1000,
      points: Array.from({ length: 90 }, (_, i) => ({
        value: 1000 + i * 6 + Math.sin(i / 5) * 40,
        phase: i < 60 ? ("backtest" as const) : ("live" as const),
      })),
    },
    whatYouGet: ["Full prompts", "Agent topology + ordering", "Threshold values", "MCP/skill config", "Creator notes"],
    whatYouDont: ["Creator's data sources", "Future updates without re-purchase", "Broker credentials"],
    onChain: {
      nft: {
        tokenId: "#0043", lineageId: "btc-momentum", agentURI: "ipfs://bafybeib4xjq2y7l",
        manifestHash: "blake3:7f2b1ad91c4", parentLineage: null, bornAt: "2026-05-13T04:12:00Z",
        operatorSig: "ed25519:7f2b1ad91c4", contract: "0xCa5522Be", network: "mantle-sepolia",
      },
      attestations: [
        { attester: "regime-verifier", verdict: "endorse", targetVersion: "v3.0", at: "2026-05-26T13:30:00Z" },
        { attester: "diversity-check", verdict: "endorse", targetVersion: "v3.0", at: "2026-05-26T13:31:00Z" },
        { attester: "diversity-check", verdict: "question", targetVersion: "v3.1", at: "2026-05-26T10:30:00Z" },
      ],
      anchors: [
        { kind: "merkle", label: "Snapshot · btc-momentum-v3", tx: "0x2e1d44a9", at: "2026-05-26T12:30:00Z", gasEth: "0.0024" },
        { kind: "mint", label: "Identity NFT minted", tx: "0xc0a4f3b2", at: "2026-05-22T05:00:00Z", gasEth: "0.0011" },
        { kind: "commit", label: "SessionCommitment 01H8RTZ", tx: "0x4f8aee01", at: "2026-05-21T18:00:00Z", gasEth: "0.0008" },
      ],
      trades: [
        { at: "2026-05-26T12:30:00Z", action: "close", symbol: "BTC", qty: "0.024", entry: 67420, exit: 68840, pnlUsd: 34.08, pnlPct: 2.1, runner: "0x7c2e…aa07", runnerKind: "human", tx: "0xa83ef12d", anchorTx: "0x2e1d44a9" },
        { at: "2026-05-26T09:00:00Z", action: "close", symbol: "BTC", qty: "0.018", entry: 66910, exit: 67820, pnlUsd: 16.38, pnlPct: 1.4, runner: "agent #14", runnerKind: "agent", tx: "0x4f8adc11", anchorTx: "0x2e1d44a9" },
      ],
      tradesMeta: { totalOnChain: 178, lastAnchorAt: "2026-05-26T12:30:00Z", receiptKind: "TradeBatch", netPnlUsd: 94.88, window: "7d", anchorTx: "0x2e1d44a9" },
    },
  },
};

// Synthesize a dignified ListingDetail from a ListingRow so EVERY curated
// entry is inspectable in the demo (not just the one hand-authored detail).
// Real on-chain listings ship their own backend detail; this is the demo-only
// path that keeps "Inspect freely" honest across the full collection.
// Performance is left as a designed empty record (no equity points, no trades)
// so the detail page renders the honest "pending first live cycle" empty state
// rather than a fabricated curve.
export function synthDetailFromRow(row: ListingRow): ListingDetail {
  const tokenId = `#${tokenId4(row.id)}`;
  return {
    ...row,
    promise: `${row.assets.join("/")} ${row.style.toLowerCase()} strategy on ${row.model}.`,
    metrics: {
      return30dPct: row.return30dPct,
      sharpe: row.sharpe,
      winRatePct: 0,
      maxDrawdownPct: 0,
      avgDurationDays: 0,
    },
    paidToCreatorUsd: 0,
    platformFeeBps: 500,
    ingredients: [{ name: row.model, kind: "model", installed: false }],
    variants: [
      { version: row.version, parent: null, genArtSeed: row.genArtSeed, sharpe: row.sharpe, current: true },
    ],
    recentBuyers: [],
    creatorOther: [],
    equityCurve: { base: 1000, points: [] },
    whatYouGet: ["Full prompts", "Agent topology + ordering", "Threshold values"],
    whatYouDont: ["Creator's data sources", "Future updates without re-purchase"],
    onChain: {
      nft: {
        tokenId,
        lineageId: row.lineageId,
        agentURI: `ipfs://${row.genArtSeed}`,
        manifestHash: `blake3:${row.genArtSeed}`,
        parentLineage: null,
        bornAt: "2026-05-13T04:12:00Z",
        operatorSig: `ed25519:${row.genArtSeed}`,
        contract: "0xCa5522Be",
        network: "mantle-sepolia",
      },
      attestations: [],
      anchors: [],
      trades: [],
      tradesMeta: {
        totalOnChain: 0,
        lastAnchorAt: row.return30dPct ? "2026-05-26T12:30:00Z" : "",
        receiptKind: "TradeBatch",
        netPnlUsd: 0,
        window: "7d",
        anchorTx: "",
      },
    },
  };
}

// Stable 4-digit NFT token id for a slug id.
function tokenId4(id: string): string {
  if (/^\d+$/.test(id)) return id.padStart(4, "0");
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return String(h % 10000).padStart(4, "0");
}

// Resolve a demo detail for a listing: prefer a hand-authored detail, otherwise
// synthesize one from the curated row. Returns undefined when the id is not in
// the curated collection at all (→ designed not-found state).
export function getDemoDetail(idOrName: string): ListingDetail | undefined {
  const explicit = LISTING_DETAILS[idOrName];
  if (explicit) return explicit;
  const row = DEMO_LISTINGS.find((r) => r.id === idOrName);
  return row ? synthDetailFromRow(row) : undefined;
}
