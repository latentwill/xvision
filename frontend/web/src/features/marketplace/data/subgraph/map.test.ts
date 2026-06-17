// src/features/marketplace/data/subgraph/map.test.ts
import { describe, expect, it } from "vitest";
import type { SgListing } from "./client";
import {
  buyerCounts,
  mapListingDetail,
  mapListingRow,
  mapStats,
  priceUsdcOrNull,
  tierLabel,
  txHashFromId,
  type SgListingFull,
} from "./map";

const baseAgent = {
  id: "42",
  owner: "0x00000000000000000000000000000000000000ab",
  manifestCid: "ipfs://cid",
  validations: [{ id: "v1" }],
};

const paidListing: SgListing = {
  id: "7",
  seller: "0x00000000000000000000000000000000000000ab",
  contentHash: "0xdead",
  tier: 0,
  priceUSDC: "1500000", // 1.5 USDC
  protocolFeeBps: 500,
  revoked: false,
  agent: baseAgent,
  sales: [
    { id: "s1", purchasePath: 0 },
    { id: "s2", purchasePath: 1 },
    { id: "s3", purchasePath: 1 },
  ],
  attestations: [{ id: "a1" } as never],
};

describe("primitives", () => {
  it("tierLabel maps 0/1", () => {
    expect(tierLabel(0)).toBe("open");
    expect(tierLabel(1)).toBe("sealed");
  });

  it("priceUsdcOrNull: paid → float, open+zero → null, sealed+zero → 0", () => {
    expect(priceUsdcOrNull("1500000", 0)).toBe(1.5);
    expect(priceUsdcOrNull("0", 0)).toBeNull();
    expect(priceUsdcOrNull("0", 1)).toBe(0);
  });

  it("buyerCounts splits by purchasePath", () => {
    expect(buyerCounts(paidListing.sales)).toEqual({ humans: 1, agents: 2 });
    expect(buyerCounts(undefined)).toEqual({ humans: 0, agents: 0 });
  });

  it("txHashFromId strips the log index", () => {
    expect(txHashFromId("0xabc-3")).toBe("0xabc");
    expect(txHashFromId("0xabc")).toBe("0xabc");
  });
});

describe("mapListingRow", () => {
  it("projects on-chain facts and honest off-chain defaults", () => {
    const row = mapListingRow(paidListing, null);
    expect(row.id).toBe("7");
    expect(row.lineageId).toBe("42");
    expect(row.creator.address).toBe(baseAgent.owner);
    expect(row.priceUsdc).toBe(1.5);
    expect(row.tier).toBe("open");
    expect(row.buyers).toEqual({ humans: 1, agents: 2 });
    expect(row.verification).toBe("verified"); // has a validation
    expect(row.acceptsX402).toBe(true); // positive price
    // QA11: genArtSeed from agent.id
    expect(row.genArtSeed).toBe("42");
    // off-chain (no manifest/eval): defaults, not fabricated
    expect(row.model).toBe("—");
    expect(row.assets).toEqual([]);
    expect(row.sharpe).toBe(0);
    expect(row.return30dPct).toBe(0);
    expect(row.transferableLicense).toBe(false);
    // QA9: name is undefined when no manifest
    expect(row.name).toBeUndefined();
  });

  it("applies resolved manifest metadata when present", () => {
    const row = mapListingRow(paidListing, {
      name: "BTC Momentum v3",
      model: "kimi-k2",
      assets: ["ETH/USD"],
      style: "breakout",
    });
    expect(row.model).toBe("kimi-k2");
    expect(row.assets).toEqual(["ETH/USD"]);
    expect(row.style).toBe("breakout");
    // QA9: name from manifest
    expect(row.name).toBe("BTC Momentum v3");
  });

  it("QA11: genArtSeed falls back to listing id when agent.id is empty", () => {
    const noAgentId: SgListing = {
      ...paidListing,
      agent: { ...baseAgent, id: "" },
    };
    const row = mapListingRow(noAgentId, null);
    // Falls back to listing id "7"
    expect(row.genArtSeed).toBe("7");
  });

  it("unverified when no validations; x402 from sales when free", () => {
    const free: SgListing = {
      ...paidListing,
      tier: 0,
      priceUSDC: "0",
      agent: { ...baseAgent, validations: [] },
      sales: [{ id: "s1", purchasePath: 1 }],
    };
    const row = mapListingRow(free, null);
    expect(row.priceUsdc).toBeNull();
    expect(row.verification).toBe("unverified");
    expect(row.acceptsX402).toBe(true); // an x402 sale exists
  });
});

describe("mapListingDetail", () => {
  const detail: SgListingFull = {
    id: "7",
    seller: baseAgent.owner,
    contentHash: "0xfeed",
    tier: 0,
    priceUSDC: "1500000",
    protocolFeeBps: 500,
    revoked: false,
    agent: { ...baseAgent, reputation: [], validations: [] },
    sales: [
      {
        id: "0xtx1-0",
        buyer: "0x1111111111111111111111111111111111111111",
        priceUSDC: "1500000",
        sellerProceeds: "1425000",
        protocolProceeds: "75000",
        purchasePath: 1,
        blockTimestamp: "1700000000",
      },
    ],
    attestations: [
      {
        id: "0xtxA-2",
        attester: "0x2222222222222222222222222222222222222222",
        evalResultHash: "0xhash",
        schema: "0xschema",
        postedAt: "1700000500",
      },
    ],
  };

  it("populates factual fields and leaves analytics empty", () => {
    const d = mapListingDetail(detail, { description: "does a thing" });
    expect(d.id).toBe("7");
    expect(d.platformFeeBps).toBe(500);
    // promise comes from meta.description (no fabrication)
    expect(d.promise).toBe("does a thing");
    expect(d.paidToCreatorUsd).toBeCloseTo(1.425, 3);
    expect(d.onChain.nft.tokenId).toBe("42");
    expect(d.onChain.nft.manifestHash).toBe("0xfeed");
    expect(d.onChain.nft.network).toBe("mantle-sepolia");
    // attestation surfaces as a factual anchor (no fabricated verdict)
    expect(d.onChain.attestations).toEqual([]);
    expect(d.onChain.anchors).toHaveLength(1);
    expect(d.onChain.anchors[0].tx).toBe("0xtxA");
    // recent buyer derived from a sale
    expect(d.recentBuyers).toHaveLength(1);
    expect(d.recentBuyers[0].payerKind).toBe("agent");
    // analytics stay zero/empty
    expect(d.metrics.sharpe).toBe(0);
    expect(d.equityCurve.points).toEqual([]);
    expect(d.onChain.trades).toEqual([]);
  });

  it("promise is empty string when meta is null (no fabrication)", () => {
    const d = mapListingDetail(detail, null);
    expect(d.promise).toBe("");
  });
});

describe("mapStats", () => {
  it("counts listings and agent purchases, windows recent activity", () => {
    const now = 1700000600;
    const stats = mapStats(
      {
        agents: [{ id: "1" }, { id: "2" }],
        listings: [{ id: "7" }],
        sales: [
          { id: "s1", purchasePath: 1, blockTimestamp: String(now - 100) },
          { id: "s2", purchasePath: 0, blockTimestamp: String(now - 2 * 24 * 3600) },
        ],
        _meta: { block: { number: 10, timestamp: now } },
      },
      now,
    );
    expect(stats.totalStrategies).toBe(1);
    expect(stats.agentPurchases).toBe(1);
    expect(stats.mintedLast24h).toBe(1); // only the recent sale is in-window
  });
});
