// src/features/marketplace/data/fixtures/slices.ts
import type { Slice } from "../types";

export const SLICES: Slice[] = [
  { id: "trending", label: "Trending", hint: "weighted by 24h velocity × return", count: 1247, filter: { segment: "trending", sort: "return30d" } },
  { id: "sol-7d", label: "Top on SOL · 7d", hint: "asset=SOL · 7d", count: 142, filter: { assets: ["SOL"], sort: "return30d" } },
  { id: "claude", label: "Top with Claude", hint: "model=Claude", count: 431, filter: { models: ["Claude · Haiku 4.5"], sort: "return30d" } },
  { id: "agents", label: "Most agent-bought", hint: "sort by agent purchases", count: 88, filter: { sort: "buyers", trust: { verifiedOnly: false, acceptsAgents: true, auditedOnly: false } } },
  { id: "newest", label: "Newest 24h", hint: "recently minted", count: 23, filter: { sort: "newest" } },
  { id: "cloned", label: "Most cloned", hint: "sort by clones", count: 64, filter: { sort: "mostCloned" } },
  { id: "free", label: "Free-tier breakouts", hint: "Top by 30d return", count: 17, filter: { sort: "return30d" } },
];
