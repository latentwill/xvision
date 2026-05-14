// Search API — thin client for the dashboard's `GET /api/search`. Powers
// the command palette (⌘K). Empty query returns the most-recently-touched
// artifacts; `kind` filters to a single artifact kind.

import { apiFetch } from "./client";

export type SearchKind =
  | "strategy"
  | "run"
  | "finding"
  | "scenario"
  | "deployment"
  | "journal_entry"
  | "action";

export type SearchHit = {
  artifact_id: string;
  kind: SearchKind;
  title: string;
  summary: string;
  tags: string[];
  updated_at: string;
  href: string;
  bm25_score: number;
};

type SearchResponse = {
  hits: SearchHit[];
};

export type SearchParams = {
  q: string;
  kind?: SearchKind;
  limit?: number;
};

export function searchArtifacts(params: SearchParams): Promise<SearchHit[]> {
  const qs = new URLSearchParams();
  qs.set("q", params.q);
  if (params.kind) qs.set("kind", params.kind);
  if (params.limit) qs.set("limit", String(params.limit));
  return apiFetch<SearchResponse>(`/api/search?${qs.toString()}`).then(
    (r) => r.hits,
  );
}
