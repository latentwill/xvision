# Smart Contract Surface — Design

> **Status:** Draft for user review · 2026-05-08
> **Depends on:** [`decisions/0008-erc8004-deployment.md`](../../../decisions/0008-erc8004-deployment.md), [`docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`](./2026-05-08-strategy-creation-engine-design.md), and `architecture.md` §6.1.
> **Related:** [`docs/erc-8004-agent-uses.md`](../../erc-8004-agent-uses.md).

---

## 1. Scope

This spec covers the on-chain contract surface for Xianvec: the marketplace, commerce, licensing, and discoverability layer that sits on top of the three ERC-8004 registries already designed in ADR 0008.

**In scope (v1, single canonical chain — Mantle, chain 5000):**

- Listing CRUD on-chain.
- Sale flow with platform commission split.
- License token issuance (ERC-1155, soulbound default).
- Eval-attestation registry.
- Platform self-registration as an ERC-8004 agent.
- x402 checkout for agent-led full-license purchases.
- Upgradeability via UUPS proxies behind a 7-day timelock + 2-of-3 multisig.
- CREATE2 deterministic deploys to enable identical addresses on future mirror chains.

**Explicitly out of scope (recorded as future paths in §10):**

- Multi-chain mirroring or cross-chain message passing.
- Pay-per-fire (x402 micropayments for skill or strategy invocations).
- Subscription / streaming licenses.
- Curator / referrer / royalty splits beyond a single seller-vs-platform split.
- Decentralized Tier B content protection (TEEs, threshold encryption).
- Other sale assets beyond USDC.e on Mantle.
- Refund flows on-chain.
- Resale-market mechanics for transferable licenses.

---

## 2. Architecture overview

```
                                 Mantle (canonical, chain 5000)
┌─────────────────────────────────────────────────────────────────────┐
│  Existing (ADR 0008 + Strategy Engine spec §13)                     │
│   IdentityRegistry  ──► (ERC-721 strategy NFTs, +platform agent #0) │
│   ReputationRegistry  (per-run feedback)                            │
│   ValidationRegistry  (per-trade proofs)                            │
└─────────────────────────────────────────────────────────────────────┘
          ▲                                                ▲
          │ reads agentNftId                               │ reads agentNftId
          │                                                │
┌─────────┴───────────────┐               ┌────────────────┴──────────┐
│  ListingRegistry        │               │  EvalAttestationRegistry  │
│  · createListing        │               │  · postAttestation        │
│  · updateListing        │               │  (publish-time + 3rd-party│
│  · revokeListing        │               │   eval pointers, EAS-shape)│
│  · view listings        │               └───────────────────────────┘
└──────────┬──────────────┘                              ▲
           │ readListing                                 │ optional reads at sale time
           ▼                                             │
┌──────────────────────────┐         ┌──────────────────────────────┐
│  Marketplace             │ mints   │  LicenseToken (ERC-1155)     │
│  · buy(listingId)        │────────►│  · authorizedMint(to,id,qty) │
│  · USDC pull + split     │         │  · per-listing tokenId       │
│  · emit Sold(...)        │         │  · soulbound by default,     │
│  · x402 settlement entry │         │     transferable opt-in/listing│
└──────────┬───────────────┘         └──────────────────────────────┘
           │ split
           ├──► seller wallet (95%)
           └──► protocol fee recipient (5%)
```

**Key principles:**

- **One concern per contract.** Listing data is not on the sale contract; sale is not on the token contract. Each is small, replaceable, and audit-scoped.
- **USDC.e on Mantle is the only sale currency in v1.** Native MNT, ETH, and other stables are deferred. One asset → one math path → one slippage story.
- **Soulbound license tokens by default.** A license is bound to the buyer's wallet. Sellers can opt a listing into "transferable license" at listing time. Soulbound is the safe default for IP — it stops trivial license farming.
- **Upgradeability via UUPS proxy + 7-day timelock + 2-of-3 multisig** on each new contract. The three ERC-8004 registries (per ADR 0008) stay non-proxied as already specced.
- **Deterministic CREATE2 deploys** via a single `XvnDeployer` factory. Same bytecode, same salts → same address on any EVM chain we mirror to later. No multi-chain code in v1.
- **Platform discoverability:** xvn-the-platform is registered as an ERC-8004 agent in `IdentityRegistry`, with `Marketplace` as its endpoint. Existing 8004 indexers (0xbits, dmihal, AgentCity) light up xvn for free.
- **Read path matters as much as write path.** Every contract emits events with stable, indexer-friendly schemas (named topic-zero, indexed seller / buyer / listingId / agentNftId). Subgraph-readable on day 1.

---

## 3. Contract surfaces

### 3.1 `ListingRegistry`

```solidity
struct Listing {
    uint256 listingId;             // monotonically increasing
    address seller;
    uint256 agentNftId;            // ERC-8004 IdentityRegistry token (the strategy)
    bytes32 contentHash;           // keccak256 of canonical bundle JSON
    string  contentURI;            // ipfs://… (Tier A) or https://api.xvn… (Tier B)
    uint8   tier;                  // 0 = Open, 1 = Sealed
    uint96  priceUSDC;             // 6-decimal USDC, 2^96 enough headroom
    uint16  protocolFeeBps;        // snapshot at create time (resists rug)
    bool    transferableLicense;   // soulbound default = false
    uint64  createdAt;
    bool    revoked;
}

function createListing(
    uint256 agentNftId,
    bytes32 contentHash,
    string  calldata contentURI,
    uint8   tier,
    uint96  priceUSDC,
    bool    transferableLicense
) external returns (uint256 listingId);

function updateListing(uint256 listingId, bytes32 contentHash, string calldata contentURI) external; // seller-only, content rotation only
function revokeListing(uint256 listingId) external;                                                  // seller-only, blocks new sales
function getListing(uint256 listingId) external view returns (Listing memory);

event ListingCreated(uint256 indexed listingId, address indexed seller, uint256 indexed agentNftId, bytes32 contentHash, uint8 tier, uint96 priceUSDC);
event ListingUpdated(uint256 indexed listingId, bytes32 contentHash, string contentURI);
event ListingRevoked(uint256 indexed listingId, address indexed seller);
```

