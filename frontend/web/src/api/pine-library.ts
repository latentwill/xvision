// Pine Script library API — wraps `GET /api/strategy/pine-library` and
// `POST /api/strategy/pine-library/{id}/import`.
// Types mirror the WU9 contract (see plan Part 4, WU9).

import { apiFetch } from "./client";
import type { PineImportResult } from "./pine-import";

// ─── Library types ───────────────────────────────────────────────────────────

/** Summary of a curated library entry (no source text). */
export type LibraryEntrySummary = {
  id: string;
  name: string;
  description: string;
};

export type PineLibraryListResponse = {
  items: LibraryEntrySummary[];
};

// ─── API calls ───────────────────────────────────────────────────────────────

/**
 * GET /api/strategy/pine-library
 *
 * Returns the list of curated library entry summaries (id, name, description).
 * Source text is NOT included — use `importLibraryEntry` to import.
 */
export function getPineLibrary(): Promise<PineLibraryListResponse> {
  return apiFetch<PineLibraryListResponse>("/api/strategy/pine-library");
}

/**
 * POST /api/strategy/pine-library/{id}/import
 *
 * Imports a curated library entry by its stable id.
 * Returns the same `{ strategy, fidelity_report }` as the Pine import route.
 * Throws ApiError with status 404 when the id is not found.
 */
export function importLibraryEntry(id: string): Promise<PineImportResult> {
  return apiFetch<PineImportResult>(
    `/api/strategy/pine-library/${encodeURIComponent(id)}/import`,
    { method: "POST" },
  );
}
