# Phase 1 — Marketplace Metadata & Data-Contract Spec

> **Purpose:** Harden the now-built (Phase F) frontend `MarketplaceData` seam into
> the **canonical data contract** every layer agrees on: the on-chain `tokenURI`
> (Tier 0), the IPFS-pinned public metadata (Tier 1), the sealed bundle (Tier 2),
> and the **chain events → subgraph** read path. Reconciles the three divergent
> source shapes (the seam, the contract-surface events/`Listing`, the plugin
> `LineageManifest`) into one schema, and locks the open decisions. This is the
> artifact the nav doc's Phase 1 always wanted; it is now writable because the
> frontend revealed exactly what each surface displays.
>
> **Status:** Draft for operator review. **Decisions A8 + A9 locked** (operator,
> 2026-05-26); other decisions carry a recommendation to confirm.
> **Parent:** [`2026-05-26-marketplace-program-strategy.md`](../plans/2026-05-26-marketplace-program-strategy.md)
> (this closes its Phase 1). **Frontend contract:** the seam at
> `frontend/web/src/features/marketplace/data/types.ts` (Phase F, on main).
> **Reconciles:** [`smart-contract-surface-design.md`](./2026-05-08-smart-contract-surface-design.md)
> (events/`Listing`/subgraph) · [`marketplace-plugin-design.md`](./2026-05-09-marketplace-plugin-design.md)
> (`LineageManifest`/receipts) · [`marketplace-design-direction.md`](../plans/2026-05-26-marketplace-design-direction.md) §6.2 (tiers).
>
> **Not in scope:** contract implementation (Phase 5), gen-art encoding internals
> (Phase 4 — only the `tokenURI` *slot* is reserved here), IPFS pinning ops
> (Phase 5, Pinata behind `IpfsStore`).

---

## 1. The lineage / variant model (read this first)

The single most important reconciliation. Per A4 (locked across both contract
specs): **one ERC-8004 NFT per *lineage*; *variants* are content-hash records
under it.** The frontend works in both terms — so the schema must too:

| Concept | On-chain / IPFS | Frontend seam | Example |
|---|---|---|---|
| **Lineage** | the NFT (`agentNftId`); its `agentURI` → the **lineage manifest** (Tier 1a) | `ListingRow.lineageId` | `btc-momentum` |
| **Variant** | a `Listing` (content-hash record under the NFT); its `contentURI` → the **listing metadata** (Tier 1b) | `ListingRow.id` (+ `version`) | `btc-momentum-v3` |

So there are **two Tier-1 documents**: a lineage-level manifest (one per NFT,
behind `agentURI`) and a variant-level listing metadata doc (one per listing,
behind `contentURI`). This resolves the apparent "6-field `LineageManifest` vs
rich Tier-1" conflict — they are **different documents at different levels**, not
two versions of one thing. The identity page (`/marketplace/lineage/:name`) is a
**variant** view; the lineage tree on it walks the variants under one NFT.

---

## 2. The storage tiers — canonical schemas

### 2.1 Tier 0 — on-chain (immutable, free to read forever)

Lives in the lineage NFT + the marketplace contracts. The viewer/subgraph reads
these; they are the trust root.

**Lineage NFT (`IdentityRegistry`):**
- `agentNftId` (uint256) — the lineage token id.
- `owner` (address) — the creator (canonical identity; A8).
- `agentURI` (string) — `ipfs://<cid>` → the **lineage manifest** (Tier 1a).
- `tokenURI` (string) — JSON containing the **gen-art** as a `data:image/svg+xml`
  URI. **The gen-art bytes are Phase 4** — Phase 1 only reserves the slot and the
  seed inputs (`agentNftId` + `manifestHash`). Shape:
  ```json
  { "name": "<lineage>", "description": "...", "image": "data:image/svg+xml;base64,<PHASE-4>",
    "external_url": "<viewer-domain>/marketplace/lineage/<headVariant>",
    "attributes": [{ "trait_type": "lineage", "value": "<lineage>" }] }
  ```
- `parent_lineage_id` (uint256 | 0) — clone provenance edge (A10).

**Marketplace `Listing` (per variant, `ListingRegistry`)** — from the contract
surface §3.1, this is the on-chain anchor for each variant:
`listingId, seller, agentNftId, contentHash, contentURI, tier (0=open/1=sealed),
priceUSDC, protocolFeeBps, transferableLicense, createdAt, revoked`.
- `contentHash` (blake3 digest stored as `bytes32`) — commits the **variant's
  full content** (Tier 2 bundle); swapping content under a live listing is
  detectable. This intentionally supersedes the older contract-surface
  `keccak256` comment; Phase 5 must align the contract docs + bindings.