`createListing` requires `IdentityRegistry.ownerOf(agentNftId) == msg.sender`. `protocolFeeBps` is snapshotted from `Marketplace`'s current admin-set value at create-time so a future fee bump cannot retroactively rug existing listings.

### 3.2 `Marketplace`

```solidity
function buy(uint256 listingId, address recipient) external returns (uint256 licenseTokenId);

// USDC pulled from msg.sender via approve+transferFrom; split:
//   sellerProceeds   = price * (10000 - protocolFeeBps) / 10000
//   protocolProceeds = price - sellerProceeds
// Mints LicenseToken(listingId, qty=1) to recipient.

function buyWithAuthorization(
    uint256 listingId,
    address recipient,
    TransferAuthorization calldata auth   // EIP-3009: from, to, value, validAfter, validBefore, nonce, v, r, s
) external returns (uint256 licenseTokenId);

function setProtocolFeeBps(uint16 newBps) external onlyAdmin; // capped at MAX_PROTOCOL_FEE_BPS
function setFeeRecipient(address newRecipient) external onlyAdmin;
function pause() external onlyAdmin;     // multisig-direct (no timelock)
function unpause() external onlyAdmin;   // timelocked

event Sold(
    uint256 indexed listingId,
    uint256 indexed agentNftId,
    address indexed buyer,
    uint96  priceUSDC,
    uint96  sellerProceeds,
    uint96  protocolProceeds,
    uint256 licenseTokenId
);
event ProtocolFeeBpsChanged(uint16 oldBps, uint16 newBps);
event FeeRecipientChanged(address oldRecipient, address newRecipient);
```

`recipient` lets an x402 facilitator pay on behalf of the agent's wallet — the facilitator is `msg.sender` (pulls USDC), the agent's wallet is `recipient` (receives the license). One on-chain transaction, two-party logical settlement.

`buy` and `buyWithAuthorization` are reentrancy-guarded (`nonReentrant`). USDC transfer happens before `LicenseToken` mint; revert reverts both. Constants:

```solidity
uint16 constant MAX_PROTOCOL_FEE_BPS = 1000;  // 10%
```

Raising the ceiling requires a contract upgrade — explicit "we changed the deal" signal.

### 3.3 `LicenseToken` (ERC-1155)

```solidity
// tokenId == listingId; supply per id is uncapped (one mint per buy)
function authorizedMint(address to, uint256 listingId, uint256 amount) external onlyAuthorized;
function isAuthorized(address) external view returns (bool);
function setAuthorized(address, bool) external onlyAdmin;

// Per-listing transferable flag mirrored from ListingRegistry at mint time;
// soulbound enforcement happens in _beforeTokenTransfer.
function transferableForId(uint256 listingId) external view returns (bool);

event AuthorizedSet(address indexed caller, bool allowed);
```

Authorized minters in v1: `Marketplace` only. Future authorized minters: subscription contract, skills marketplace, x402 pay-per-fire settlement contract — all addable without redeploying `LicenseToken`. This is the lever that pays off the per-concern split.

### 3.4 `EvalAttestationRegistry`

```solidity
struct Attestation {
    bytes32 evalResultHash;        // keccak256 of full eval JSON
    string  evalResultURI;         // ipfs://…
    address attester;
    uint64  postedAt;
    bytes32 schema;                // EAS-style schema id, future-compatible
}

function postAttestation(uint256 listingId, bytes32 evalResultHash, string calldata evalResultURI, bytes32 schema) external;
function getAttestations(uint256 listingId) external view returns (Attestation[] memory);
function getAttestationCount(uint256 listingId) external view returns (uint256);

event AttestationPosted(uint256 indexed listingId, address indexed attester, bytes32 evalResultHash, bytes32 schema);
```

Two write paths use the same function:

- **Publish-time attestation** by the seller: signed eval result for the canonical scenario (per Strategy Engine spec §13).
- **Third-party attestation** by independent validators: someone re-runs the eval, signs the result, posts it. Cheap on-chain anti-fraud surface.

