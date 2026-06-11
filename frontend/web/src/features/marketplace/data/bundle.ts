// src/features/marketplace/data/bundle.ts
// Shared hook over `GET /api/marketplace/listings/:id/bundle` (open-tier
// verified manifest fetch). The route returns the FULL canonical Strategy
// JSON under `manifest`; the human-readable fields live one level deeper at
// `manifest.manifest.*` (PublicManifest). Models are `attested_with` — the
// canonical manifest has no `required_models` field (legacy name, rejected
// by the engine deserializer).
import { useQuery } from "@tanstack/react-query";
import { apiFetch } from "@/api/client";

/** PublicManifest fields the marketplace surfaces. All optional defensively —
 *  the bundle is author-supplied JSON. */
export interface BundleManifestFields {
  display_name?: string;
  plain_summary?: string;
  creator?: string;
  attested_with?: string[];
  required_tools?: string[];
}

/** Mirrors `BundleOut` in marketplace_read.rs. `manifest` is the canonical
 *  Strategy JSON whose own top-level `manifest` key is the PublicManifest. */
export interface BundleOut {
  listing_id: number;
  content_uri: string;
  verified: boolean;
  manifest: { manifest?: BundleManifestFields };
}

/** On-chain listing ids are numeric; fixture ids are slugs. */
export function isOnChainListingId(id: string | undefined): id is string {
  return !!id && /^\d+$/.test(id);
}

/** One requirement derived from the manifest (no installed-state claim —
 *  the chain doesn't tell us what the viewer has installed). */
export interface Requirement {
  name: string;
  kind: "model" | "tool";
}

export function requirementsFromManifest(
  manifest: BundleManifestFields | null | undefined,
): Requirement[] {
  if (!manifest) return [];
  return [
    ...(manifest.attested_with ?? []).map(
      (name): Requirement => ({ name, kind: "model" }),
    ),
    ...(manifest.required_tools ?? []).map(
      (name): Requirement => ({ name, kind: "tool" }),
    ),
  ];
}

/**
 * Fetch the verified manifest for an on-chain listing. Disabled for fixture
 * (non-numeric) ids; any error (404, 409, 503, indexer absent) resolves to
 * `null` so callers simply render nothing extra.
 */
export function useBundleManifest(
  listingId: string | undefined,
): BundleManifestFields | null {
  const enabled = isOnChainListingId(listingId);
  const { data } = useQuery({
    queryKey: ["marketplace", "bundle", listingId],
    queryFn: () =>
      apiFetch<BundleOut>(`/api/marketplace/listings/${listingId}/bundle`),
    enabled,
    retry: false,
  });
  return data?.manifest?.manifest ?? null;
}