- `contentURI` — `ipfs://<cid>` → the **listing metadata** (Tier 1b).
- `createdAt` → surfaces as `publishedAt` (resolves the F1 `newest`-sort gap; **no
  new field needed** — map it through).
- `transferableLicense` — default `false` (direction); already in the struct.

**On-chain hash commitments (Tier-0 trust anchors):**
- `manifestHash` — blake3 of the lineage manifest (Tier 1a).
- `contentHash` — blake3 of the sealed bundle (Tier 2).
- **`perfHash`** — blake3 of the performance summary block in Tier 1b, committed
  at publish time so creators can't retro-edit numbers. **(A9 evidence.)**
- ValidationRegistry / ReputationRegistry receipts — per-cycle PnL hashes,
  attestations (the verification evidence, see §4 A9).

### 2.2 Tier 1a — lineage manifest (IPFS-pinned, behind `agentURI`)

The plugin's `LineageManifest`, adopted as-is with explicit fields:
```json
{
  "schema": "xvn.lineage.v1",
  "lineage_id": "<ulid>",
  "initial_bundle_hash": "blake3:…",
  "parent_lineage_id": "<ulid|null>",     // clone provenance (A10)
  "born_at": "<iso8601>",
  "operator_signature": "ed25519:…",
  "autooptimizer_session_id": "<ulid>",
  "creator": { "address": "0x…", "handle": "@ed", "ens": "ed.xvn" }  // A8: address canonical, handle/ens optional display
}
```

### 2.3 Tier 1b — listing metadata (IPFS-pinned, behind `contentURI`)

The **marketing-copy tier** — everything a buyer needs to decide, per direction
§6.2 Tier 1. One doc per variant/listing. Every field below backs a seam field
(see §5 mapping):
```json
{
  "schema": "xvn.listing.v1",
  "id": "btc-momentum-v3", "lineage_id": "btc-momentum", "version": "v3.0",
  "creator": { "address": "0x…", "handle": "@ed", "ens": "ed.xvn" },   // A8
  "model": "Claude · Haiku 4.5", "style": "Day",
  "asset_tags": ["BTC"], "style_tags": ["Day","Momentum"],
  "promise": "BTC momentum with Claude regime detection. …",
  "published_at": "<iso8601>",                                          // == Listing.createdAt
  "performance": {                                                      // hashed → perfHash on-chain (A9)
    "return_30d_pct": 47.2, "sharpe": 1.31, "win_rate_pct": 62,
    "max_drawdown_pct": -8.4, "avg_duration_days": 1.8,
    "equity_curve_cid": "ipfs://…",                                     // CSV/JSON, base $1000, backtest+live segments
    "backtest_result_cid": "ipfs://…",
    "live_paper_days": 34,
    "positive_closed_cycle_hash": "0x…"                                 // must match on-chain Validation/Reputation receipt
  },
  "required_ingredients": [                                             // drives the ingredient check
    { "name": "Claude Haiku 4.5", "kind": "model" },
    { "name": "Birdeye MCP", "kind": "mcp" },
    { "name": "SOL Strategist skill", "kind": "skill" } ],
  "license": { "tier": "sealed", "price_usdc": 49, "transferable": false, "perpetual": true },
  "x402": true,                                                         // accepts agent-paid auto-purchase
  "what_you_get": ["Full prompts","Agent topology + ordering", …],
  "what_you_dont": ["Creator data sources","Future updates without re-purchase", …],
  "rating_receipts": ["ipfs://…"]                                       // attestation rationale pointers
}
```
**Note:** `verification.status`, `audited`, buyer counts, and lineage-tree edges
are **not authored** here — they are **derived** from on-chain events/receipts by
the subgraph (§4). Tier 1b carries only creator-authored fields plus
perf-committed evidence pointers. This keeps the trust boundary clean: numbers a
creator could fake are either hash-committed (perf) or chain-derived (buyers,
verification, audit).

### 2.4 Tier 2 — sealed bundle (Tier-B listings only, paywalled)

The actual content; encrypted client-side, IPFS-pinned, decryption gated by a
relay verifying `LicenseToken.balanceOf(buyer) ≥ 1` (A7 relay = Phase 5/6).
```json
{ "schema": "xvn.bundle.v1", "id": "btc-momentum-v3",
  "prompts": [...], "agent_topology": {...}, "thresholds": {...},
  "mcp_config": {...}, "skill_config": {...}, "creator_notes": "..." }
```
Committed by `contentHash` on-chain (§2.1). Tier-A (open) listings skip
encryption + relay and expose this directly under `contentURI`.

### 2.5 Tier 3 — never shared
Creator journal, research scratch, broker creds, proprietary feeds, deleted-prompt
history. Stays in the self-hosted XVN. Never bundled.

---

## 3. Events & subgraph — the read path