Open question (§11): use [EAS](https://attest.org) directly on Mantle (if EAS is deployed there) instead of a bespoke contract. Default v1 is bespoke — same surface, less external dependency for the hackathon.

### 3.5 `PlatformAgent` registration (no new contract)

A one-shot operator script registers xvn itself as agent #0 in `IdentityRegistry`:

```
xvn admin register-platform-agent
  → IdentityRegistry.register("ipfs://<xvn-platform-manifest-cid>")
  → emits AgentRegistered(tokenId=0, agentURI=…)
```

Platform manifest JSON (pinned to IPFS, CID stored in agent NFT metadata):

```json
{
  "schema": "https://xianvec.dev/schemas/platform-agent.v1.json",
  "name": "Xianvec",
  "description": "Marketplace and identity layer for AI trading agents.",
  "endpoints": {
    "marketplace_contract":          "0x…Marketplace",
    "listing_registry_contract":     "0x…ListingRegistry",
    "license_token_contract":        "0x…LicenseToken",
    "eval_attestation_contract":     "0x…EvalAttestationRegistry",
    "x402_buy_endpoint":             "https://api.xvn.dev/x402/listings/{listingId}/buy",
    "listings_browse":               "https://api.xvn.dev/listings",
    "marketplace_dapp":              "https://app.xvn.dev"
  },
  "supported_protocols": ["erc-8004", "erc-1155", "x402", "eip-3009"],
  "owner_multisig":                  "0x…2of3multisig",
  "discovery_canonical_chain":       { "chainId": 5000, "name": "Mantle" }
}
```

After this tx, any 8004-aware indexer treats xvn as a discoverable agent. Cost: one ERC-721 mint, ~80k gas.

---

## 4. Sale flow + x402 wire shape

### 4.1 Two settlement paths into the same `Marketplace` contract

1. **Direct path:** buyer holds USDC, calls `USDC.approve(Marketplace, price)` then `Marketplace.buy(listingId, buyer)`. Two txs. Used when a human runs a wallet UI.
2. **x402 path:** agent never holds an approval. Buyer signs an off-chain EIP-3009 `TransferWithAuthorization` for USDC; the facilitator submits one tx calling `Marketplace.buyWithAuthorization(listingId, recipient, auth)` which atomically settles USDC + mints the license. One tx, no lingering approval state.

### 4.2 End-to-end x402 sequence

```
┌─────────┐         ┌──────────────┐       ┌──────────────┐      ┌──────────────────┐
│ Agent   │         │ xvn API      │       │ Facilitator  │      │ Mantle (chain)   │
│ (Claude)│         │ (resource    │       │ (e.g.        │      │  Marketplace +   │
│         │         │  server)     │       │  Coinbase)   │      │  LicenseToken    │
└────┬────┘         └──────┬───────┘       └──────┬───────┘      └────────┬─────────┘
     │                     │                      │                       │
 (1) │ GET /listings/42/   │                      │                       │
     │ buy                 │                      │                       │
     │────────────────────▶│                      │                       │
     │                     │                      │                       │
 (2) │ 402 Payment Required│                      │                       │
     │ { x402PaymentRequirements: {                                       │
     │     scheme: "exact",                                               │
     │     network: "mantle-5000",                                        │
     │     asset: "0x09Bc4E0D…(USDC)",                                    │
     │     payTo: "0x…Marketplace",                                       │
     │     maxAmountRequired: "15000000",  // 15 USDC                     │
     │     resource: "/listings/42/bundle",                               │
     │     extra: { listingId: 42, recipient: agent_wallet, contractCall:│
     │              "buyWithAuthorization(42, agent_wallet, auth)" }      │
     │ } }                 │                      │                       │
     │◀────────────────────│                      │                       │
     │                     │                      │                       │
 (3) │ Sign EIP-3009 auth (off-chain)             │                       │
     │ Submit auth + listingId to facilitator     │                       │
     │───────────────────────────────────────────▶│                       │
     │                                            │                       │
 (4) │                                            │ buyWithAuthorization(42, agent_wallet, auth)
     │                                            │──────────────────────▶│
     │                                            │                       │
     │                                            │                  (settle:
     │                                            │                   USDC.transferWithAuth → Marketplace
     │                                            │                   95% → seller, 5% → fee recipient
     │                                            │                   LicenseToken.mint(agent_wallet, 42, 1)
     │                                            │                   emit Sold(...))
     │                                            │                       │
     │                                            │ tx hash + receipt     │
     │                                            │◀──────────────────────│
     │                                            │                       │
     │ tx hash                                    │                       │
     │◀───────────────────────────────────────────│                       │
     │                     │                      │                       │
 (5) │ GET /listings/42/   │                      │                       │
     │ bundle              │                      │                       │
     │ X-PAYMENT: <tx>     │                      │                       │
     │────────────────────▶│ verify license:                              │
     │                     │ LicenseToken.balanceOf(agent_wallet, 42) >=1 │
     │                     │─────────────────────────────────────────────▶│
     │                     │ 200 OK                                       │
     │                     │◀─────────────────────────────────────────────│
     │ 200 bundle ⬇        │                                              │
     │◀────────────────────│                                              │
```

### 4.3 What goes in the `X-PAYMENT` and `402` headers

xvn implements the **x402 PaymentRequirements / PaymentPayload** schema verbatim — no bespoke variants. `scheme: "exact"`, `network: "mantle-5000"`, the `extra` object carries `listingId`, `recipient`, and the encoded `Marketplace.buyWithAuthorization` calldata so generic facilitators can settle without xvn-specific knowledge. Agents using a stock x402 client work with xvn out of the box.

### 4.4 Verification model on the resource server

The xvn API verifies licenses with a **single chain read**: `LicenseToken.balanceOf(agentWallet, listingId) >= 1`. No off-chain state, no replay-attack window. License tokens are soulbound by default, so the wallet that holds the token IS the licensee. For Tier B (sealed), the API additionally checks device fingerprint + signature freshness — same as Strategy Engine spec §5.

### 4.5 Refunds, failures, and partial states

- **402 → buyer never pays:** server has no obligation, no on-chain state changes. Buyer retries.
- **Auth signed but tx fails (e.g. insufficient balance):** EIP-3009 nonces are single-use; buyer signs a new one. No on-chain state changes from the failed tx.
- **Tx succeeds but server can't deliver bundle (Tier B server outage):** license token is already minted — buyer is licensed. Server should retry serving the bundle on next request. Refund flow is **not in v1** — listing seller can issue a refund manually off-chain (USDC → buyer wallet), but no contract path. Documented gap.
- **Listing revoked between 402 issue and tx settlement:** `Marketplace.buyWithAuthorization` checks `Listing.revoked == false` at settlement time; reverts cleanly. EIP-3009 nonce is consumed but USDC isn't moved.

---

## 5. Commission, royalty, and fee model

### 5.1 v1 — single seller-side split, fixed bps

```
buyer pays priceUSDC
  ─► seller receives  priceUSDC * (10000 - protocolFeeBps) / 10000
  ─► fee recipient   priceUSDC * protocolFeeBps / 10000
```

- **Default `protocolFeeBps = 500` (5%).** Tunable post-deploy via timelocked admin.
- **Hard ceiling `MAX_PROTOCOL_FEE_BPS = 1000` (10%) in contract code.** Even the admin multisig cannot push fees above this — it requires a contract upgrade.
- **Snapshotted per-listing at `createListing` time.** A fee bump applies only to *new* listings, never retroactively. Sellers know the deal at the moment they list.
- **One fee recipient address.** Settable by admin. v1 points it at the xvn-treasury multisig.
- **USDC.e on Mantle only.** Other-asset support requires a schema migration, on purpose — `priceUSDC` is typed `uint96`.

### 5.2 Failure and edge-case math

- `priceUSDC = 0` is permitted (free listings). Skips the USDC transfer entirely; `protocolProceeds = 0`. License token still mints. This is the path L1 wizard users take for free templates.
- `protocolFeeBps = 0` listings are also permitted (admin allowlist tx snapshots `0` at create-time). Not a v1 launch feature; storage layout supports it.
- All math uses 6-decimal USDC integer arithmetic. `priceUSDC * bps / 10000` rounds-down on integer division; rounding dust accrues to the seller, not the protocol — by convention. Documented in NatSpec.

---

## 6. Discoverability + GEO

### 6.1 Event design

Stable, indexer-friendly events across all six contracts:

| Contract | Event | Indexed fields |
|---|---|---|
| `IdentityRegistry` | `AgentRegistered(uint256 tokenId, address owner, string agentURI)` | `tokenId`, `owner` |
| `ReputationRegistry` | `FeedbackPosted(uint256 agentId, address rater, ...)` | `agentId`, `rater` |
| `ValidationRegistry` | `ValidationPosted(uint256 agentId, bytes32 resultHash, ...)` | `agentId` |
| `ListingRegistry` | `ListingCreated(uint256 listingId, address seller, uint256 agentNftId, bytes32 contentHash, uint8 tier, uint96 priceUSDC)` | `listingId`, `seller`, `agentNftId` |
| `ListingRegistry` | `ListingUpdated(uint256 listingId, bytes32 contentHash, string contentURI)` | `listingId` |
| `ListingRegistry` | `ListingRevoked(uint256 listingId, address seller)` | `listingId`, `seller` |
| `Marketplace` | `Sold(uint256 listingId, uint256 agentNftId, address buyer, uint96 price, uint96 sellerProceeds, uint96 protocolProceeds, uint256 licenseTokenId)` | `listingId`, `agentNftId`, `buyer` |
| `LicenseToken` | `TransferSingle / TransferBatch` (ERC-1155 standard) | `from`, `to`, `id` (standard) |
| `EvalAttestationRegistry` | `AttestationPosted(uint256 listingId, address attester, bytes32 evalResultHash, bytes32 schema)` | `listingId`, `attester`, `schema` |

These topics are part of the public ABI, locked at v1 launch, and survive proxy upgrades.

### 6.2 Platform agent #0 — xvn discovers itself via the same primitive everyone else uses

Already specified in §3.5. The mint is the GEO play: AI search engines and crawlers asking "what marketplaces exist for trading agents?" land on xvn through the existing 8004 directory. No SEO investment required — the ERC-8004 ecosystem is the index, and xvn is in it as a first-class participant.

### 6.3 Subgraph / indexer schema sketch

Shipped in `crates/xianvec-marketplace/subgraph/`:

```graphql
type Strategy @entity {
  id: ID!                           # agentNftId
  owner: Bytes!
  manifestCid: String!
  reputation: [Feedback!]! @derivedFrom(field: "strategy")
  validations: [Validation!]! @derivedFrom(field: "strategy")
  listings: [Listing!]! @derivedFrom(field: "strategy")
}

type Listing @entity {
  id: ID!                           # listingId
  strategy: Strategy!               # agentNftId
  seller: Bytes!
  contentHash: Bytes!
  tier: Int!
  priceUSDC: BigInt!
  protocolFeeBps: Int!
  revoked: Boolean!
  sales: [Sale!]! @derivedFrom(field: "listing")
  attestations: [EvalAttestation!]! @derivedFrom(field: "listing")
}

type Sale @entity {
  id: ID!                           # txHash-logIndex
  listing: Listing!
  buyer: Bytes!
  priceUSDC: BigInt!
  sellerProceeds: BigInt!
  protocolProceeds: BigInt!
  blockTimestamp: BigInt!
}

type EvalAttestation @entity {
  id: ID!
  listing: Listing!
  attester: Bytes!
  evalResultHash: Bytes!
  schema: Bytes!
  postedAt: BigInt!
}
```

Marketplace browse experience reads entirely from the indexer (no centralized DB lookup), and third-party explorers can surface xvn listings without permission.

### 6.4 What an AI crawler sees

When ChatGPT/Claude/Perplexity-style crawlers (or chain-aware variants like Allium AI / Dune AI) index Mantle, they find:

1. An ERC-8004 agent #0 with name "Xianvec" and a manifest declaring marketplace endpoints.
2. A `Sold` event log with stable topics, queryable by `agentNftId` for any strategy.
3. `AttestationPosted` events keyed by `listingId` for "what evals exist for this strategy."
4. `LicenseToken` ERC-1155 transfers giving a public count of licensed users per listing.

Together: an AI agent asked "find me high-reputation trading strategies for ETH range-bound regimes" can answer entirely from on-chain reads — manifest CIDs reveal regime fit, validation registry reveals trade outcomes, sales count reveals adoption, attestations reveal eval evidence. **No xvn-API call required for browse; the API is only needed for sealed (Tier B) bundle fetches.**

### 6.5 CREATE2 deterministic addresses (the future-mirror lever)

All five new contracts deploy via a single immutable `XvnDeployer` factory:

```solidity
function deploy(bytes32 salt, bytes calldata bytecode) external returns (address);
// salt = keccak256("xvn.<contractName>.v1")
```

Salts: `keccak256("xvn.ListingRegistry.v1")`, `keccak256("xvn.Marketplace.v1")`, etc. The factory is deployed *first* on Mantle from a freshly-funded EOA whose nonce is 0; the factory's address is then fixed across chains by deploying the same EOA at nonce 0 on each.

Net effect: Marketplace at the same address on Mantle / Base / Arbitrum / Polygon, whenever we choose to mirror. AI crawlers don't need a per-chain registry to know where xvn lives — the address is the address.

---

## 7. Upgradeability + decentralization roadmap

### 7.1 Proxy pattern: UUPS

UUPS (EIP-1822) over Transparent because:

- Cheaper per-call (no admin slot collision check on every tx).
- Upgrade authority lives in the implementation contract, which means once the implementation is set to a non-upgradeable version, the proxy is permanently frozen — that's the **admin-burn primitive**.
- Battle-tested via OpenZeppelin's audited `UUPSUpgradeable`.

Each new contract (`ListingRegistry`, `Marketplace`, `LicenseToken`, `EvalAttestationRegistry`) deploys as `Proxy → Implementation v1`. The three existing 8004 registries (per ADR 0008) stay non-proxied as already specced.

### 7.2 Admin chain: 2-of-3 multisig → 7-day Timelock → Proxy

```
multisig (2-of-3 Safe)  ──schedule──►  Timelock (7d delay)  ──execute──►  Proxy.upgradeTo(newImpl)
```

- **Multisig:** 2-of-3 (founder + ops + community-trustee). Cold-stored.
- **Timelock:** OpenZeppelin's `TimelockController`, 7-day delay on all admin ops. Anyone can watch the queued tx. The timelock owns the proxy admin role.
- **Emergency `pause()`** on `Marketplace` only is exempted from the timelock — multisig can pause sales immediately if a sale-flow exploit is discovered. Pause cannot mint, burn, transfer, or change fees. Unpause is timelocked.

### 7.3 Per-contract admin powers

| Contract | Admin can | Admin cannot |
|---|---|---|
| `ListingRegistry` | upgrade implementation | rewrite existing listings, change `protocolFeeBps` snapshots |
| `Marketplace` | upgrade impl, set `protocolFeeBps` (≤ `MAX = 1000`), set fee recipient, pause/unpause | bypass `MAX_PROTOCOL_FEE_BPS`, mint license tokens directly |
| `LicenseToken` | upgrade impl, add/remove authorized minters | mint without an authorized caller, change a listing's transferable flag |
| `EvalAttestationRegistry` | upgrade impl | delete or mutate existing attestations |

### 7.4 Progressive decentralization ladder

```
v1 launch
  ├── All four new contracts: UUPS proxy + 7d timelock + 2-of-3 multisig
  └── Existing IdentityRegistry/ReputationRegistry/ValidationRegistry: already non-upgradeable

3 months post-launch (M+3) — burn LicenseToken admin
  ├── Gate: zero security incidents on LicenseToken; no needed protocol changes.
  ├── Action: multisig executes upgradeToAndCall(NonUpgradeableLicenseToken).
  └── Result: LicenseToken implementation is permanently frozen. The minter set
      becomes the only mutable surface — and minter additions still go through
      the timelock, watchable by anyone.

6 months post-launch (M+6) — burn ListingRegistry admin
  ├── Gate: schema has been stable for 6 months; no upgrade in the last 90 days.
  └── Action: same as above. Listing schema becomes permanent.

12 months post-launch (M+12) — Marketplace governance handoff
  ├── Gate: protocol-fee economics validated; treasury policy ratified.
  ├── Action: transfer Timelock admin from 2-of-3 multisig to a token-governed
  │           DAO contract OR to a 5-of-9 community-trustee multisig.
  └── EvalAttestationRegistry can either burn alongside or migrate to EAS.
```

Each gate is a public commitment, documented here; meeting or missing it is a transparent event.

### 7.5 Storage layout discipline

Every UUPS contract reserves a storage gap at the end:

```solidity
uint256[50] private __gap;
```

All new state variables in v2+ go into the gap, never above existing variables. CI runs OpenZeppelin's `storage-layout` plugin against every implementation tagged for upgrade — diffs that move existing slots fail the build. This closes the single most common upgrade-bricking bug at CI, not at code review.

### 7.6 Upgrade testing

- **Fork tests** (Foundry `vm.createFork(mantle)`) replay the upgrade against mainnet state in CI before any timelock schedule. Tests must show: balances preserved, listing IDs preserved, license token holdings preserved, all events still emittable.
- **Audit cadence:** every implementation that goes through the timelock requires an external audit before `schedule()` is called. The 7-day window covers the audit publication window — auditors and users see the diff before it lands.

---

## 8. Crate integration + deploy ordering

### 8.1 New top-level `contracts/` tree (Foundry)

```
contracts/
├── foundry.toml
├── lib/
│   ├── openzeppelin-contracts/             # standard ERC-1155, USDC interfaces
│   ├── openzeppelin-contracts-upgradeable/ # UUPS, AccessControl, Initializable
│   └── forge-std/
├── src/
│   ├── ListingRegistry.sol
│   ├── Marketplace.sol
│   ├── LicenseToken.sol
│   ├── EvalAttestationRegistry.sol
│   ├── XvnDeployer.sol                     # CREATE2 factory
│   ├── interfaces/
│   │   ├── IIdentityRegistry.sol           # the existing ADR 0008 contract
│   │   ├── IListingRegistry.sol
│   │   ├── ILicenseToken.sol
│   │   └── IEvalAttestationRegistry.sol
│   └── libraries/
│       └── Splits.sol                      # math for protocol-fee split
├── script/
│   ├── DeployTestnet.s.sol                 # Mantle Sepolia (chain 5003)
│   ├── DeployMainnet.s.sol                 # Mantle (chain 5000)
│   ├── RegisterPlatformAgent.s.sol         # one-shot 8004 mint
│   └── UpgradeTimelock.s.sol               # queue/execute helpers
└── test/
    ├── unit/
    ├── integration/
    └── fork/                               # vm.createFork(mantle) upgrade tests
```

`contracts/` is independent of the Rust workspace — Foundry-managed, its own CI lane. The Rust side consumes the JSON ABI artifacts under `contracts/out/`.

### 8.2 Rust crate responsibilities

| Crate | Role | New or existing |
|---|---|---|
| `xianvec-identity` | `alloy::sol!` bindings for **all** on-chain contracts (existing 8004 + four new). Low-level read/write helpers. | Existing — expand |
| `xianvec-marketplace` | Higher-level orchestration: `publish_listing`, `buy_listing`, `attest_eval`, `revoke_listing`. Wraps `xianvec-identity`. Holds the x402 server-side handler logic. | NEW (per Strategy Engine spec §14) |
| `xianvec-engine` | `bundle::hash`, `bundle::publish` — produces the `contentHash` + `contentURI` that gets handed to `xianvec-marketplace::publish_listing`. | NEW (already specced) |
| `xianvec-cli` | Verbs: `xvn admin register-platform-agent`, `xvn strategy publish`, `xvn marketplace buy`, `xvn marketplace attest`, `xvn admin upgrade-queue`. | Existing — extend |
| `xianvec-execution` | Posts to `ValidationRegistry` after closed Orderly trades (already specced in ADR 0008). Unchanged here. | Existing |

`xianvec-identity` is the only crate that imports `alloy::sol!`. Everything else goes through it. This isolates ABI changes to a single recompile target.

### 8.3 Deploy sequence (extends ADR 0008)

ADR 0008 covers steps 1–2 already; steps 3–9 are new in this spec.

```
Mantle Sepolia (chain 5003) — testnet first

1. IdentityRegistry          (ADR 0008, no proxy)
2. ReputationRegistry        (ADR 0008, no proxy)
3. ValidationRegistry        (NEW — referenced in spec §13 but not in ADR 0008; add here)
                             (no proxy, immutable like 1 & 2)
4. XvnDeployer               (NEW — CREATE2 factory, deployed from fresh EOA at nonce 0)
                             (deterministic address; same EOA used to deploy on every future chain)
5. LicenseToken proxy        (NEW — UUPS; minter set is empty at deploy)
6. ListingRegistry proxy     (NEW — UUPS; reads IdentityRegistry from step 1)
7. EvalAttestationRegistry   (NEW — UUPS)
8. Marketplace proxy         (NEW — UUPS; reads ListingRegistry, calls LicenseToken)
   → atomically: LicenseToken.setAuthorized(Marketplace, true)
9. RegisterPlatformAgent.s   (NEW — calls IdentityRegistry.register(platformManifestCid))
                             → emits AgentRegistered for the xvn platform
10. config/mantle-sepolia.toml updated with all eight addresses.

Mantle mainnet (chain 5000) — gated on Phase 9 eval clearing per ADR 0008

Same sequence, same XvnDeployer address (nonce-0 EOA reused), same CREATE2 salts.
Mainnet addresses for the four new contracts are predictable from the salts before
deploy — they can be embedded into config/mantle.toml ahead of time, audited
publicly, and verified post-deploy by hash equality.
```

### 8.4 Config surface

`config/mantle.toml` gains a `[marketplace]` block alongside existing blocks:

```toml
[erc8004]
identity_registry        = "0x…"
reputation_registry      = "0x…"
validation_registry      = "0x…"     # new in this spec, was implied in §13

[marketplace]
xvn_deployer             = "0x…"     # CREATE2 factory
listing_registry         = "0x…"
marketplace              = "0x…"
license_token            = "0x…"
eval_attestation         = "0x…"
platform_agent_token_id  = 0          # the IdentityRegistry NFT minted in step 9
fee_recipient            = "0x…"     # multisig
admin_multisig           = "0x…"
timelock                 = "0x…"

[marketplace.usdc]
address                  = "0x09Bc…" # USDC.e on Mantle
decimals                 = 6
```

Mirror file for Sepolia: `config/mantle-sepolia.toml`. Both checked in. Loading is selected by `XVN_NETWORK` env (`mainnet` | `sepolia`).

### 8.5 ABI handoff — keeping `xianvec-identity` honest across upgrades

When a UUPS implementation upgrades, the **proxy address stays**, but the **function selectors and ABI may change**. Mitigation:

- `xianvec-identity` pins the v1 ABI for each contract under `crates/xianvec-identity/abi/v1/*.json`, committed in-tree.
- `forge build` in `contracts/` writes ABIs to `contracts/out/`. A small CI check compares the current build output against `crates/xianvec-identity/abi/v1/` and fails if any function selector changes without an explicit version bump.
- v2 implementations live at `crates/xianvec-identity/abi/v2/`; a feature-flag chooses which version's bindings compile in. Rolling upgrades become explicit at the Rust call site — no silent ABI breakage.

### 8.6 What this changes elsewhere in the codebase

- **ADR 0008** needs an addendum: add `ValidationRegistry` to the deployable contract list, and reference this new spec for the marketplace contracts.
- **Strategy Engine spec §13** ("publish flow" + "buy flow") gets cross-references to this spec instead of holding the open question about license token contract ABI.
- **`config/mantle.toml`** schema extension (above) — small, additive.
- **No changes needed** to `xianvec-engine`, `xianvec-execution`, `xianvec-eval`, or any of the existing strategy crates — they're consumers of `xianvec-marketplace`, which is the new shim.

---

## 9. Testing strategy

### 9.1 Foundry unit tests

One test file per contract, covering:

- Happy paths for each external function.
- Access-control negative paths (unauthorized callers revert).
- Boundary conditions: `priceUSDC = 0`, `protocolFeeBps = 0`, `protocolFeeBps = MAX`, `priceUSDC = type(uint96).max`.
- Reentrancy attempts on `buy` / `buyWithAuthorization` (mock USDC with reentrant `transferFrom`).
- Snapshot semantics: changing `protocolFeeBps` does not affect existing listings.
- Soulbound enforcement: ERC-1155 transfers revert for non-transferable listing IDs.
- EIP-3009 nonce replay: same auth submitted twice → second reverts.

### 9.2 Foundry integration tests

End-to-end sale flows against in-memory deployments:

- Direct `buy` flow (approve + buy).
- x402 `buyWithAuthorization` flow with a mocked USDC implementing EIP-3009.
- Listing revocation between 402 issue and tx settlement.
- Platform-agent registration emits the expected `AgentRegistered` event with the right manifest URI.
- Eval attestation lifecycle (publish-time + third-party).

### 9.3 Foundry fork tests

Run against a Mantle fork (`vm.createFork(MANTLE_RPC)`) before any production upgrade:

- Existing listings preserved across upgrade.
- License token balances preserved across upgrade.
- Storage layout invariant: no slot moves (cross-checked against `storage-layout` plugin).
- Events still emit with v1 topic-zeros.

### 9.4 Rust integration tests

`crates/xianvec-marketplace/tests/`:

- Spin up an Anvil instance with the four new contracts deployed via the deploy script.
- Drive the full publish → list → x402-buy → fetch flow from Rust.
- Verify the `xvn admin register-platform-agent` CLI command produces the expected on-chain state.

### 9.5 Audit cadence

- v1 mainnet deploy is gated on one external audit (Trail of Bits / OpenZeppelin / equivalent). Hackathon submission may launch on Sepolia without an audit; mainnet does not.
- Every subsequent implementation upgrade also requires an audit before the timelock `schedule()` is called.

---

## 10. Future paths (documented, not built)

These are the directions flagged during brainstorming but deferred from v1. Each is recorded so future implementers don't re-relitigate.

### 10.1 Commission Option 3 — protocol fee + seller-defined royalty / referral splits

```
buyer pays priceUSDC
  ─► protocol fee     (constant, e.g. 100 bps)
  ─► curator/referrer (per-listing-configurable, e.g. 0–500 bps)
  ─► seller           (residual)
```

Implemented by extending `Listing` with a `RoyaltyConfig[]` array (recipients + bps). The `Marketplace` sale flow loops the array and sends to each recipient. ERC-2981 royalty interface compatibility considered for resale royalties (only relevant once `transferableLicense` listings have a secondary market).

### 10.2 Commission Option 4 — subscription / streaming licenses

Two variants:

- **Streaming via Superfluid-style:** the buyer opens a USDC stream to a `SubscriptionVault` contract; the vault credits the seller continuously and skims the protocol fee per second. License is "active" iff the stream's flow rate ≥ listing's required rate. License gating becomes a function of *current* stream state, not a one-time mint.
- **Pay-per-fire via x402:** strategies and skills expose per-decision endpoints (`POST /skills/sentiment/decide`) that return 402 with a small price (e.g. `0.005 USDC`). Each fire is a micropayment. License is implicit (you paid, you get the response). `LicenseToken` is *not* used here — the receipt is the response itself, plus an optional on-chain log via the same x402 facilitator path.

Both paths need a new authorized minter (or no minter at all, in the pay-per-fire case) on `LicenseToken`. Neither requires changing v1 contracts: that's the architectural payoff of Approach B.

### 10.3 Decentralized Tier B content protection

Two non-mutually-exclusive paths, both deferred from v1:

- **TEE-hosted execution** (iExec, Phala, Oasis Sapphire). Plaintext is genuinely protected from the host; trust shifts from "trust xvn-the-company" to "trust the TEE attestation." Requires `Marketplace` to verify TEE attestation receipts at fetch time but no on-chain change.
- **Threshold encryption** (Lit Protocol, Nucypher). Decryption keys gated by `LicenseToken` ownership; ciphertext on IPFS. Decentralizes *licensing* but plaintext is exposed once the buyer holds it locally — same exfil exposure as Tier A. Useful as "Tier A with on-chain access control," not real IP protection.

Tier C (envelope-encrypted) remains deferred per Strategy Engine spec §5.

### 10.4 Multi-chain mirroring for GEO

Same bytecode, same CREATE2 salts, deployed identically on Base / Arbitrum / Polygon. AI crawlers indexing any of those chains find xvn at predictable addresses. Aggregation strategy (which deployment is canonical, how listings are mirrored, whether a CCIP / LayerZero relayer syncs them, or whether each chain runs its own listings independently) is its own design exercise. v1 contracts are designed to make this possible; v1 itself is Mantle-only.

### 10.5 Refund flow on-chain

v1 has no contract path for refunds; sellers handle them off-chain. A future `RefundContract` could escrow a portion of `Sold` proceeds for N days and let buyers claim back against revoked-license signatures from the seller. Adds significant complexity for a feature that the OSShip-style centralized handling can cover initially.

### 10.6 Resale market for transferable licenses

Listings can opt into `transferableLicense = true`, but v1 ships no resale UI or order book. Future work: a thin `LicenseOrderBook` contract (or integration with an existing NFT marketplace) for licenses that allow transfer.

---

## 11. Open questions

- **EAS on Mantle:** is the canonical Ethereum Attestation Service deployed on Mantle? If yes, prefer EAS over the bespoke `EvalAttestationRegistry` for compatibility with existing EAS tooling. Default v1 plan is bespoke.
- **Multisig signer set:** the 2-of-3 — who are the three? Founder, ops, community-trustee — but the community-trustee identity is TBD before mainnet deploy.
- **Platform manifest schema URL:** `https://xianvec.dev/schemas/platform-agent.v1.json` — domain not yet provisioned. Pin to IPFS for v1 if domain ownership isn't ready.
- **Fee recipient address at v1 launch:** placeholder until treasury multisig is deployed. Document as TBD-before-mainnet.
- **Subgraph hosting:** The Graph hosted-service vs decentralized network vs a self-hosted Goldsky / Alchemy indexer. Decision deferred to deployment; affects indexer URL in platform manifest.
- **EIP-3009 support on USDC.e (Mantle):** verify the bridged USDC on Mantle supports `transferWithAuthorization`. If not, fall back to Permit2 or two-tx approve+buy and document the choice. Action item before contract finalization.

---

## 12. Decision log (this brainstorm, 2026-05-08)

- **Scope:** full contract surface for marketplace + commerce, single canonical chain (Mantle).
- **Approach:** layered (Approach B) — five new contracts, one concern per contract, each behind UUPS proxy.
- **Commission:** seller-side split, 5% default, 10% hard ceiling, snapshotted per-listing. Buyer-side fees and royalty splits deferred.
- **x402:** used as the buy-rail for full licenses via EIP-3009 `buyWithAuthorization`. Pay-per-fire and broader x402 fabric deferred.
- **Decentralization:** UUPS + 7d timelock + 2-of-3 multisig at v1 (Option 3 chosen explicitly for hackathon). Progressive admin-burn ladder at M+3 / M+6 / M+12 documented as the path to immutability.
- **Multi-chain GEO:** Mantle-only contracts; CREATE2 deterministic deploys make future mirroring on Base / Arbitrum / Polygon a deploy exercise rather than a redesign.
- **Discoverability:** xvn registers itself as ERC-8004 agent #0 in `IdentityRegistry`; reuses existing 8004 indexers (0xbits, dmihal, AgentCity) for free GEO pickup.
- **License token:** ERC-1155, soulbound by default, transferable opt-in per listing. Authorized-minter pattern enables future skill marketplace, subscription, and pay-per-fire without redeploy.
- **Sale currency:** USDC.e on Mantle only. Other assets require schema migration.
- **Tier B decentralization (TEE + threshold encryption):** documented as future paths, not v1.
- **Audit:** required before mainnet; not required for hackathon Sepolia deploy.

---

## 13. References

- ADR 0008 — ERC-8004 Registry Deployment on Mantle: [`decisions/0008-erc8004-deployment.md`](../../../decisions/0008-erc8004-deployment.md)
- Strategy Creation Engine design: [`docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`](./2026-05-08-strategy-creation-engine-design.md)
- ERC-8004 in practice (research notes): [`docs/erc-8004-agent-uses.md`](../../erc-8004-agent-uses.md)
- ERC-8004 EIP: https://eips.ethereum.org/EIPS/eip-8004
- x402 spec: https://www.x402.org
- EIP-3009 (TransferWithAuthorization): https://eips.ethereum.org/EIPS/eip-3009
- EIP-1822 (UUPS): https://eips.ethereum.org/EIPS/eip-1822
- OpenZeppelin Upgradeable Contracts: https://docs.openzeppelin.com/contracts/5.x/upgradeable
- Ethereum Attestation Service (EAS): https://attest.org
