// ReceiptRoute.real.test.tsx — the route must render a *real* mapped receipt
// (ApiMarketplaceData.getReceipt output: honest empties, no fixture-only
// fields) without crashing or showing fixture data.
import { screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { renderMarketplace } from "../test-utils";
import { FixtureMarketplaceData } from "@/features/marketplace/data/MarketplaceData";
import type { Receipt } from "@/features/marketplace/data/types";
import { ReceiptRoute } from "./ReceiptRoute";

const TX = ("0x" + "ab".repeat(32)) as `0x${string}`;
const ISO = new Date(1_750_000_000 * 1000).toISOString();

// Shape produced by ApiMarketplaceData.getReceipt for an on-chain Sold event.
const realReceipt: Receipt = {
  txHash: TX,
  network: "mantle-sepolia",
  at: ISO,
  buyer: "0x7c2e000000000000000000000000000000000007",
  listing: {
    id: "3",
    version: "v1",
    creator: { address: "" },
    genArtSeed: "seed-xyz",
    return30dPct: 0,
    buyers: { humans: 0, agents: 0 },
  },
  license: {
    tokenId: "3",
    contract: "0x4444444444444444444444444444444444444444",
    manifestHash: "",
    bundleCid: "",
    pricePaidUsdc: 49,
    feeUsdc: 2.45,
    netToCreatorUsdc: 46.55,
    mintedAt: ISO,
  },
  install: { xvnDetected: false, xvnEndpoint: "", ingredients: [] },
  share: {
    ogCard: {
      id: "3",
      version: "v1",
      creator: { address: "" },
      genArtSeed: "seed-xyz",
      return30dPct: 0,
      buyers: { humans: 0, agents: 0 },
      paidToCreatorUsd: 0,
      priceUsdc: 49,
      verification: "unverified",
      acceptsX402: true,
      promise: "BTC Momentum",
      url: "/marketplace/lineage/3",
    },
    buyerStamp: "bought by 0x7c2e…0007",
    caption: "I just bought BTC Momentum for 49 USDC on Mantle Sepolia.",
    variants: [],
    notificationHint: "",
  },
};

class RealReceiptClient extends FixtureMarketplaceData {
  async getReceipt(): Promise<Receipt> {
    return realReceipt;
  }
}

describe("ReceiptRoute with a real mapped receipt", () => {
  it("renders header, license card, install and share panels sanely", async () => {
    renderMarketplace(<ReceiptRoute />, {
      path: "/marketplace/receipts/:tx",
      route: `/marketplace/receipts/${TX}`,
      client: new RealReceiptClient(),
    });

    // Success header with the on-chain listing id + real amounts
    expect(await screen.findByText(/You bought/)).toBeInTheDocument();
    expect(screen.getAllByText(/49 USDC/).length).toBeGreaterThan(0);
    expect(screen.getByText(/46\.55 USDC/)).toBeInTheDocument();

    // License panel renders the real token + minted time, empty manifest is fine
    expect(screen.getByText("LICENSE 3")).toBeInTheDocument();
    expect(screen.getByText(ISO)).toBeInTheDocument();

    // Install panel: honest "not detected" state for the real receipt
    expect(screen.getAllByText(/XVN not detected/).length).toBeGreaterThan(0);

    // Tx link points at the real hash
    const link = screen.getByRole("link", { name: /view on mantlescan/i });
    expect(link).toHaveAttribute(
      "href",
      `https://sepolia.mantlescan.xyz/tx/${TX}`,
    );
  });
});