The viewer never reads contracts directly for lists; it reads a **subgraph**.
The events below must fully hydrate the read model for the fields they own; the
subgraph may call contracts for repair/backfill, but required list fields do not
depend on contract calls during normal indexing. Event schemas (contract surface
§3/§6.3, **amended here**):

| Event | Fields | Feeds |
|---|---|---|
| `ListingCreated` | `listingId, seller, agentNftId, contentHash, contentURI, tier, priceUSDC, protocolFeeBps, transferableLicense, createdAt` | listing rows, `publishedAt`, metadata indexing |
| `ListingUpdated` | `listingId, contentHash, contentURI` | re-pin / re-index |
| `ListingRevoked` | `listingId, seller` | hide listing |
| `Sold` **(amended)** | `listingId, agentNftId, buyer, priceUSDC, sellerProceeds, protocolProceeds, licenseTokenId, ` **`payerKind (0=human/1=agent)`**`, purchasePath (0=direct/1=x402)` | buyer counts split (E6), recent buyers, receipts |
| `AttestationPosted` | `listingId\|agentNftId, attester, verdict, evalResultHash, schema, postedAt` | verification, reputation feed, `audited` |
| `LicenseToken Transfer` | `from, to, id, value` | ownership, clone-gate (A10) |
| `ReputationPosted` / `ValidationPosted` | `agentNftId, cycle_id, pnl_hash, …` | A9 verification evidence, trade history |

**Subgraph entities** (rename `Strategy`→**`Lineage`** per S4 to match the
terminology lock):
- **`Lineage`** (`id=agentNftId`): owner, manifestCid, parentLineageId, variants[], reputation, validations.
- **`Listing`** (`id=listingId`): lineage, seller, version, contentHash, contentURI, tier, priceUSDC, transferableLicense, publishedAt(createdAt), revoked, sales[], attestations[].
- **`Sale`** (`id=tx-logIndex`): listing, buyer, priceUSDC, sellerProceeds, protocolProceeds, **payerKind**, blockTimestamp.
- **`License`** (`id=licenseTokenId`): listing, owner, mintedAt.
- **`EvalAttestation`** (`id`): listing/lineage, attester, verdict, evalResultHash, schema, postedAt.
- **`Creator`** (derived, `id=address`): mintedLineages[], lifetimeEarned (Σ sellerProceeds), totalBuyers {humans,agents}, clonesSpawned, attestationsIssued. Powers `/marketplace/creator/:addr` (A8: keyed by address; handle/ens are display lookups).

---

## 4. Decisions

### Locked (operator, 2026-05-26)
- **A8 — handle/identity:** **address is canonical**; `handle` + `ens` are
  **optional display fields** in Tier 1a/1b + the `Creator` entity. No handle
  registry/ENS dependency in V2; either can layer on later without a schema
  change. The subgraph keys `Creator` by address; the viewer resolves a handle→
  address via the metadata (and, if present later, ENS).
- **A9 — verification badge (green):** `verified` iff **backtested + ≥30 days
  live-paper data + ≥1 closed cycle with positive PnL hash-committed on-chain**
  (Reputation/ValidationRegistry). The subgraph *derives* `verification.status`
  from these on-chain facts; Tier 1b carries the perf-committed evidence fields
  (`live_paper_days`, `positive_closed_cycle_hash`, etc.) while Tier 0 carries
  `perfHash`. Below the bar → `unverified` (gray). Audit attestation is a
  *separate* `audited` flag, not required for green.

### Recommended (confirm in review)
- **A10 — Tier-B clone semantics:** the on-chain clone edge (`parent_lineage_id`)
  is writable only after the cloner holds `LicenseToken.balanceOf ≥ 1` for a
  variant in the parent lineage (Tier-B); Tier-A clones are free. Enforced at
  mint time (Phase 5 contract check). Frontend gate already derives from
  `getViewer().ownedListingIds`.
