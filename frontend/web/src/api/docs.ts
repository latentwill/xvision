// Thin wrapper around the dashboard's `/api/docs/*` surface.
// Implements the `v2a-in-app-docs` contract.
//
// The backend bakes a small set of curated markdown pages into the
// binary; this client just fetches the index and an individual page
// body. No streaming, no auth, no headers beyond what `apiFetch` /
// `fetch` already sets.

import { apiFetch } from "./client";

export type DocPageMeta = {
  slug: string;
  title: string;
};

/** Fetch the ordered list of in-app docs pages. */
export function getDocsIndex(): Promise<DocPageMeta[]> {
  return apiFetch<DocPageMeta[]>("/api/docs/index");
}

/**
 * Fetch the raw markdown body for a single doc page.
 *
 * Throws on a non-2xx response so the caller can render an inline
 * error state without swallowing 404s for unknown slugs.
 */
export async function getDocsPage(slug: string): Promise<string> {
  const res = await fetch(`/api/docs/page/${encodeURIComponent(slug)}`);
  if (!res.ok) {
    throw new Error(`docs page '${slug}' failed: ${res.status}`);
  }
  return res.text();
}

export const docsKeys = {
  all: ["docs"] as const,
  index: () => [...docsKeys.all, "index"] as const,
  page: (slug: string) => [...docsKeys.all, "page", slug] as const,
};
