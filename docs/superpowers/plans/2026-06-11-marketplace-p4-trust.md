# Marketplace Phase 4 — Seller Trust + Earnings (plan)

> Branch `feat/marketplace-p4-trust` (stacked on P3, PR #916). Subagent-driven; TDD;
> `CARGO_TARGET_DIR=~/.cargo-target/xvision-mreads`. The 20-trade auto-attest engine
> loop is OUT OF SCOPE (separate track); manual attest + display + earnings only.

**Ground truths (2026-06-11 investigation):**
- `EvalAttestationRegistry.postAttestation(listingId, evalResultHash, evalResultURI, schema)` — PERMISSIONLESS, attester = msg.sender; getters `getAttestations(listingId) -> Attestation[{evalResultHash, evalResultURI, attester, postedAt, schema}]` + `getAttestationCount` (contract :42-66; binding `IEvalAttestationRegistry` in contracts.rs:151-177). Driver: `attest_eval(AttestRequest{listing_id, eval_result_hash, eval_result_uri, schema}) -> TxHash` (adapter.rs:396). CLI payload convention: keccak256 of `{"cycles":N,"sharpe":F}` JSON, uri `xvn://eval/listing/{id}`, schema ZERO (marketplace.rs:420-448).
- Earnings: NO totalSupply on LicenseToken; source = `Sold` event log scan (listingId indexed; sellerProceeds in data). Env for filter: XVN_MARKETPLACE_CONTRACT.
- `updateListing(listingId, contentHash, contentURI)` seller-only, content-only; **price immutable, no re-price function exists** — operator guidance: revoke + relist.
- Frontend: `OnChainReceipts.attestations: {attester, verdict("endorse"|"question"|"reject"), targetVersion, at}[]`; verification badge positive-only (locked); WalletRoute ListingRowItem (WalletRoute.tsx:232+) has revoke only.
- IndexedListing has no attestation/sold fields.

### Task 1 (Rust)
1. **Indexer enrichment** (`marketplace_index.rs` + `IndexerCfg`): optional `eval_attestation: Option<Address>` (env XVN_EVAL_ATTESTATION) and `marketplace: Option<Address>` (XVN_MARKETPLACE_CONTRACT) in cfg. Per poll: per-listing `attestation_count: u64` via getAttestationCount (degrade 0); one `eth_getLogs` for `Sold` (address=marketplace, topic0=Sold sig, from block env XVN_MARKETPLACE_DEPLOY_BLOCK or 0) → per-listing `units_sold: u64`, `earned_usdc: f64` (sum sellerProceeds/1e6). New IndexedListing fields default 0 (serde stays additive).
2. **Attest write**: `POST /api/marketplace/listings/:id/attest` body `{cycles: u64, sharpe: f64 (finite)}` (mutating router; publish-route conventions: validate → 404 unknown listing via snapshot → env gate 503 → driver.attest_eval with the CLI payload convention) → 201 `{tx_hash}`.
3. **Attest read**: `GET /api/marketplace/listings/:id/attestations` (readonly) → getAttestations via read provider → 200 `{items: [{attester, posted_at_unix, eval_result_uri, eval_result_hash, schema}]}`; dormant 503; 404 unknown listing.
4. **Content update**: `POST /api/marketplace/listings/:id/update` (mutating, no body) → listing from snapshot (404) → load local strategy by listing.agent_id (404 w/ explicit "local strategy not found") → canonical+hash (+pin when PINATA_JWT, reuse publish pin fn) → `updateListing` via the `IListingRegistry` binding w/ signer (NotSeller revert → 400 w/ chain text) → 200 `{listing_id, content_hash, content_uri, tx_hash}`.
TDD per route; full dashboard suite green; rustfmt. Commit: `feat(api): attestations write/read, Sold-derived earnings in indexer, listing content update`

### Task 2 (frontend)
1. `IndexedListing` TS mirror gains attestation_count/units_sold/earned_usdc; `listListings`/`getListing` map `verification: attestation_count > 0 ? "verified" : "unverified"` (badge stays positive-only) and `buyers: {humans: units_sold, agents: 0}` (honest approximation, comment it).
2. Listing detail (LineageRoute): inline "Verified evals" section when attestations exist — fetch GET attestations, render attester (truncated), date, uri; map into `onChain.attestations` with `verdict: "endorse"` (v1: any attestation = endorsement; comment).
3. WalletRoute ListingRowItem: earnings chip (`sold ×N · $X earned`); "Republish content" action (inline two-step confirm like revoke → POST update → refetch); "Post attestation" inline mini-form (cycles + sharpe number inputs, POST attest, pending/error inline). No popups; theme borders.
TDD; full npm test + tsc green. Commit: `feat(marketplace): verified badges from attestations, earnings + attest/update actions`

### Task 3: live verify + PR
Post attestation on listing 2 (cycles 20, sharpe 1.5) → GET attestations shows it → after re-poll listListings shows attestation_count 1 (verified) + units_sold 2 / earned 1.90 for listing 2 → content update on listing 2 → tx + listing reflects new hash. Suites green; review; PR stacked on #916; close bead xvision-8uy; append outcome.

## Outcome
(append after Task 3)
