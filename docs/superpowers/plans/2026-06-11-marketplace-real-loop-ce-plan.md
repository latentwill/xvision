# Marketplace Real Loop — CE Plan (concept + execution)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development to implement Phase 1 task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Make the marketplace loop real end-to-end for users: browse real listings, see what you own, manage what you sell — building on the genart-v3 mint/list/buy rails (PR #910).

**Status context (2026-06-11):** mint → list → buy verified live on Mantle Sepolia through product paths (identity NFT #2, listing #2, license #2). Everything read-side is fixture-backed; the buyer receives no bundle; in-UI purchase has no signer.

**Branch:** `feat/marketplace-reads-real`, stacked on `feat/genart-v3-onchain` (PR #910). Worktree `.worktrees/marketplace-reads`.

---

## Wave map (concept)

| Phase | Scope | Depends on |
|---|---|---|
| **1 — Reads become real** (THIS PLAN) | Chain indexer (RPC enumeration, in-memory snapshot), `GET /api/marketplace/*` routes, `ApiMarketplaceData` frontend source, wallet/ownership page, listing revoke, `xvn identity show`, chain-backed `xvn marketplace list` | PR #910 |
| **2 — In-UI purchase** | `useWallet()` signer exposure, network-switch to Mantle Sepolia, USDC balance + approve+buy path, EIP-3009 `buyWithAuthorization` EIP-712 signing (gasless x402), real receipts | Phase 1 |
| **3 — Bundle delivery + license gating** | Real `content_uri` (Pinata/IPFS pinning at publish), buyer bundle fetch + import into engine, engine license check when running licensed strategies, sealed-tier encryption design | Phase 1; design spike for sealed tier |
| **4 — Seller trust + earnings** | Attestation posting/display in UI (CLI `attest` exists), 20-trade auto-attest loop, earnings view, `updateListing` (re-price/re-point) surface | Phases 1–2 |

Each later phase gets its own spec+plan through the normal pipeline; do not freelance them from this doc.

**Key design facts discovered (ground truth for all phases):**
- `ListingRegistry.totalListings()` + `getListing(id)` exist → **the v1 indexer enumerates by RPC; no event-log decoding or subgraph needed.** (`contracts/src/ListingRegistry.sol:208,225`)
- Listing management on-chain = `revokeListing(listingId)` + `updateListing(listingId, contentHash, contentURI)` (`ListingRegistry.sol:182,193`), seller-only.
- The **real genart seed is fully recoverable from chain**: `seed = {agent_id}:{hex(listing.contentHash)}` where `agent_id` comes from decoding `IdentityRegistry.tokenURI(listing.agentNftId)` metadata JSON (field `agent_id`), and `contentHash` IS the manifest hash (route sets them identical in PR #910).
- Ownership surfaces without ERC721Enumerable: the indexer knows every `agent_nft_id` from listings → wallet page checks `ownerOf(token)` and `LicenseToken.balanceOf(wallet, listing_id)` per indexed id.
- `ListingRow` (frontend `data/types.ts:34`) carries metrics (return30dPct, sharpe, buyers, clones) the chain cannot supply in Phase 1 → real rows render with zeroed metrics + `verification: "unverified"` until Phase 4 attestations. This is accepted and explicit.

---

## Phase 1 — execution plan

**Architecture:** a poll-and-enumerate indexer task inside `xvision-dashboard` (modeled on the existing janitor spawn pattern in `server.rs`) holding an `Arc<RwLock<MarketplaceSnapshot>>` — no DB migration; restart re-polls (cheap at testnet scale, revisit persistence when listings > ~500). Read routes serve the snapshot; wallet route does live `ownerOf`/`balanceOf` RPC per request. Frontend gains `ApiMarketplaceData` implementing the existing `MarketplaceData` interface, selected over fixtures when the API reports a live indexer; everything else falls back to fixtures unchanged.

**Env contract (additive):** indexer activates only when `XVN_RPC_URL` + `XVN_LISTING_REGISTRY` + `XVN_IDENTITY_REGISTRY` are set (re-using PR #910's vars; `XVN_LICENSE_TOKEN` enables license lookups; `XVN_CHAIN_ID` optional check). Without them: indexer dormant, `/api/marketplace/status` reports `{"active": false}`, frontend stays on fixtures. Revoke additionally needs `XVN_PUBLISHER_PK` (same gating style as publish).

**Conventions:** worktree `.worktrees/marketplace-reads`, `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"`, build via `scripts/cargo`, frontend tests `cd frontend/web && npm test`. TDD per task. `bd` issue: xvision-pu3.

### Task 1: snapshot types + chain reader (Rust, pure-ish core)

**Files:** Create `crates/xvision-dashboard/src/marketplace_index.rs`; register `mod` in `lib.rs`/`main` module tree.

Core types (exact):

```rust
#[derive(Debug, Clone, serde::Serialize)]
pub struct IndexedListing {
    pub listing_id: u64,
    pub agent_nft_id: String,      // U256 as decimal string
    pub agent_id: String,           // decoded from tokenURI metadata JSON
    pub seller: String,             // 0x… lowercase
    pub content_hash: String,       // 64-hex (== manifest hash == genart seed part)
    pub content_uri: String,
    pub tier: u8,
    pub price_usdc: f64,            // u96 6dp → f64 for display
    pub transferable_license: bool,
    pub revoked: bool,
    pub gen_art_seed: String,       // "{agent_id}:{content_hash}"
    pub name: String,               // from tokenURI metadata "name"
    pub symmetry: String,           // from tokenURI attributes (display traits)
    pub palette: String,
}

#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct MarketplaceSnapshot {
    pub listings: Vec<IndexedListing>,
    pub last_poll_unix: i64,
    pub last_error: Option<String>,
    pub total_onchain: u64,
}
```

Functions: `async fn poll_once(cfg: &IndexerCfg) -> anyhow::Result<MarketplaceSnapshot>` — `totalListings()`, then per id `getListing(id)` (alloy bindings exist in `xvision_identity::contracts::IListingRegistry`; check the `Listing` struct fields incl. seller + revoked flag — read the sol! macro in `contracts.rs`), then `IIdentityRegistry.tokenURI(agent_nft_id)` → base64-decode → `serde_json` → `agent_id`/`name`/attributes. Decode failures degrade per-listing (`agent_id: ""`, traits empty, listing still included) — never fail the whole poll for one bad token. Plus `pub fn spawn_indexer(state: SharedSnapshot, cfg) -> JoinHandle` polling every 30s (`tokio::time::interval`), modeled on the janitor spawn in `server.rs`.

**Tests (TDD):** unit-test the tokenURI metadata decoder (base64 JSON → agent_id/name/traits; malformed → graceful default) and the seed composition with a fixture tokenURI generated by `xvision_identity::generate_token_uri` (gives true integration with PR #910's format). Chain calls are not unit-tested (covered by the live smoke in Task 8).

Commit: `feat(api): marketplace indexer core — snapshot types + tokenURI decoding`.

### Task 2: wire indexer into server + GET routes

**Files:** Modify `crates/xvision-dashboard/src/server.rs` (AppState gains `marketplace_snapshot: SharedSnapshot`; spawn on startup when env present, log activation/dormancy); Create `crates/xvision-dashboard/src/routes/marketplace_read.rs`; register in `routes/mod.rs` + **readonly router** (these are GETs — match how readonly routes register, ~`server.rs:196+`).

Routes (exact shapes):
- `GET /api/marketplace/status` → `{active: bool, last_poll_unix, total_onchain, last_error}`
- `GET /api/marketplace/listings` → `{items: [IndexedListing], total: usize}` (revoked excluded by default; `?include_revoked=1` includes)
- `GET /api/marketplace/listings/:id` → `IndexedListing` or 404
- `GET /api/marketplace/wallet/:address` → `{address, strategies: [{token_id, agent_id, name, gen_art_seed, listed: bool, listing_id?}], licenses: [{listing_id, agent_id, name, gen_art_seed, balance}], listings: [IndexedListing]}` — strategies = indexed agent tokens where `ownerOf == address`; licenses = `balanceOf(address, listing_id) > 0`; listings = snapshot rows where `seller == address`. Live RPC per request; 503-style error (existing `ServiceUnavailable`) when indexer dormant.

**Tests:** route unit tests against a hand-built snapshot injected into AppState (status/listings/detail/404; wallet route's pure aggregation factored into a testable function with RPC results passed in).

Commit: `feat(api): marketplace read routes over indexer snapshot`.

### Task 3: revoke route (listing management v1)

**Files:** Modify `routes/marketplace.rs` (same env-gating + error conventions as publish): `POST /api/marketplace/listings/:id/revoke` → signer from `XVN_PUBLISHER_PK`, call `ListingRegistry.revokeListing(id)` via the alloy binding (check whether `Erc8004MantleDriver`/`AnchorDriver` exposes revoke; if not, call the contract binding directly in the route like the indexer does — do NOT widen the driver trait in this phase). 201/200 `{listing_id, tx_hash}`; contract reverts (not-seller, unknown id) map to 4xx with the revert string.

**Tests:** unit-test the input validation + env gating (503 without config), mirroring `marketplace.rs` test style.

Commit: `feat(api): POST /api/marketplace/listings/:id/revoke`.

### Task 4: frontend `ApiMarketplaceData` + source selection

**Files:** Create `frontend/web/src/features/marketplace/data/ApiMarketplaceData.ts` + test; Modify the provider wiring (`MarketplaceLayout.tsx` / `provider.tsx` — read them first).

- `ApiMarketplaceData` implements `MarketplaceData` for: `listListings` (map `IndexedListing → ListingRow`: `id = String(listing_id)`, `genArtSeed = gen_art_seed`, `priceUsdc`, `tier` (0→"open",1→"sealed"), `creator.address = seller`, `version "v1"`, metrics zeroed, `verification: "unverified"`, `buyers/clones 0`), `getListing` (detail mapped the same; fields the chain can't fill use honest empty defaults — read `ListingDetail` type first), `submitListing` (reuse `publishListing` from PR #910), `getStats` (derive from snapshot: totalStrategies = items.length, others 0). Everything else (slices, creators, leaderboard, receipts, viewer, listable strategies, purchase/clone intents, subscribePurchases) **delegates to an injected fallback** (`FixtureMarketplaceData`) — explicit constructor arg, one obvious seam.
- Selection: on mount, layout calls `GET /api/marketplace/status`; `active: true` → `new ApiMarketplaceData(new FixtureMarketplaceData())`, else fixtures. No env vars in the bundle.

**Tests:** mapping unit tests (IndexedListing→ListingRow incl. seed passthrough and tier mapping), fallback delegation test, status-selection test (mock fetch).

Commit: `feat(marketplace): ApiMarketplaceData — real listings with fixture fallback`.

### Task 5: wallet page

**Files:** Create `frontend/web/src/features/marketplace/routes/WalletRoute.tsx` + test; register the route + nav entry (read how marketplace routes register — likely `MarketplaceLayout`/router file; follow the existing pattern).

Single full-width column (chat-rail rule: NO side panels, NO modals):
1. **Wallet strip** — connected address via existing `useWallet()` (connect button when disconnected), inline.
2. **Strategies you own** — grid of cards: `GenArtPlaceholder seed={gen_art_seed} size={96}`, name, token_id, listed badge → links to listing.
3. **Licenses you hold** — same card treatment + balance.
4. **Your listings** — rows with price/tier/status and a **Revoke** action using inline two-step confirm (button → "confirm revoke?" inline swap — no popup), calling the Task 3 route, then refetching.

Data: `GET /api/marketplace/wallet/:address`. Empty/dormant states are explicit inline text ("indexer offline — set XVN_* env").

**Tests:** render with mocked fetch — empty state, populated sections, revoke confirm flow firing the POST.

Commit: `feat(marketplace): wallet page — owned strategies, licenses, listing management`.

### Task 6: post-publish lands somewhere real

**Files:** Modify `SellRoute.tsx` (+ its test): add `catch` to `handleMint` showing an inline error strip (no popup; closes the unhandled-rejection gap from xvision-a5p); on success navigate to `/marketplace/listing/{listing_id}` (the detail route backed by Task 4's real data) instead of the fixture receipt.

Commit: `fix(marketplace): publish errors surface inline; success lands on real listing`.

### Task 7: CLI parity

**Files:** Modify `crates/xvision-cli/src/commands/marketplace.rs` (`list` verb: when `MARKETPLACE_DRIVER=onchain`, enumerate via the same contracts bindings — print `listing_id | agent_id | price_usdc | seller | revoked`; fixture path remains the default); Create `xvn identity show <token-id>` (new subcommand under the identity/marketplace command tree — find where identity verbs live or add `marketplace show-token`): fetches `tokenURI`, decodes, prints name/agent_id/traits + optionally `--svg-out <path>`. Update `cli_surface_snapshot` via its regen mechanism; wiki-doc the new verb if the `every_top_level_verb_is_documented_in_wiki` test demands it.

**Tests:** decoder reuse from Task 1 keeps this thin; CLI tests follow existing marketplace test patterns (env-gated usage errors).

Commit: `feat(cli): chain-backed marketplace list + identity token show`.

### Task 8: live verification + docs + PR

1. `scripts/cargo test -p xvision-dashboard -p xvision-cli -p xvision-identity` + full frontend suite — green (pre-existing cli/engine failures tracked in xvision-3k3 excepted).
2. Live (operator env, same vars as the 2026-06-11 run): start dashboard with indexer env → `GET /api/marketplace/listings` shows listings #1/#2 with real seeds; wallet route for `0xb5d2…E553` shows token #2 owned + license #2 held; browse page renders listing #2's real art (visual check vs the minted SVG); revoke a throwaway listing end-to-end.
3. Docs: append Phase 1 outcome note to this plan; update `MANUAL.md`/dashboard wiki if marketplace pages are operator-documented surfaces (check).
4. `bd close xvision-pu3`, push, `gh pr create` (base: `feat/genart-v3-onchain` until #910 merges, then retarget `main`).

---

## Phase 1 self-review notes

- **Spec coverage:** user's three asks map to Task 4 (real listing page data), Task 5 (wallet page incl. licenses + owned strategies), Tasks 3+5 (manage listings = revoke; `updateListing` deliberately deferred to Phase 4 — re-pricing UX needs design).
- **No-popup / chat-rail rules** honored (inline confirms, full-width sections).
- **Known accepted gaps:** metrics zeroed until attestations (Phase 4); in-memory snapshot resets on restart; wallet route trusts the address path param (read-only data, no auth claim made); purchase from UI still Phase 2.
- **Adaptation points are named** (Listing struct fields, driver-vs-direct revoke, route registration files, ListingDetail mapping) — implementers read ground truth, never guess.
