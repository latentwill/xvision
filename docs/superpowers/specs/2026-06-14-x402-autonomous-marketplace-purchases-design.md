# x402 Autonomous Marketplace Purchases — Design Spec

- **Date:** 2026-06-14
- **Status:** Draft — awaiting operator review
- **Branch:** `feat/x402-autonomous-purchases`
- **Author:** brainstorming session (operator + Claude)

## 1. Goal

Let an autonomous agent **discover, pay for, and acquire a marketplace strategy
over the real x402 payment protocol**, settling in USDC on **Mantle mainnet**,
with no human in the loop and **no buyer funds or keys ever held by the
xvision platform**.

Two distinct wins, deliberately bundled:

1. **Interoperability (primary):** xvision exposes a spec-compliant x402
   resource server + facilitator. *Any* x402 client on the internet
   (`x402-fetch`, `x402-axios`, third-party agent fleets, discovery "bazaars")
   can pay for a listing with zero custom integration.
2. **First-party autonomy (convenience):** xvision's own MCP surface ships a
   thin x402 client so agents already driving `xvn` can buy in one tool call.

## 2. Decisions locked (operator, 2026-06-14)

| Decision | Choice | Rationale |
|---|---|---|
| Payment rail | **Real, spec-compliant x402 on Mantle, self-hosted facilitator** | Hosted facilitators (Coinbase CDP, Polygon Labs) don't list Mantle, but x402 is facilitator-agnostic; the existing `buyWithAuthorization` relay is already a proto-facilitator. Keeps all contracts on Mantle. |
| Custody model | **Non-custodial / bring-your-own-key** | Platform never holds buyer keys or funds — it only relays signed authorizations and pays gas. |
| Primary surface | **Shape B — public x402 HTTP endpoint** | Maximum interop; any x402 client can pay. |
| Convenience surface | **Shape A — MCP client** layered on B | One-call buys for agents already in `xvn`. |
| Mainnet governance | **Operator EOA as proxy admin + fee recipient** | Hackathon-grade; single key controls upgrades + fees. Explicitly accepted by operator; no multisig/timelock step. |
| Mainnet gate | **Remove `MainnetDeployIsV4Gated()`** | Deploy the marketplace contracts to Mantle mainnet (chain 5000). |

## 3. Background — what exists today (verified against `origin/main`)

The purchase stack is **real but testnet-pinned** to Mantle Sepolia (chain 5003).

- **Settlement engine already works.** `POST /api/marketplace/buy`
  (`crates/xvision-dashboard/src/routes/marketplace.rs:561-608`, handler
  `post_buy`) accepts a buyer-signed EIP-3009 authorization and relays it as
  `IMarketplace::buyWithAuthorization(...)` via
  `Erc8004MantleDriver::buy_listing` (`crates/xvision-marketplace/src/adapter.rs:336`).
  The gas signer is `XVN_PUBLISHER_PK` (`crates/xvision-dashboard/src/chain_config.rs:106-124`).
  - Request `BuyBody` (`marketplace.rs:465-472`): `{ listing_id, recipient,
    authorization: { from, to, value, valid_after, valid_before, nonce, v, r, s } }`.
  - Response `BuyOut` (`marketplace.rs:475-480`): `{ tx_hash, license_token_id }`.
  - Guard M-2: `recipient == authorization.from` enforced before any chain call.
- **No HTTP 402 anywhere.** The relay is a plain JSON POST. There is no
  resource-server route returning `402` with payment requirements, no
  `X-PAYMENT` header handling, no `/verify` + `/settle` split, no
  `X-PAYMENT-RESPONSE` header.
- **Exactly one key in the codebase.** `XVN_PUBLISHER_PK` (gas relayer). There
  is **no per-agent wallet, no keystore, no mnemonic, and no EIP-3009
  *signing* code** — only on-chain verification. `xvision-identity` takes a
  `PrivateKeySigner` as a parameter and never loads keys itself
  (`crates/xvision-identity/src/client.rs`).
- **Contracts deployed on testnet, not mainnet.** `DeployTestnet.s.sol`
  deployed 8 contracts on 5003. `DeployMainnet.s.sol:17-24` is a stub whose
  entire `run()` body is `revert MainnetDeployIsV4Gated()`.
  `MarketplaceAddresses::mantle_mainnet()` returns `None`
  (`crates/xvision-identity/src/contracts.rs:205-207`).
- **MCP has zero marketplace tools.** `crates/xvision-mcp/src/tools.rs`
  (registry at lines 579-615) covers indicators, strategy authoring, eval,
  memory, flywheel — nothing for marketplace browse/buy/import.

