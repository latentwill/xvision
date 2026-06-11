# Marketplace Phase 3 — Bundle Delivery + License Gating (plan)

> Branch `feat/marketplace-p3-delivery` (stacked on P2, PR #914). Subagent-driven;
> TDD; `CARGO_TARGET_DIR=~/.cargo-target/xvision-mreads`. Sealed-tier encryption is
> OUT OF SCOPE (design spike later); open-tier delivery + license gate only.

**Goal:** a buyer actually receives the strategy: publish pins the canonical
manifest to IPFS (`content_uri = ipfs://CID`); a bundle route serves+verifies the
bytes; an import route (license-gated) installs the strategy into the buyer's
local engine as a new ULID; the receipt's "Add to strategies" button works.

**Ground truths (2026-06-11 investigation):**
- `xvision_marketplace::PinataDriver::new(jwt, gateway)`, `put(&[u8]) -> Result<String /*CID*/>`, `get(cid)` round-trips via gateway; `with_api_base` for test mocking; tests exist (`ipfs.rs`).
- Publish route (`routes/marketplace.rs:165-169`) already holds `canonical: String` (the exact bytes to pin) and sets `content_uri: format!("xvn://strategy/{agent_id}")` at :234.
- NO import-from-JSON path exists in the engine strategy API — `create_strategy` takes `CreateStrategyReq` only; clone path mints `Ulid::new()` (engine `api/strategy.rs:927`) and the store writes `~/.xvn/strategies/<ulid>.json` (full `Strategy` serde).
- License check pattern: `ILicenseToken::new(addr, &provider).balanceOf(address, U256::from(listing_id))` (`marketplace_read.rs:305-314`), env `XVN_LICENSE_TOKEN`.
- Frontend: `InstallSteps.tsx` step 4 "Add to strategies" button (currently dead); `Receipt.license.bundleCid` exists in the type; receipt mapping in `ApiMarketplaceData.getReceipt`.

### Task 1 (Rust)
1. **Engine**: `pub async fn import_strategy(ctx: &ApiContext, manifest: serde_json::Value) -> ApiResult<Strategy>` in `crates/xvision-engine/src/api/strategy.rs` — deserialize `Strategy`, assign NEW `Ulid::new()` id (overwrite the manifest's id field — find the id field on Strategy; mark imported provenance in metadata if a notes/origin field exists, else skip), persist via the same store the clone path uses, return the stored Strategy. TDD vs a tempdir store (follow existing strategy api tests).
2. **Publish pins**: in `post_publish`, when env `PINATA_JWT` is set: `PinataDriver::new(jwt, env PINATA_GATEWAY or "")` → `put(canonical.as_bytes())` → `content_uri = format!("ipfs://{cid}")`; pin failure → 502-class error BEFORE mint (no orphan); JWT unset → keep `xvn://strategy/{agent_id}` (log info). Response gains `content_uri`.
3. **Bundle route**: `GET /api/marketplace/listings/:id/bundle` (readonly) → listing from snapshot (404) → resolve: `ipfs://cid` → `PinataDriver` gateway get (driver without JWT is fine for gets — check; else plain reqwest to gateway); `xvn://strategy/{ulid}` → load local strategy + canonical_json (404 if absent) → verify `manifest_hash_hex(bytes) == listing.content_hash` (409 on mismatch with explicit integrity error) → 200 `{listing_id, content_uri, verified: true, manifest: <json>}`.
4. **Import route**: `POST /api/marketplace/listings/:id/import` body `{address}` (mutating router) → validate address (400) → license gate: balanceOf(address, listing_id) > 0 via env XVN_LICENSE_TOKEN + read provider (403 `no license for {address}`; 503 env dormant) → fetch+verify bundle (reuse step 3 logic as a fn) → `import_strategy` → 201 `{agent_id}`. V1 CAVEAT (doc comment): address is asserted, not proven — sig-challenge auth arrives with sealed tier.
TDD per route (validation/403/404/409/503 paths; import_strategy engine test). `scripts/cargo test -p xvision-engine --lib api::strategy` (or the right filter) + `-p xvision-dashboard`. Commit.

### Task 2 (frontend)
- `getReceipt` mapping: `bundleCid` from listing `content_uri` (fetch listing by receipt.listing_id or extend the receipts route — simplest: backend already joins listing; ADD `content_uri` to the receipts route response (tiny Rust edit, include in this task) and map it).
- `InstallSteps` step 4 "Add to strategies": wire to `POST /api/marketplace/listings/:id/import {address: currentAddress()}` with inline pending/success (link "Open in strategies" → `/authoring/{agent_id}`)/error (incl. 403 no-license, wallet-required). Step 2 "Decrypt sealed bundle" → render only for sealed tier; open tier shows "Open bundle" linking to gateway URL when ipfs://.
- TDD: mapping + InstallSteps states (mock fetch/chain). Full `npm test` + tsc green. Commit.

### Task 3: live verify + PR
PINATA_JWT from op (`op item list | grep -i pinata` to find; if absent, operator supplies — degrade: verify xvn:// local path instead and note ipfs pin as operator-pending). Publish new listing → content_uri ipfs:// (or xvn://) → GET bundle (verified:true) → POST import with op wallet address (holds licenses) → 201 new agent_id → confirm `~/.xvn/strategies/<new>.json` exists and GET /api/strategy/:id 200. Suites green; review; PR stacked on #914; close bead xvision-mnb.

## Outcome
(append after Task 3)
Live-verified 2026-06-11: bundle route verified:true on listing 2 (xvn:// local path,
hash-checked); import with licensed wallet → 201 new agent_id 01KTV3DYXKJ8VPFR6JQ5YCWS85
(strategy on disk, GET 200); unlicensed address → 403. Gate ordering spot-checked
(balanceOf before fetch before import; fresh ULID + audit asserted by engine tests).
ipfs:// pin path mock-tested only — no PINATA_JWT in 1Password yet; operator to supply
one to activate real pinning. Review agent stalled mid-run; controller spot-checked
security items directly.
