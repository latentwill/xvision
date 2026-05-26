// src/features/marketplace/data/fixtures/receipts.ts
import type { Receipt } from "../types";

export const RECEIPTS: Record<string, Receipt> = {
  "0xdemo-tx": {
    txHash: "0xdemo-tx", network: "mantle-sepolia", at: "2026-05-26T14:42:00Z", buyer: "0x7c2e…aa07",
    listing: { id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" }, genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, buyers: { humans: 247, agents: 14 } },
    license: { tokenId: "#0184", contract: "0xCa5522Be", manifestHash: "blake3:7f2b1ad91c4", bundleCid: "bafybeib4xjq2y7l", pricePaidUsdc: 49, feeUsdc: 2.45, netToCreatorUsdc: 46.55, mintedAt: "2026-05-26T14:42:00Z" },
    install: {
      xvnDetected: true, xvnEndpoint: "localhost:3000",
      ingredients: [
        { name: "Claude Haiku 4.5", kind: "model", installed: true },
        { name: "Birdeye MCP", kind: "mcp", installed: false },
        { name: "SOL Strategist skill", kind: "skill", installed: false },
        { name: "Mantlescan MCP", kind: "mcp", installed: true },
      ],
    },
    share: {
      ogCard: { id: "btc-momentum-v3", version: "v3.0", creator: { address: "0xa83e", handle: "@ed" }, genArtSeed: "btc-momentum-7a91-v3", return30dPct: 47.2, return30dLabel: "30D", buyers: { humans: 247, agents: 14 }, paidToCreatorUsd: 1240, priceUsdc: 49, verification: "verified", acceptsX402: true, promise: "BTC momentum with Claude regime detection.", url: "xvn.market/lineage/btc-momentum-v3" },
      buyerStamp: "just bought by 0x7c…aa07",
      caption: "I just bought btc-momentum-v3 by @ed — running it now. +47.2% in 30d · 247 humans + 14 agents already running it.",
      variants: ["Just got handed +47% by an autonomous agent", "247 humans run this. Me too now", "@ed's btc-momentum is real. screenshot proof:"],
      notificationHint: "@ed's XVN just got a +$46.55 notification",
    },
  },
};
