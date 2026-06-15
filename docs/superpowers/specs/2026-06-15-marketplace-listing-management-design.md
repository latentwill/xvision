# Marketplace listing management — owner price edit, delete, and a "My Listings" page

- **Date:** 2026-06-15
- **Status:** Design (awaiting review)
- **Tracking:** `xvision-2j65.5` (deferred item from the marketplace QA batch, PR #1073)
- **Author:** brainstorming session (operator + Claude)

## 1. Problem & goal

Operator QA: *"Need to allow users to change their listings (edit, delete) — should be
able to change from open to paid and vice versa."*

A marketplace listing is an on-chain record in `ListingRegistry` (a UUPS proxy on
Mantle Sepolia). The goal is to give a listing **owner** a complete management
surface:

1. **Change the price** of a listing in place (the headline ask — including
   "open ↔ paid", modeled as a price change; see §4).
2. **Delete** a listing.
3. **Edit** a listing's published content.
4. See **all** of their listings in one place.

## 2. What already exists (do NOT rebuild)

Ground-truthed in `WalletRoute.tsx` (`/marketplace/wallet`, linked from the browse
header `HeaderStrip.tsx`) — the `ListingRowItem` component already has working,
backend-wired owner controls:

| Action | UI | Endpoint | On-chain |
|---|---|---|---|
| **Delete** | "Revoke" (inline confirm) | `POST /api/marketplace/listings/:id/revoke` | `ListingRegistry.revokeListing` (sets `revoked=true`, blocks future sales) |
| **Edit content** | "Republish content" | `POST /api/marketplace/listings/:id/update` | `ListingRegistry.updateListing` (re-pins + rotates contentHash/URI) |
| Post attestation | "Post attestation" | `POST /api/marketplace/listings/:id/attest` | `EvalAttestationRegistry.postAttestation` |

So **delete and edit-content are done.** This feature adds **price edit** (net-new,
needs a contract change) and improves **discoverability/placement** of all owner
controls.

## 3. The constraint that shaped this design

`ListingRegistry.sol` makes a listing's **price and tier immutable after creation** —
`updateListing` rotates only `contentHash`/`contentURI`; its doc comment is explicit:
*"price, tier, fee snapshot, and the transferable flag are immutable after creation."*
There is no `updatePrice`/`setPrice`/`updateTier`.

Three facts make adding price-edit clean rather than a redeploy nightmare:

- **`ListingRegistry` is a UUPS upgradeable proxy** (`Initializable, OwnableUpgradeable,
  UUPSUpgradeable`; admin can upgrade). Adding a function is an **implementation
  upgrade** — the **proxy address is unchanged and all existing listings are
  preserved.** Precedent: `contracts/script/UpgradeMarketplaceSetUsdc.s.sol`.
- **Rust bindings are hand-written `alloy::sol!` interfaces** (`crates/xvision-identity/src/contracts.rs`)
  — adding the function is a one-line interface edit, no artifact regeneration.
- **The indexer re-reads `getListing(id)` every poll** (`marketplace_index.rs:587`),
  so an updated price reflects on the next poll with **no indexer change**.

## 4. Key decisions

1. **In-place price edit via a contract upgrade** (operator chose this over a
   "revoke + re-list" flow). Preserves the listing id, sales history, and NFT.

2. **Free vs paid is driven by PRICE, not tier.** Today the frontend hardcodes
   `open = free` / `sealed = paid` (`isFree = priceUsdc === null || tier === "open"`).
   We decouple: **`price === 0` ⇒ free (clone/run-free path); `price > 0` ⇒ paid (buy
   path)**, independent of the open(plaintext)/sealed(encrypted) tier. "open ↔ paid"
   then becomes a pure price edit — **no content re-encryption.**

3. **Owner controls live on the listing detail page** (`LineageRoute`) when the viewer
   owns the listing, **and** on a new dedicated **"My Listings"** page. A shared
   component renders the owner actions in both places.

4. **Auth/custody model is inherited, not changed.** The new price endpoint mirrors the
   existing `revoke`/`update` endpoints exactly: it signs with the server's configured
   signer and relies on the contract's `NotSeller` revert for authorization; the HTTP
   route is gated by the existing `require_auth_middleware`. Per-user non-custodial
   wallet signing is **out of scope** (it isn't how revoke/update work today either).

## 5. Non-goals

- True **open ↔ sealed encryption toggle** (re-encrypt + re-pin content). Deferred;
  this pass treats free/paid as price only.
- Changing the **custody/signing model** (who signs the on-chain tx).
- Editing a listing's **name, gen-art, or assets** (separate from price/content).
- **Un-revoke** (the contract has no un-revoke; delete stays one-way).

## 5a. Pricing model validation (industry research, 2026-06-15)

Checked our model against how production NFT marketplaces handle listing/repricing.
There are two dominant architectures:

- **Off-chain signed orders** (Seaport/OpenSea, Blur, Reservoir): a listing is a *signed
  message* stored off-chain — listing and repricing are **gasless**, settlement is
  on-chain only when a buyer fulfills. This is why big marketplaces show millions of
  listings without per-seller gas. **Its core hazard is the stale-order problem:** old
  signed listings remain fulfillable on-chain even after they're hidden in the UI —
  this is the exact class of bug behind OpenSea's ~$1.8M refund (buyers bought at old,
  far-below-market prices from listings sellers thought were gone). Sellers must pay gas
  to truly cancel.
- **On-chain listing registry** (what xvision uses; also what "build an NFT
  marketplace" contract guides teach): a listing is on-chain storage; `list` /
  `update` / `cancel` are on-chain ops. "Update listing" (incl. price) is a **standard,
  expected operation** in this model.

**Verdict: our `updatePrice` design is the correct choice for xvision's architecture,
and it side-steps the off-chain model's biggest footgun.**

- `updatePrice` mutating the single on-chain record means price has **one source of
  truth read at buy time** — there are **no stale orders** to exploit (the OpenSea
  failure mode is structurally impossible here).
- xvision signs the tx with the **server publisher key**, so the **user pays no gas** to
  reprice — we get the "gasless to the user" benefit that off-chain orders are prized
  for, without the stale-order risk (and Mantle gas is negligible regardless).
- **Buyer-overpay protection is already best-practice:** buys are EIP-3009
  `TransferWithAuthorization` where the buyer signs an exact `value` (their max spend).
  A price *increase* between view and purchase makes a stale authorization fail rather
  than overpay — matching the industry rule that "the value paid is never more than the
  buyer agreed to." **Implementation must confirm** `Marketplace.buy*` charges the
  *current* price and rejects an authorization whose `value` is below it (verify in
  `Marketplace.sol`); this is the slippage guard.
- **"free = price 0" is intentional and xvision-specific.** In NFT-land "free" is
  usually a primary-mint concept and open editions are *fixed-price*; xvision's free
  path is "clone / run-free" (copy the strategy), which is a legitimate distinct intent.
  Routing `price 0 → cloneIntent`, `price > 0 → purchaseIntent` is internally
  consistent.

**Future (non-goal):** if xvision ever wants OpenSea-scale gasless listing/repricing,
that's a bigger pivot to a Seaport-style off-chain signed-order model — out of scope for
this feature.

Sources: OpenSea Help (lower/cancel listing & gas), OpenSea Seaport (off-chain orders /
gas savings), Reservoir off-chain cancellation, paintswap marketplace mechanics
(buyer-overpay protection), reporting on the OpenSea stale-listing exploit.

## 6. Architecture, by layer

### 6.1 Contract — `contracts/src/ListingRegistry.sol` (+ interface)

Add a seller-only price mutator and an event. Mirror the access pattern of
`revokeListing` (`if (l.seller != msg.sender) revert NotSeller(...)`).

```solidity
// IListingRegistry.sol
event ListingPriceUpdated(uint256 indexed listingId, uint96 oldPriceUSDC, uint96 newPriceUSDC);
function updatePrice(uint256 listingId, uint96 newPriceUSDC) external;

// ListingRegistry.sol
function updatePrice(uint256 listingId, uint96 newPriceUSDC) external override {
    Listing storage l = _listings[listingId];
    if (l.seller == address(0)) revert UnknownListing(listingId);
    if (l.seller != msg.sender) revert NotSeller(listingId, msg.sender);
    if (l.revoked) revert ListingRevoked(listingId);   // can't reprice a dead listing
    uint96 old = l.priceUSDC;
    l.priceUSDC = newPriceUSDC;                          // 0 ⇒ free, >0 ⇒ paid
    emit ListingPriceUpdated(listingId, old, newPriceUSDC);
}
```

- UUPS storage layout: only *appends behavior*, no new state variables, so the layout
  is unchanged and upgrade-safe.
- **Foundry test** (`contracts/test/`): seller can reprice; non-seller reverts
  `NotSeller`; repricing a revoked listing reverts; event emitted; a subsequent
  `buy`/`buyWithAuthorization` charges the new price.
- **Upgrade script** `contracts/script/UpgradeListingRegistry.s.sol` modeled on
  `UpgradeMarketplaceSetUsdc.s.sol`: deploy new impl, call `upgradeToAndCall` on the
  existing proxy. Proxy address unchanged.

### 6.2 Rust bindings — `crates/xvision-identity/src/contracts.rs`

Add to the `sol! { interface IListingRegistry { ... } }` block:
```rust
function updatePrice(uint256 listingId, uint96 newPriceUSDC) external;
event ListingPriceUpdated(uint256 indexed listingId, uint96 oldPriceUSDC, uint96 newPriceUSDC);
```

### 6.3 Driver — `crates/xvision-marketplace/src/adapter.rs`

Add to the `AnchorDriver` trait + `Erc8004MantleDriver` impl, mirroring
`revoke_listing` (build wallet provider → call binding → `.send().get_receipt()`,
wrapped in `with_chain_timeout`):
```rust
async fn update_price(&self, listing_id: U256, new_price_usdc6: U96) -> Result<TxHash, MarketplaceError>;
```

### 6.4 Backend endpoint — `crates/xvision-dashboard/src/routes/marketplace.rs`

New handler `post_set_price`, registered in `server.rs` under the existing
auth-gated marketplace routes (same group as `revoke`/`update`):
```
POST /api/marketplace/listings/:id/price
body:  { "price_usdc": <f64> }          // whole USDC; converted via usdc6()
200:   { "listing_id": <u64>, "price_usdc": <f64>, "tx_hash": "0x…" }
```
- Validate `price_usdc >= 0` and finite (400 otherwise).
- `usdc6()` conversion (reuse the publish helper).
- Call `driver.update_price(id, price6)`.
- Error mapping identical to `revoke`/`update`: contract revert (`NotSeller`,
  `UnknownListing`, `ListingRevoked`) → 400; chain unconfigured → 503.

### 6.5 Indexer — no change

`marketplace_index.rs` re-reads `getListing(id)` each poll, so the updated
`price_usdc` lands on the next snapshot automatically. (The new event exists for
external consumers / audit, but the indexer doesn't need it.)

### 6.6 Frontend — data seam

`MarketplaceData` (`data/MarketplaceData.ts`) gains:
```ts
setListingPrice(listingId: Id, priceUsdc: number): Promise<TxRef>;
```
- `ApiMarketplaceData`: POST to `/api/marketplace/listings/:id/price` (pattern of the
  existing wallet-route mutations using `apiFetch`).
- `FixtureMarketplaceData`: returns a fake `TxRef` (so the demo/dev path works).
- `SubgraphMarketplaceData`: delegate to the fallback.

### 6.7 Frontend — free/paid model (decouple from tier)

Replace the `isFree = priceUsdc === null || tier === "open"` rule **everywhere it
appears** with a single shared helper:
```ts
// data/pricing.ts
export const isFreeListing = (l: { priceUsdc: number | null }) =>
  l.priceUsdc === null || l.priceUsdc === 0;
```
Audit + update call sites: `LineageRoute` (run-free vs buy CTA), `ListingCard`,
`ListingPreviewCard`, `browse/BrowseRoute`. A paid listing routes to `purchaseIntent`;
a free one to `cloneIntent` — driven by price, not tier.

### 6.8 Frontend — owner controls

- **Shared `OwnerListingCard`** (extracted from `WalletRoute`'s `ListingRowItem`):
  renders status + actions (Edit price, Republish, Delete) with inline-confirm flows,
  **no popups**. Used by both the My Listings page and the detail-page owner strip.
- **Listing detail page** (`LineageRoute`): when `viewer.createdListingIds` includes
  `detail.id`, render a **full-width inline owner strip** above the fold (honors the
  "no right-side boxes / no popups" rules) with the owner actions. Includes an
  **Edit-price** inline control (number input → `setListingPrice`).
- **My Listings page** `/marketplace/mine`: a dedicated, canonical route listing every
  viewer listing (active + revoked) using `OwnerListingCard`. Linked from the
  marketplace header and the user menu. The existing `/marketplace/wallet` page keeps
  its other sections (owned strategies, license balances) but its "Your listings"
  section is refactored to render `OwnerListingCard` (the same component as
  `/marketplace/mine`) with a "Manage all →" link to `/marketplace/mine`. No redirect;
  both surfaces share the one component so they can't drift.

### 6.9 Edit-price UX

Inline (no modal): a small number input (USDC) + Save. `0` = free. On submit →
`setListingPrice` → optimistic refetch of the wallet/listing query. Errors render
inline (e.g. `NotSeller` → "Only the listing owner can change the price").

## 7. Data flow — price edit (end to end)

```
Owner edits price on LineageRoute / My Listings
  → MarketplaceData.setListingPrice(id, usdc)
  → POST /api/marketplace/listings/:id/price { price_usdc }
  → driver.update_price(id, usdc6)
  → ListingRegistry.updatePrice(id, newPrice)   [reverts NotSeller if not seller]
  → ListingPriceUpdated event
  → next indexer poll re-reads getListing → snapshot price_usdc updates
  → UI refetch shows the new price; free/paid recomputed from price
```

## 8. Error handling

| Case | Surfaced as |
|---|---|
| Not the seller | contract `NotSeller` → 400 → inline "Only the owner can change the price" |
| Listing revoked | contract `ListingRevoked` → 400 → inline "This listing is deleted" |
| Negative/non-finite price | 400 (server) / disabled Save (client) |
| Chain unconfigured | 503 → inline "Marketplace chain not available" |

## 9. Testing

- **Foundry** (`contracts/test/ListingRegistry.t.sol`): the cases in §6.1.
- **Rust**: driver unit test (mock/anvil if the harness exists) + backend handler
  validation tests (price parsing, error mapping), mirroring the `revoke` tests.
- **Frontend (vitest)**: `isFreeListing` helper unit tests; `setListingPrice` in
  `ApiMarketplaceData` (mocked `apiFetch`); `OwnerListingCard` renders actions + the
  edit-price control; `LineageRoute` shows the owner strip only when owned;
  My Listings page lists listings. Update existing tests touched by the free/paid
  decoupling.

## 10. Rollout / deploy

1. Land contract + Rust + frontend behind a normal PR (no behavior change until the
   proxy is upgraded).
2. Run the Foundry tests; **upgrade the Mantle Sepolia `ListingRegistry` proxy** via
   `UpgradeListingRegistry.s.sol` (proxy address unchanged → no env/address churn in
   the dashboard/indexer/frontend).
3. Verify: reprice a test listing end-to-end; confirm the indexer snapshot + UI
   reflect the new price and that buy charges it.

This work ships on a **separate branch** (`feat/marketplace-listing-management`) and PR
— independent of the QA batch PR #1073.

## 11. Risks / open questions

- **Custody model**: the on-chain `seller` is whatever the publish path set (the
  existing endpoints sign with the server signer). The price endpoint inherits this; if
  publishing later moves to per-user wallet signing, the price endpoint follows the same
  pattern with no structural change. Flagged, not solved here.
- **Free/paid decoupling blast radius**: `tier` ("open"/"sealed") still exists for the
  plaintext-vs-encrypted distinction; only the *free/paid* meaning moves to price. The
  audit in §6.7 must catch every `tier === "open"`-means-free assumption.
- **`uint96` price bound**: max ~7.9e28 in 6-decimal USDC — far beyond any real price;
  client caps input sanely.
