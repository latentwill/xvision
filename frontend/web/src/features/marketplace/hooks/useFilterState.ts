// src/features/marketplace/hooks/useFilterState.ts
import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { defaultFilterState } from "@/features/marketplace/data/filter";
import type { FilterState, SortKey, Tier } from "@/features/marketplace/data/types";

const SORTS: SortKey[] = ["return30d", "sharpe", "buyers", "newest"];
const SEGMENTS = ["trending", "new", "mine"] as const;
const TIERS: Tier[] = ["open", "sealed"];
const list = (v: string | null) => (v ? v.split(",").filter(Boolean) : []);

function parse(sp: URLSearchParams): FilterState {
  const base = defaultFilterState();
  const sort = sp.get("sort");
  const seg = sp.get("segment");
  const priceRaw = sp.get("price");
  const priceMatch = priceRaw?.match(/^(\d+)-(\d+)$/);
  return {
    ...base,
    segment: seg && (SEGMENTS as readonly string[]).includes(seg) ? (seg as FilterState["segment"]) : base.segment,
    search: sp.get("q") ?? "",
    sort: sort && (SORTS as string[]).includes(sort) ? (sort as SortKey) : base.sort,
    assets: list(sp.get("assets")),
    models: list(sp.get("models")),
    styles: list(sp.get("styles")),
    tier: list(sp.get("tier")).filter((v): v is Tier => (TIERS as string[]).includes(v)),
    trust: {
      verifiedOnly: sp.get("verified") === "1",
      acceptsAgents: sp.get("agents") === "1",
      auditedOnly: sp.get("audited") === "1",
    },
    priceUsdc: priceMatch
      ? { from: Number(priceMatch[1]), to: Number(priceMatch[2]) }
      : base.priceUsdc,
    minBuyers: Number(sp.get("minBuyers") ?? 0) || 0,
    slice: sp.get("slice") ?? undefined,
  };
}

function serialize(f: FilterState): Record<string, string> {
  const out: Record<string, string> = {};
  if (f.segment !== "trending") out.segment = f.segment;
  if (f.search) out.q = f.search;
  if (f.sort !== "return30d") out.sort = f.sort;
  if (f.assets.length) out.assets = f.assets.join(",");
  if (f.models.length) out.models = f.models.join(",");
  if (f.styles.length) out.styles = f.styles.join(",");
  if (f.tier.length) out.tier = f.tier.join(",");
  if (f.trust.verifiedOnly) out.verified = "1";
  if (f.trust.acceptsAgents) out.agents = "1";
  if (f.trust.auditedOnly) out.audited = "1";
  if (f.minBuyers) out.minBuyers = String(f.minBuyers);
  if (f.priceUsdc.from !== 0 || f.priceUsdc.to !== 500) out.price = `${f.priceUsdc.from}-${f.priceUsdc.to}`;
  if (f.slice) out.slice = f.slice;
  return out;
}

export function useFilterState() {
  const [sp, setSp] = useSearchParams();
  const filter = useMemo(() => parse(sp), [sp]);
  const setFilter = useCallback(
    (patch: Partial<FilterState>) => setSp(serialize({ ...filter, ...patch }), { replace: true }),
    [filter, setSp],
  );
  return { filter, setFilter };
}
