// src/features/marketplace/hooks/useFilterState.ts
import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";
import { defaultFilterState } from "@/features/marketplace/data/filter";
import type { FilterState, SortKey } from "@/features/marketplace/data/types";

const SORTS: SortKey[] = ["return30d", "sharpe", "buyers", "mostCloned", "newest"];
const list = (v: string | null) => (v ? v.split(",").filter(Boolean) : []);

function parse(sp: URLSearchParams): FilterState {
  const base = defaultFilterState();
  const sort = sp.get("sort");
  return {
    ...base,
    segment: (sp.get("segment") as FilterState["segment"]) ?? base.segment,
    search: sp.get("q") ?? "",
    sort: sort && (SORTS as string[]).includes(sort) ? (sort as SortKey) : base.sort,
    assets: list(sp.get("assets")),
    models: list(sp.get("models")),
    styles: list(sp.get("styles")),
    trust: {
      verifiedOnly: sp.get("verified") === "1",
      acceptsAgents: sp.get("agents") === "1",
      auditedOnly: sp.get("audited") === "1",
    },
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
  if (f.trust.verifiedOnly) out.verified = "1";
  if (f.trust.acceptsAgents) out.agents = "1";
  if (f.trust.auditedOnly) out.audited = "1";
  if (f.minBuyers) out.minBuyers = String(f.minBuyers);
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