## 4. Architecture

### 4.1 Roles & trust domains

| Role | Signs at purchase? | Holds a key? | Lives in |
|---|---|---|---|
| **Buyer agent** | ✅ EIP-3009 `transferWithAuthorization` (the only payment signature) | Its **own** wallet key + USDC | The agent's own environment (Shape B) or the local `xvn` MCP process (Shape A) |
| **Seller** (strategy creator) | ❌ (signed once to *create* the listing; passive at buy-time) | No payment key | n/a |
| **Platform** = resource server + facilitator | ❌ no payment signature | Gas-relayer key only (`XVN_PUBLISHER_PK`) | Hosted `xvision-dashboard` |

**Non-custodial invariant:** the hosted dashboard never receives a buyer's
private key and never custodies buyer funds. It sees *signatures* and pays gas.

### 4.2 Two surfaces

- **Shape B (source of truth):** `xvision-dashboard` becomes a spec-compliant
  x402 **resource server + facilitator**. Public HTTP. Any x402 client pays it.
- **Shape A (convenience):** `xvision-mcp` (a local `xvn` stdio process in the
  agent's trust domain) ships a thin x402 **client**: it loads the agent's own
  key locally, signs locally, and runs the handshake against the Shape-B
  endpoint. The key never leaves the local process; the platform never sees it.

### 4.3 End-to-end flow (non-custodial)

```
Operator (once): fund the buyer wallet with USDC on Mantle; provide its key to
the agent's environment ONLY (never to the platform).

1. Agent → browse listings        (read-only; GET /api/marketplace/listings)
2. Agent → GET /listings/:id/x402  → HTTP 402 + { accepts: [...] }
3. Agent (locally):
   a. build EIP-3009 transferWithAuthorization typed data
      (USDC domain on Mantle, value=price, to=payTo, nonce, valid_before)
   b. SIGN locally with the buyer's own key
   c. retry with header  X-PAYMENT: base64(payload)
4. Platform facilitator:
   /verify  → off-chain EIP-712 hash + ecrecover (no chain call)
   /settle  → existing buy_listing() → buyWithAuthorization on-chain
              (relayer pays gas)  → tx_hash, license_token_id
   response carries  X-PAYMENT-RESPONSE: base64({ txHash, network, paidAt })
5. Agent → import the strategy     (POST /listings/:id/import → on-chain
            license balanceOf check → strategy installed locally)
```

## 5. Components & file map

| # | Component | Crate / file | New vs existing |
|---|---|---|---|
| C1 | 402 resource route emitting `accepts` | `xvision-dashboard/src/routes/marketplace.rs` | **new** (listing data already in snapshot) |
| C2 | `X-PAYMENT` header decode (base64 → `TransferAuthorization`) | dashboard middleware/handler | **new** (struct exists `adapter.rs:50`) |
| C3 | `POST /facilitator/verify` (EIP-712 hash + `ecrecover`) | dashboard + `xvision-marketplace` | **new crypto** |
| C4 | `POST /facilitator/settle` | wraps `adapter.rs::buy_listing` | ~existing |
| C5 | `X-PAYMENT-RESPONSE` header on success | dashboard handler | **new** (`tx_hash` already returned) |
| C6 | Client-side EIP-3009 **signing** helper | `xvision-marketplace` (or `xvision-identity`) | **new** |
| C7 | Non-custodial local key load (env/keystore) | `xvision-mcp` | **new** |
| C8 | MCP tools: browse / get_listing / buy / import / wallet | `xvision-mcp/src/tools.rs` (+ `tests/parity.rs` + matrix doc) | **new** |
| C9 | Mainnet deploy (gate removal, EOA admin, chainid guard) | `contracts/script/DeployMainnet.s.sol` | **new** (mirror of testnet) |
| C10 | Rust address wiring for mainnet | `xvision-identity/src/contracts.rs`, `config/mantle.toml`, env | **new** |
| C11 | Rate-limiting on public x402/facilitator routes | `xvision-dashboard` (per-IP token-bucket middleware) | **new** |

### 5.1 x402 protocol mapping (net-new vs existing)

| x402 piece | Exists? | What's there | Net-new |
|---|---|---|---|
| 402 body (`accepts`/paymentRequirements) | No | price + USDC + payTo in indexed snapshot | route returning 402 |
| `X-PAYMENT` decode | No | `TransferAuthorization` struct exists | base64 decode + deser |
| `/verify` (off-chain) | No | — | EIP-712 typed-data hash + `ecrecover` (alloy primitives) |
| `/settle` (on-chain) | Partial | `buy_listing()` does exactly this | expose as route; wrap in x402 shape |
| `X-PAYMENT-RESPONSE` | No | `tx_hash` returned in body | base64 response header |
| gas relay | Yes | `XVN_PUBLISHER_PK` | nothing |

`accepts` entry shape (Mantle mainnet):

```json
{
  "x402Version": 1,
  "accepts": [{
    "scheme": "exact",
    "network": "eip155:5000",
    "asset": "<usdc_address>",
    "payTo": "<marketplace_contract>",
    "maxAmountRequired": "<price_usdc_6dp>",
    "extra": { "listingId": <id> }
  }]
}
```

## 6. MCP tools (Shape A)

Add via the established 4-edit recipe (req struct with
`#[derive(Debug, Deserialize, JsonSchema)]` → `#[tool(description=...)]` fn in
the `#[tool_router]` impl → name in `tool_names()` (sorted) → name in
`EXPECTED_MCP_TOOLS` in `tests/parity.rs` (sorted)). Stateful tools open
`self.api_context().await?` against `$XVN_HOME/store.db`. Also update
`docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md`.

| Tool | Stateful? | Behavior |
|---|---|---|
| `xvn_marketplace_browse` | read-only | list listings (filters: price, tier, etc.) via dashboard read API |
| `xvn_marketplace_get_listing` | read-only | one listing + bundle hash/manifest |
| `xvn_marketplace_buy` | signs locally | full x402 handshake: GET 402 → sign EIP-3009 with local key → `X-PAYMENT` → settle. Returns `tx_hash`, `license_token_id`. **Default:** tool holds the key and signs. **Strict variant (config):** accepts a pre-signed `authorization` and never touches a key. |
| `xvn_marketplace_import` | yes | post-purchase: on-chain license `balanceOf` check → install strategy locally with fresh ULID |
| `xvn_marketplace_wallet` | read-only | buyer address + USDC/MNT balance (funding helper) |

## 7. Mainnet deploy (operator-EOA fast path)

Mirror `DeployTestnet.s.sol` (8 contracts via `XvnDeployer` CREATE2 with
`keccak256("xvn.<Name>.v1")` salts), with two deltas:

1. Replace `DeployMainnet.run()`'s revert body with the full deploy logic.
2. Add `if (block.chainid != 5000) revert WrongChain(block.chainid);`.

`admin` = `feeRecipient` = **operator EOA** for every proxy `initialize(...)`
(same as testnet). Required env: `OPERATOR_EOA`, `USDC_ADDRESS`, `LICENSE_URI`,
`PROTOCOL_FEE_BPS` (default 500 = 5%, capped 1000), optional `XVN_DEPLOYER`
(reuse only if the EOA is nonce-0 on mainnet, preserving CREATE2 address
determinism).

Post-deploy:
- `RegisterPlatformAgent.s.sol` (assert `tokenId == 0`).
- Fill `config/mantle.toml` with all 8 addresses; set `fee_recipient` + `admin`
  = operator EOA.
- Set the 8 `XVN_*` env vars so `MarketplaceAddresses::from_env()` resolves
  `Some` (`XVN_LISTING_REGISTRY` gates this), restart server.
- Optionally pin verified addresses in `mantle_mainnet()`.

Governance reality (accepted): `onlyOwner` guards `_authorizeUpgrade`,
`setProtocolFeeBps`, `setFeeRecipient`, `setUsdc`, pause/unpause on
`Marketplace.sol`. With EOA admin, one key can upgrade to arbitrary bytecode
(no delay), redirect fees, or repoint USDC. This is the accepted hackathon
posture; no timelock/multisig.

## 8. Hard gate (P0): USDC EIP-3009 on Mantle mainnet — ✅ CONFIRMED 2026-06-15

Real x402 on Mantle requires an **EIP-3009-capable USDC**. **Verified live on
Mantle mainnet (chainId 5000)** against `https://rpc.mantle.xyz` on 2026-06-15.

USDC.e at **`0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9`** is Circle's
**FiatTokenV2** (`name="USD Coin"`, `symbol="USDC"`, `decimals=6`,
`version="2"`) and implements EIP-3009:

- `authorizationState(0,0)` → `false` (function live).
- `TRANSFER_WITH_AUTHORIZATION_TYPEHASH()` → `0x7c7c6cdb67a18743f49ec6fa9b35f50d52ed05cbed4cc592e13b44501c1a2267` (canonical EIP-3009 typehash).
- `DOMAIN_SEPARATOR()` → `0x213af627bcb897cb58330ea735c1dceb19deed319fd39bbb200b6fc6bd5450cd`, which **matches** the value recomputed from the domain below (byte-for-byte).

**Exact EIP-712 domain the signer (C6) MUST use** — confirmed by matching the
on-chain `DOMAIN_SEPARATOR`:

```
EIP712Domain(string name, string version, uint256 chainId, address verifyingContract)
  name              = "USD Coin"
  version           = "2"
  chainId           = 5000
  verifyingContract = 0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9
```

**Consequence:** the EIP-3009 `exact` scheme works on Mantle as designed.
The Permit2 fallback and any 3009-wrapper deployment are **dropped** (open
question #2 resolved). Update `config/mantle.toml:34` to remove the
"illustrative; verify before mainnet" caveat.

Reproduce: `cast call <USDC> "TRANSFER_WITH_AUTHORIZATION_TYPEHASH()(bytes32)" --rpc-url https://rpc.mantle.xyz`

## 9. Testing

- **Unit (crypto):** EIP-712 domain + `transferWithAuthorization` struct hash
  and `ecrecover` against known vectors (mirror the HL EIP-712 vector approach
  already used for the degen-arena venue).
- **Unit (facilitator):** `/verify` accepts a valid sig, rejects bad
  sig/expired/spent-nonce/insufficient-value; `/settle` maps chain reverts to
  4xx with revert text.
- **Contract route:** 402 body shape; `X-PAYMENT` decode round-trip;
  `X-PAYMENT-RESPONSE` header.
- **MCP:** `parity.rs` set match + matrix doc; schema snapshot for the 5 tools.
- **End-to-end (Mantle Sepolia, before mainnet):** sign → GET 402 → verify →
  settle → import, asserting a real `tx_hash` and a locally installed strategy.
- **Interop smoke:** pay the public endpoint with an off-the-shelf x402 client
  (`x402-fetch`) to prove Shape B is spec-compliant, not just self-consistent.

## 10. Phasing

Each phase is testable on **Mantle Sepolia** before mainnet.

- **P0 — Blocking gate:** ✅ DONE (2026-06-15) — USDC.e EIP-3009 confirmed on Mantle mainnet (§8).
- **P1 — Facilitator + 402 (Shape B):** C1–C5 on testnet; interop smoke.
- **P2 — Client signing + non-custodial key:** C6, C7.
- **P3 — MCP tools (Shape A):** C8.
- **P4 — Mainnet deploy:** C9, C10 (after P0 passes).

## 11. Security considerations

- **EOA admin (accepted):** single-key control of upgrades + fees + USDC
  pointer, no delay. Documented as the operator's explicit choice.
- **Open-tier import has no address proof (pre-existing):** anyone who knows a
  license-holding address can trigger `import`. Sealed-tier closes this via a
  nonce challenge. Out of scope to change here; noted.
- **Non-custodial keys:** buyer keys live only in the agent's environment. The
  spec must ensure no log/telemetry path on the dashboard ever captures a raw
  authorization's private material (only the signature, which is safe).
- **Replay / nonce:** EIP-3009 nonces are single-use on-chain; `/verify` should
  also pre-check `authorizationState` to fail fast and avoid wasted gas.
- **Rate-limiting (C11):** the public `/x402`, `/facilitator/verify`, and
  `/facilitator/settle` routes stay unauthenticated for interop but get a
  per-IP token-bucket limiter to bound abuse / gas-drain attempts against the
  relayer. Tunable; sane hackathon defaults.

## 12. Out of scope (YAGNI)

- Multisig/timelock governance (explicitly deferred by operator).
- Per-agent HD wallets / on-platform custody (rejected in favor of BYOK).
- Cross-chain payment (e.g. pay on Base, license on Mantle) — everything stays
  on Mantle.
- Permit2 path — only if P0 fails (documented fallback, not built by default).
- Changes to listing *creation* / sealed-tier import proofs.

## 13. Open questions for review

1. ~~**Shape A key handling**~~ — ✅ RESOLVED (2026-06-15): the MCP `buy` tool
   **holds the local key and signs locally** (the autonomous default). The
   strict pre-signed-only variant is dropped for v1. Key never leaves the local
   MCP process; the platform only ever receives the signature.
2. ~~**USDC fallback appetite**~~ — ✅ RESOLVED (2026-06-15): EIP-3009 confirmed
   live on Mantle mainnet USDC.e (§8). No fallback needed; Permit2 path dropped.
3. ~~**Endpoint auth/rate-limiting**~~ — ✅ RESOLVED (2026-06-15): the public
   402/facilitator endpoints stay unauthenticated (open interop) but **add
   rate-limiting** (see C11 / §11). Per-IP token-bucket on
   `/x402`, `/facilitator/verify`, `/facilitator/settle`.
