// src/features/marketplace/data/fixtures/slices.ts
import type { Slice } from "../types";

// Slice DEFINITIONS only. The `count` here is a non-authoritative seed: every
// MarketplaceData client (fixture, api, subgraph) recomputes the live count
// from its real pool in getSlices(), so the chip strip never shows a stale,
// fabricated figure (no hardcoded 1,247 against a 6-entry collection). Seeded
// to 0 so any path that somehow reads these raw is visibly "not yet counted"
// rather than a confident lie.
export const SLICES: Slice[] = [
  { id: "trending", label: "Trending", hint: "weighted by 24h velocity × return", count: 0, filter: { segment: "trending", sort: "return30d" } },
  { id: "sol-7d", label: "Top on SOL · 7d", hint: "asset=SOL · 7d", count: 0, filter: { assets: ["SOL"], sort: "return30d" } },
  { id: "claude", label: "Top with Claude", hint: "model=Claude", count: 0, filter: { models: ["Claude · Haiku 4.5"], sort: "return30d" } },
  { id: "agents", label: "Most agent-bought", hint: "sort by agent purchases", count: 0, filter: { sort: "buyers", trust: { verifiedOnly: false, acceptsAgents: true, auditedOnly: false } } },
  { id: "newest", label: "Newest 24h", hint: "recently minted", count: 0, filter: { sort: "newest" } },
  { id: "free", label: "Free-tier breakouts", hint: "Free-tier · top 30d return", count: 0, filter: { tier: ["open"], sort: "return30d" } },
];
