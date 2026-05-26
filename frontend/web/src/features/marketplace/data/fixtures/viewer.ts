// src/features/marketplace/data/fixtures/viewer.ts
import type { Viewer } from "../types";

// Fixture demo account. Wallet-connect (real viewer) is Phase 6 (A5).
export const VIEWER: Viewer = {
  isConnected: true,
  address: "0xa83e7c2efabb91d4eea7c2efbb91d4eef12d4",
  handle: "@ed",
  createdListingIds: ["btc-momentum-v3", "btc-grid-v2", "eth-mr-v2"],
  ownedListingIds: ["sol-strategist-pro"],
};