- **E6 — payer class:** add **`payerKind`** to the `Sold` event, set by the
  purchase path (x402/agent → `agent`; direct wallet → `human`). The subgraph
  aggregates `Creator.totalBuyers {humans, agents}` and per-listing buyer splits.
  (Bare `LicenseToken` transfers can't encode this — confirmed insufficient.)
- **`publishedAt`:** map from `Listing.createdAt` (no new on-chain field); add
  `publishedAt` to the `ListingRow` seam type so `newest` sort stops using the
  id-proxy. (Register item.)
- **`transferableLicense`:** already in `Listing`; surface in Tier 1b `license`
  + on the `Receipt` (register item).
- **`audited`:** derived boolean — true iff an `AttestationPosted` with an audit
  schema exists. Present in the canonical read projection; the seam field lands
  with the real-data rewire, and the audit attester lands later.
- **A6 — viewer domain:** **placeholder `<viewer-domain>`** throughout; the
  shareable-URL / `external_url` host is provisioned before Phase 4 (operator
  decision, tracked in the register). Schema is domain-independent (agentURI/
  contentURI are `ipfs://`).

---

## 5. Seam ↔ schema mapping (the bridge — proves Phase 6 is a tightening)

Every `MarketplaceData` field (Phase F seam) → its canonical source. Phase 6
swaps `FixtureMarketplaceData` for real impls that satisfy exactly this:

| Seam field | Source |
|---|---|
| `ListingRow.{id,version,lineageId}` | `Listing` + `tokenURI`/Tier 1b |
| `ListingRow.creator{address,handle,ens}` | `Lineage.owner` (addr) + Tier 1a/1b display (A8) |
| `ListingRow.{model,style,assets}` | Tier 1b tags |
| `ListingRow.{return30dPct,sharpe}` | Tier 1b `performance` (perfHash-committed) |
| `ListingRow.buyers{humans,agents}` | subgraph aggregate of `Sale.payerKind` (E6) |
| `ListingRow.{priceUsdc,tier,transferableLicense}` | `Listing` |
| `ListingRow.verification` | derived (A9) |
| `ListingRow.audited` *(new)* | derived from `AttestationPosted` audit schema |
| `ListingRow.acceptsX402` | Tier 1b `x402` |
| `ListingRow.clones` | subgraph count of child `Lineage.parentLineageId` |
| `ListingRow.publishedAt` *(new)* | `Listing.createdAt` |
| `ListingRow.genArtSeed` | `agentNftId + manifestHash` (Phase 4 art) |
| `ListingDetail.{metrics,promise,whatYouGet/Dont,ingredients}` | Tier 1b |
| `ListingDetail.paidToCreatorUsd` | subgraph Σ `Sale.sellerProceeds` |
| `ListingDetail.platformFeeBps` | `Listing.protocolFeeBps` |
| `ListingDetail.variants` | `Lineage.variants[]` |
| `ListingDetail.recentBuyers[]{label,payerKind,outcome}` | `Sale` + Validation receipts |
| `ListingDetail.equityCurve` | Tier 1b `equity_curve_cid` |
| `ListingDetail.onChain.*` | Tier 0 + events (NFT, attestations, anchors, trades) |
| `CreatorProfile.*` | `Creator` derived entity |
| `Receipt.*` | `Sale` + `License` + Tier 1b + Tier 2 (post-decrypt) |
| `TxRef.network` | the deploy chain (mantle-sepolia → mainnet) |
| `Viewer.{ownedListingIds,createdListingIds}` | `License` (owner) + `Lineage` (owner) for the connected wallet (A5, Phase 6) |

---

## 6. Downstream amendments this spec requires (feeds Phase 5)

Apply when the contracts/crate/subgraph get built (Phase 3/5):
1. **Subgraph entity rename** `Strategy → Lineage` (S4 / terminology lock).
2. **`ListingCreated` event** gains `contentURI`, `protocolFeeBps`,
   `transferableLicense`, and `createdAt` so the subgraph can hydrate the
   listing read model without a required contract call.
3. **`Sold` event** gains `payerKind` (E6) and keeps `purchasePath`.
4. **Hash algorithm alignment** — `Listing.contentHash` uses blake3 bytes32 to
   match Tier 1/2 commitments; amend the older `keccak256` contract-surface
   comment and generated bindings before implementation.
5. **Surface `Listing.createdAt`** as `publishedAt` in the subgraph + seam.
6. **`perfHash` commitment** at publish time (A9) — the marketplace `publishListing`
   path hashes the Tier-1b `performance` block and stores/anchors it.
7. **Clone-edge write gate** (A10) — `parent_lineage_id` writable only with a held
   `LicenseToken` for Tier-B parents.
8. **Amend `smart-contract-surface-design.md`** §6.3 (subgraph) + §3.2 (`Sold`)
   to match §3/§4 here. (Tracked; do during Phase 5 prep alongside the §2
   staleness fixes already applied.)

---

## 7. Open / deferred (tracked in the program-strategy register §7.1)
- **A6 viewer domain** — provision before Phase 4; placeholder until then.
- **Gen-art `tokenURI` encoding** — Phase 4 (this spec reserves the slot + seed).
- **A7 sealed-bundle decryption relay host** — Phase 5/6.
- **IPFS pinning** — Pinata behind `IpfsStore` (Phase 5); `iroh` V3.
- Subgraph host (C2), EIP-3009 on USDC.e (B3) — Phase 5 prep.

---

## 8. Exit criteria
- This schema is reviewed + approved by the operator.
- The Phase-1-bound register items (`publishedAt`, payer-class, `transferableLicense`,
  `audited`, A8, A9) are resolved here (✓) and ticked in §7.1 of the program doc.
- Phase 3/5 build to §2–§4; Phase 6 wires the seam (§5) — a tightening, not a
  redesign.
