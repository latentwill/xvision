// src/features/marketplace/data/subgraph/client.ts
//
// Thin typed GraphQL client for the marketplace subgraph (authored in
// crates/xvision-marketplace/subgraph, deployed to Goldsky — runbook §2.9/§3.2).
// The endpoint is injected at build time via VITE_MARKETPLACE_SUBGRAPH_URL; when
// unset, the app falls back to the fixture client (see MarketplaceLayout).
//
// This layer returns RAW subgraph entity shapes. Projection onto the view types
// in ../types.ts lives in ./map.ts — kept separate so the mappers are unit
// testable without a network and the query strings stay declarative.

/** Raw Agent entity (subset selected by our queries). */
export interface SgAgent {
  id: string;
  owner: string;
  manifestCid: string;
  validations?: { id: string }[];
}

/** Raw Listing entity (subset). `agent` is always co-selected. */
export interface SgListing {
  id: string;
  seller: string;
  contentHash: string;
  tier: number;
  priceUSDC: string; // BigInt as decimal string (6-dp USDC units)
  protocolFeeBps: number;
  revoked: boolean;
  agent: SgAgent;
  sales?: SgSaleLite[];
  attestations?: SgAttestation[];
}

/** Minimal Sale projection used for buyer/x402 counts on a row. */
export interface SgSaleLite {
  id: string;
  purchasePath: number; // 0 = direct, 1 = x402
}

/** Raw EvalAttestation entity (subset). */
export interface SgAttestation {
  id: string;
  attester: string;
  evalResultHash: string;
  schema: string;
  postedAt: string;
}

/** Raw Feedback entity (subset). */
export interface SgFeedback {
  id: string;
  rater: string;
  value: string;
  tag1: string;
  revoked: boolean;
  blockTimestamp: string;
}

const LISTING_FIELDS = `
  id
  seller
  contentHash
  tier
  priceUSDC
  protocolFeeBps
  revoked
  agent { id owner manifestCid validations(first: 1) { id } }
  sales(first: 1000) { id purchasePath }
  attestations(first: 1) { id }
`;

export const Q_LISTINGS = `query Listings($first: Int!) {
  listings(first: $first, where: { revoked: false }) {${LISTING_FIELDS}}
}`;

export const Q_LISTING = `query Listing($id: ID!) {
  listing(id: $id) {
    id seller contentHash tier priceUSDC protocolFeeBps revoked
    agent {
      id owner manifestCid
      validations(first: 100) { id validator resultHash tag blockTimestamp }
      reputation(first: 100) { id rater value tag1 revoked blockTimestamp }
    }
    sales(first: 1000) { id buyer priceUSDC sellerProceeds protocolProceeds purchasePath blockTimestamp }
    attestations(first: 100) { id attester evalResultHash schema postedAt }
  }
}`;

export const Q_LISTINGS_BY_OWNER = `query ListingsByOwner($owner: Bytes!, $first: Int!) {
  agents(where: { owner: $owner }, first: $first) {
    id owner manifestCid
    listings(first: $first, where: { revoked: false }) {${LISTING_FIELDS}}
  }
}`;

// Lightweight counters for the stats strip. The Graph has no aggregate
// functions, so we select ids up to a cap and count client-side — fine at
// testnet scale. `_meta` exposes the indexer head for a freshness check.
export const Q_STATS = `query Stats($cap: Int!) {
  agents(first: $cap) { id }
  listings(first: $cap, where: { revoked: false }) { id }
  sales(first: $cap) { id purchasePath blockTimestamp }
  _meta { block { number timestamp } }
}`;

export interface SgStatsResponse {
  agents: { id: string }[];
  listings: { id: string }[];
  sales: { id: string; purchasePath: number; blockTimestamp: string }[];
  _meta?: { block?: { number?: number; timestamp?: number } };
}

/** Thrown on transport / GraphQL errors so callers can surface a clear state. */
export class SubgraphError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "SubgraphError";
  }
}

/** The subgraph endpoint, or null when unconfigured (→ fixture fallback). */
export function subgraphUrl(): string | null {
  // `import.meta.env` is replaced at build time by Vite. Read it through a cast
  // (matching src/api/agent-runs.ts) so tsc -b doesn't require vite/client types
  // and tests can flip it with `vi.stubEnv`.
  const meta = import.meta as unknown as {
    env?: Record<string, string | undefined>;
  };
  const url = meta.env?.VITE_MARKETPLACE_SUBGRAPH_URL;
  return url && url.trim().length > 0 ? url.trim() : null;
}

export interface SubgraphClient {
  query<T>(query: string, variables?: Record<string, unknown>): Promise<T>;
}

/**
 * Build a client bound to `url` (defaults to {@link subgraphUrl}). `fetchImpl`
 * is injectable for tests. Throws {@link SubgraphError} on non-2xx, network
 * failure, or a GraphQL `errors` payload.
 */
export function createSubgraphClient(
  url: string,
  fetchImpl: typeof fetch = fetch,
): SubgraphClient {
  return {
    async query<T>(
      query: string,
      variables: Record<string, unknown> = {},
    ): Promise<T> {
      let res: Response;
      try {
        res = await fetchImpl(url, {
          method: "POST",
          headers: { "content-type": "application/json" },
          body: JSON.stringify({ query, variables }),
        });
      } catch (e) {
        throw new SubgraphError(
          `subgraph request failed: ${(e as Error).message}`,
        );
      }
      if (!res.ok) {
        throw new SubgraphError(`subgraph HTTP ${res.status}`);
      }
      const body = (await res.json()) as {
        data?: T;
        errors?: { message: string }[];
      };
      if (body.errors && body.errors.length > 0) {
        throw new SubgraphError(
          `subgraph query error: ${body.errors.map((e) => e.message).join("; ")}`,
        );
      }
      if (body.data === undefined || body.data === null) {
        throw new SubgraphError("subgraph returned no data");
      }
      return body.data;
    },
  };
}
