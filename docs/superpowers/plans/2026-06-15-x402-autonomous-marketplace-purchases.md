# x402 Autonomous Marketplace Purchases — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let an autonomous agent discover, pay for (real x402 / EIP-3009 USDC on Mantle), and acquire a marketplace strategy with no human in the loop and no buyer key ever held by the platform.

**Architecture:** The hosted `xvision-dashboard` becomes a spec-compliant x402 **resource server + facilitator** wrapping the existing `buyWithAuthorization` relay (Shape B, the public source of truth). The local `xvision-mcp` process becomes a thin x402 **client** that loads the agent's own key, signs EIP-3009 authorizations locally, and drives the public endpoint (Shape A, convenience). Settlement, gas, and license-mint already exist; net-new is the protocol skin, the off-chain crypto (EIP-712 hash + ecrecover), the client signer, the MCP tools, and the Mantle-mainnet deploy.

**Tech Stack:** Rust, `alloy` (primitives + `sol!` + EIP-712 + `PrivateKeySigner`), `axum` + `tower`/`tower_http` (dashboard), `rmcp` (MCP), `reqwest` (MCP→dashboard client), Foundry (`forge`/`cast`) for contracts, Mantle mainnet (chainId 5000), USDC.e FiatTokenV2.

**Spec:** `docs/superpowers/specs/2026-06-14-x402-autonomous-marketplace-purchases-design.md`
**Branch / worktree:** `feat/x402-autonomous-purchases` @ `.worktrees/x402-autonomous-purchases` (based on `origin/main`).

**P0 status:** ✅ DONE — EIP-3009 confirmed live on Mantle mainnet USDC.e `0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9` (FiatTokenV2; canonical typehash `0x7c7c6cdb…1a2267`; domain `name="USD Coin"`, `version="2"`, `chainId=5000`).

---

## Conventions for every task

- Build/test through the disk-guard wrapper: `scripts/cargo test -p <crate>` (never bare `cargo` — see CLAUDE.md disk hygiene). Use `export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"` in this worktree.
- Format only changed files: `rustfmt --edition 2021 <file>` (do NOT run workspace `cargo fmt` — the tree isn't rustfmt-clean).
- Commit after each task with the shown message. The pre-commit hook permits commits on this feature branch inside the worktree.
- TDD: write the failing test, run it red, implement minimally, run it green, commit.

## File structure map

**Create:**
- `crates/xvision-marketplace/src/x402.rs` — EIP-3009 typed-data: typehash, signing hash, sign, recover. (pure, no I/O)
- `crates/xvision-dashboard/src/routes/x402.rs` — resource-server + facilitator routes (`/x402`, `/facilitator/verify`, `/facilitator/settle`).
- `crates/xvision-dashboard/src/ratelimit.rs` — per-IP token-bucket layer for the public x402 routes.
- `crates/xvision-mcp/src/marketplace_client.rs` — reqwest client to the dashboard + local signer (non-custodial key load + x402 handshake).
- `crates/xvision-dashboard/tests/x402_e2e.rs` — `#[ignore]` testnet end-to-end.
- `contracts/script/DeployMainnet.s.sol` — replace the gated stub with real deploy logic (gate removed).

**Modify:**
- `crates/xvision-marketplace/src/lib.rs` — `pub mod x402;`
- `crates/xvision-marketplace/src/adapter.rs` — add `fetch_listing` read (price/seller) to the driver trait + `Erc8004MantleDriver`.
- `crates/xvision-dashboard/src/routes/mod.rs` (or wherever route modules are declared) — `pub mod x402;`
- `crates/xvision-dashboard/src/server.rs` — wire the 3 new routes + the rate-limit layer.
- `crates/xvision-dashboard/Cargo.toml` — add `tower_governor`.
- `crates/xvision-mcp/src/tools.rs` — 5 new tools + the 4-edit recipe.
- `crates/xvision-mcp/src/lib.rs` — `pub mod marketplace_client;`
- `crates/xvision-mcp/Cargo.toml` — add `reqwest`, `alloy` (signer/primitives), `xvision-marketplace`.
- `crates/xvision-mcp/tests/parity.rs` — add 5 names to `EXPECTED_MCP_TOOLS`.
- `docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md` — add 5 rows.
- `config/mantle.toml` — fill deployed mainnet addresses; remove the "illustrative; verify before mainnet" caveat.
- `crates/xvision-identity/src/contracts.rs` — pin `mantle_mainnet()` addresses (optional).

---

# PHASE 1 — x402 facilitator + resource server (Shape B)

## Task 1.1: EIP-3009 typed-data — typehash + signing hash

**Files:**
- Create: `crates/xvision-marketplace/src/x402.rs`
- Modify: `crates/xvision-marketplace/src/lib.rs`
- Test: in-module `#[cfg(test)]` in `x402.rs`

- [ ] **Step 1: Add the module declaration**

In `crates/xvision-marketplace/src/lib.rs` add (alongside the other `pub mod` lines):
```rust
pub mod x402;
```

- [ ] **Step 2: Write the failing test (canonical typehash + domain separator vector)**

Create `crates/xvision-marketplace/src/x402.rs`:
```rust
//! EIP-3009 (`transferWithAuthorization`) typed-data: the off-chain crypto for
//! the x402 `exact` scheme. Pure — no network, no chain. Mirrors the EIP-712
//! pattern in `xvision-execution/src/virtuals.rs`.

use alloy::primitives::{Address, B256, U256};
use alloy::signers::local::PrivateKeySigner;
use alloy::sol;
use alloy::sol_types::{eip712_domain, Eip712Domain, SolStruct};

sol! {
    /// EIP-3009 TransferWithAuthorization payload (the EIP-712 message body).
    struct TransferWithAuthorization {
        address from;
        address to;
        uint256 value;
        uint256 validAfter;
        uint256 validBefore;
        bytes32 nonce;
    }
}

/// Build the USDC EIP-712 domain. For Mantle mainnet USDC.e (FiatTokenV2):
/// name="USD Coin", version="2", chainId=5000, verifyingContract=<usdc>.
pub fn usdc_domain(name: &str, version: &str, chain_id: u64, usdc: Address) -> Eip712Domain {
    eip712_domain! {
        name: name.to_string(),
        version: version.to_string(),
        chain_id: chain_id,
        verifying_contract: usdc,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    // Canonical EIP-3009 typehash (verified on-chain on Mantle mainnet USDC.e).
    const CANON_TYPEHASH: &str =
        "0x7c7c6cdb67a18743f49ec6fa9b35f50d52ed05cbed4cc592e13b44501c1a2267";
    // On-chain DOMAIN_SEPARATOR() of Mantle mainnet USDC.e (chainId 5000).
    const MANTLE_USDC_DOMAIN_SEP: &str =
        "0x213af627bcb897cb58330ea735c1dceb19deed319fd39bbb200b6fc6bd5450cd";
    const MANTLE_USDC: &str = "0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9";

    #[test]
    fn typehash_matches_canonical_eip3009() {
        // SolStruct exposes the encoded type string; its keccak is the typehash.
        let got = TransferWithAuthorization::eip712_type_hash();
        assert_eq!(format!("0x{:x}", got), CANON_TYPEHASH);
    }

    #[test]
    fn domain_separator_matches_mantle_mainnet() {
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain("USD Coin", "2", 5000, usdc);
        let sep = domain.separator();
        assert_eq!(format!("0x{:x}", sep), MANTLE_USDC_DOMAIN_SEP);
    }
}
```

> Note: if `eip712_type_hash()` is not a const associated fn in the pinned alloy version, compute it as `keccak256(TransferWithAuthorization::eip712_encode_type().as_bytes())` and assert that the encoded type string equals `"TransferWithAuthorization(address from,address to,uint256 value,uint256 validAfter,uint256 validBefore,bytes32 nonce)"`.

- [ ] **Step 3: Run the test (red)**

Run: `scripts/cargo test -p xvision-marketplace x402::tests -- --nocapture`
Expected: FAIL to compile (module references) → then FAIL on assertions if any helper missing. Fix until it compiles; both tests must then **pass** (they exercise only library code + constants). If they pass immediately, that's correct — the typehash/domain are pure functions of the struct + constants.

- [ ] **Step 4: Confirm green**

Run: `scripts/cargo test -p xvision-marketplace x402::tests`
Expected: `test result: ok. 2 passed`.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-marketplace/src/x402.rs crates/xvision-marketplace/src/lib.rs
git commit -m "feat(x402): EIP-3009 typed-data module — canonical typehash + Mantle domain vectors"
```

## Task 1.2: sign + recover (ecrecover round-trip)

**Files:**
- Modify: `crates/xvision-marketplace/src/x402.rs`
- Test: in-module tests

- [ ] **Step 1: Write the failing test (sign → recover round-trip)**

Append to `x402.rs` tests module:
```rust
    #[test]
    fn sign_then_recover_round_trips() {
        let signer = PrivateKeySigner::random();
        let from = signer.address();
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain("USD Coin", "2", 5000, usdc);

        let auth = Authorization {
            from,
            to: Address::from_str("0x000000000000000000000000000000000000dEaD").unwrap(),
            value: U256::from(49_000_000u64), // 49 USDC, 6dp
            valid_after: U256::ZERO,
            valid_before: U256::from(9_999_999_999u64),
            nonce: B256::repeat_byte(0x11),
        };

        let signed = sign_authorization(&signer, &auth, &domain).unwrap();
        let recovered = recover_authorizer(&auth, &domain, signed.v, signed.r, signed.s).unwrap();
        assert_eq!(recovered, from);
    }

    #[test]
    fn recover_rejects_tampered_value() {
        let signer = PrivateKeySigner::random();
        let usdc = Address::from_str(MANTLE_USDC).unwrap();
        let domain = usdc_domain("USD Coin", "2", 5000, usdc);
        let mut auth = Authorization {
            from: signer.address(),
            to: Address::ZERO,
            value: U256::from(1u64),
            valid_after: U256::ZERO,
            valid_before: U256::from(9_999_999_999u64),
            nonce: B256::ZERO,
        };
        let signed = sign_authorization(&signer, &auth, &domain).unwrap();
        auth.value = U256::from(999u64); // tamper
        let recovered = recover_authorizer(&auth, &domain, signed.v, signed.r, signed.s).unwrap();
        assert_ne!(recovered, signer.address());
    }
```

- [ ] **Step 2: Run test (red)**

Run: `scripts/cargo test -p xvision-marketplace x402::tests::sign_then_recover_round_trips`
Expected: FAIL — `Authorization`, `sign_authorization`, `recover_authorizer` not defined.

- [ ] **Step 3: Implement the types + sign + recover**

Insert into `x402.rs` (above the tests module), after `usdc_domain`:
```rust
use alloy::primitives::Signature;
use alloy::signers::SignerSync;

use crate::error::MarketplaceError;

/// The unsigned EIP-3009 authorization (host-friendly field names).
#[derive(Debug, Clone)]
pub struct Authorization {
    pub from: Address,
    pub to: Address,
    pub value: U256,
    pub valid_after: U256,
    pub valid_before: U256,
    pub nonce: B256,
}

/// Legacy-`v` (27/28) signature parts.
#[derive(Debug, Clone, Copy)]
pub struct SignedParts {
    pub v: u8,
    pub r: B256,
    pub s: B256,
}

impl Authorization {
    fn to_sol(&self) -> TransferWithAuthorization {
        TransferWithAuthorization {
            from: self.from,
            to: self.to,
            value: self.value,
            validAfter: self.valid_after,
            validBefore: self.valid_before,
            nonce: self.nonce,
        }
    }
}

/// EIP-712 digest the buyer signs.
pub fn signing_hash(auth: &Authorization, domain: &Eip712Domain) -> B256 {
    auth.to_sol().eip712_signing_hash(domain)
}

/// Sign locally with the buyer's key (non-custodial path). Never sends the key
/// anywhere — only the returned (v, r, s).
pub fn sign_authorization(
    signer: &PrivateKeySigner,
    auth: &Authorization,
    domain: &Eip712Domain,
) -> Result<SignedParts, MarketplaceError> {
    let hash = signing_hash(auth, domain);
    let sig = signer
        .sign_hash_sync(&hash)
        .map_err(|e| MarketplaceError::Signing(format!("eip3009 sign: {e}")))?;
    Ok(SignedParts {
        v: 27 + sig.v() as u8,
        r: sig.r().into(),
        s: sig.s().into(),
    })
}

/// Off-chain `ecrecover` for the facilitator `/verify` step.
pub fn recover_authorizer(
    auth: &Authorization,
    domain: &Eip712Domain,
    v: u8,
    r: B256,
    s: B256,
) -> Result<Address, MarketplaceError> {
    let hash = signing_hash(auth, domain);
    let parity = v.checked_sub(27).ok_or_else(|| MarketplaceError::Signing("bad v".into()))?;
    let sig = Signature::from_scalars_and_parity(r, s, parity != 0);
    sig.recover_address_from_prehash(&hash)
        .map_err(|e| MarketplaceError::Signing(format!("ecrecover: {e}")))
}
```

Add a `Signing` variant to `MarketplaceError` in `crates/xvision-marketplace/src/error.rs`:
```rust
    #[error("signing error: {0}")]
    Signing(String),
```

> Alloy API note: in some versions the call is `Signature::new(r_u256, s_u256, parity_bool)` and recovery is `recover_address_from_prehash`. Match the version already used in `xvision-execution/src/virtuals.rs` (it imports `alloy::signers::SignerSync` and uses `sign_hash_sync` + `sig.r()/.s()/.v()`). Convert `U256`↔`B256` with `.into()` as needed.

- [ ] **Step 4: Run tests (green)**

Run: `scripts/cargo test -p xvision-marketplace x402::tests`
Expected: 4 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-marketplace/src/x402.rs crates/xvision-marketplace/src/error.rs
git commit -m "feat(x402): EIP-3009 local sign + off-chain ecrecover with round-trip tests"
```

## Task 1.3: Driver read — fetch listing price + seller

The 402 `accepts` body needs `maxAmountRequired` (price) and `payTo`. The indexed snapshot does NOT carry price — read it from `IListingRegistry`.

**Files:**
- Modify: `crates/xvision-marketplace/src/adapter.rs`
- Test: in-module `#[cfg(test)]` (unit, against the existing test scaffolding) + the e2e in 1.9

- [ ] **Step 1: Write the failing test (signature-only compile test)**

Add to the `adapter.rs` tests module:
```rust
    #[test]
    fn listing_view_shape() {
        let v = ListingView {
            listing_id: U256::from(1u64),
            price_usdc: U256::from(49_000_000u64),
            seller: Address::ZERO,
            active: true,
        };
        assert_eq!(v.price_usdc, U256::from(49_000_000u64));
    }
```

- [ ] **Step 2: Run (red)**

Run: `scripts/cargo test -p xvision-marketplace adapter::tests::listing_view_shape`
Expected: FAIL — `ListingView` undefined.

- [ ] **Step 3: Implement `ListingView` + `fetch_listing`**

In `adapter.rs`, add the struct near `SaleReceipt`:
```rust
/// Read-model of an on-chain listing (for building x402 payment requirements).
#[derive(Debug, Clone, Copy)]
pub struct ListingView {
    pub listing_id: U256,
    /// USDC price in 6-decimal base units.
    pub price_usdc: U256,
    /// Seller payout address (informational; funds route via the Marketplace).
    pub seller: Address,
    pub active: bool,
}
```

Add to the driver trait (next to `buy_listing`):
```rust
    /// Read a single listing's price/seller/active flag from `IListingRegistry`.
    async fn fetch_listing(&self, listing_id: U256) -> Result<ListingView, MarketplaceError>;
```

Implement on `Erc8004MantleDriver` by calling `IListingRegistry::getListing(listing_id)` (the same binding the indexer uses). Map the returned tuple/struct fields to `ListingView`. Use the existing read-provider construction in this file (the `ProviderBuilder` path already present for read calls); follow whatever `getListing` decoding the indexer/`marketplace_index.rs` uses so field order matches the ABI. Mock driver: return a fixed `ListingView` for tests.

- [ ] **Step 4: Run (green)**

Run: `scripts/cargo test -p xvision-marketplace adapter::tests::listing_view_shape`
Expected: PASS. (Live `getListing` decode is covered by the e2e in Task 1.9.)

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-marketplace/src/adapter.rs
git commit -m "feat(x402): driver fetch_listing read (price/seller) for payment requirements"
```

## Task 1.4: `accepts` builder + `GET /api/marketplace/listings/:id/x402` (HTTP 402)

**Files:**
- Create: `crates/xvision-dashboard/src/routes/x402.rs`
- Modify: route module declaration + `server.rs` (route wiring in Task 1.8)
- Test: in-module test for the `accepts` builder (pure)

- [ ] **Step 1: Write the failing test (accepts builder)**

Create `crates/xvision-dashboard/src/routes/x402.rs`:
```rust
//! x402 resource server + facilitator. Wraps the existing buyWithAuthorization
//! relay in the standard HTTP-402 protocol so any x402 client can pay.

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::error::DashboardError;
use crate::state::AppState; // adjust path to where AppState lives

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PaymentRequirements {
    pub scheme: String,             // "exact"
    pub network: String,            // "eip155:5000"
    pub asset: String,              // USDC address
    #[serde(rename = "payTo")]
    pub pay_to: String,             // Marketplace contract
    #[serde(rename = "maxAmountRequired")]
    pub max_amount_required: String, // decimal USDC base units
    pub resource: String,           // canonical resource URL
    pub extra: serde_json::Value,   // { "listingId": <id> }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Accepts {
    #[serde(rename = "x402Version")]
    pub x402_version: u8,
    pub accepts: Vec<PaymentRequirements>,
}

/// Pure builder — no chain access; caller supplies the on-chain price/addresses.
pub fn build_accepts(
    chain_id: u64,
    usdc: &str,
    marketplace: &str,
    listing_id: u64,
    price_usdc: &str,
) -> Accepts {
    Accepts {
        x402_version: 1,
        accepts: vec![PaymentRequirements {
            scheme: "exact".into(),
            network: format!("eip155:{chain_id}"),
            asset: usdc.to_string(),
            pay_to: marketplace.to_string(),
            max_amount_required: price_usdc.to_string(),
            resource: format!("/api/marketplace/listings/{listing_id}/x402"),
            extra: serde_json::json!({ "listingId": listing_id }),
        }],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_has_exact_scheme_and_network() {
        let a = build_accepts(5000, "0xUSDC", "0xMKT", 42, "49000000");
        assert_eq!(a.x402_version, 1);
        let pr = &a.accepts[0];
        assert_eq!(pr.scheme, "exact");
        assert_eq!(pr.network, "eip155:5000");
        assert_eq!(pr.max_amount_required, "49000000");
        assert_eq!(pr.extra["listingId"], 42);
    }
}
```

- [ ] **Step 2: Run (red)**

Run: `scripts/cargo test -p xvision-dashboard x402::tests::accepts_has_exact_scheme_and_network`
Expected: FAIL to compile (module not declared). Declare `pub mod x402;` in the routes module file, fix the `AppState`/`DashboardError` import paths to match the crate, then the test passes.

- [ ] **Step 3: Implement the 402 handler**

Append to `routes/x402.rs`:
```rust
/// `GET /api/marketplace/listings/:id/x402`
/// No `X-PAYMENT` header → 402 with payment requirements.
/// With a valid `X-PAYMENT` header → behaves like settle (see Task 1.7).
pub async fn get_x402(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    if headers.get("x-payment").is_some() {
        return super::x402::settle_from_header(state, id, headers).await;
    }

    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref()).ok_or_else(|| {
        DashboardError::ServiceUnavailable("chain relay not configured".into())
    })?;
    let addrs = mp.and_then(|c| c.marketplace_addresses.clone()).ok_or_else(|| {
        DashboardError::ServiceUnavailable("marketplace not configured".into())
    })?;

    let driver = xvision_marketplace::adapter::Erc8004MantleDriver::new(
        addrs.clone(),
        chain.rpc_url.clone(),
        chain.chain_id,
    );
    let view = driver
        .fetch_listing(alloy::primitives::U256::from(id))
        .await
        .map_err(DashboardError::from)?;

    let body = build_accepts(
        chain.chain_id,
        &format!("0x{:x}", addrs.usdc),
        &format!("0x{:x}", addrs.marketplace),
        id,
        &view.price_usdc.to_string(),
    );
    Ok((StatusCode::PAYMENT_REQUIRED, Json(body)).into_response())
}
```

> If `DashboardError` has no `From<MarketplaceError>`, add one mapping chain/read errors to `ServiceUnavailable`/`BadGateway`.

- [ ] **Step 4: Run (green) + manual check deferred to 1.9**

Run: `scripts/cargo test -p xvision-dashboard x402::tests`
Expected: PASS (the builder test). The live 402 is exercised in Task 1.9.

- [ ] **Step 5: Commit**

```bash
git add crates/xvision-dashboard/src/routes/x402.rs crates/xvision-dashboard/src/routes/mod.rs
git commit -m "feat(x402): payment-requirements builder + GET /listings/:id/x402 (HTTP 402)"
```

## Task 1.5: `X-PAYMENT` header decode

The header is base64(JSON) of `{ x402Version, scheme, network, payload: { authorization, signature } }` (x402 `exact`/EVM). Decode into the existing `AuthorizationBody` shape.

**Files:**
- Modify: `crates/xvision-dashboard/src/routes/x402.rs`
- Test: in-module decode test

- [ ] **Step 1: Failing test**
```rust
    #[test]
    fn decode_x_payment_roundtrip() {
        let json = serde_json::json!({
            "x402Version": 1,
            "scheme": "exact",
            "network": "eip155:5000",
            "payload": {
                "authorization": {
                    "from":"0x1111111111111111111111111111111111111111",
                    "to":"0x2222222222222222222222222222222222222222",
                    "value":"49000000","validAfter":"0","validBefore":"9999999999",
                    "nonce":"0x33"
                },
                "signature":"0x"  // not parsed here; v/r/s split done in decode
            }
        });
        use base64::Engine;
        let hdr = base64::engine::general_purpose::STANDARD.encode(json.to_string());
        let decoded = decode_x_payment(&hdr).unwrap();
        assert_eq!(decoded.listing_value, "49000000");
    }
```

- [ ] **Step 2: Run (red)** — `decode_x_payment` undefined.

- [ ] **Step 3: Implement decode**

Add `base64` to `crates/xvision-dashboard/Cargo.toml` if absent (`base64 = "0.22"`). Implement:
```rust
#[derive(Debug, Deserialize)]
struct XPaymentEnvelope {
    payload: XPaymentPayload,
}
#[derive(Debug, Deserialize)]
struct XPaymentPayload {
    authorization: XPaymentAuth,
    signature: String, // 65-byte 0x sig; split into v/r/s
}
#[derive(Debug, Deserialize)]
struct XPaymentAuth {
    from: String, to: String, value: String,
    #[serde(rename = "validAfter")] valid_after: String,
    #[serde(rename = "validBefore")] valid_before: String,
    nonce: String,
}

/// Decoded x402 payment, normalized to the relay's `AuthorizationBody`.
pub struct DecodedPayment {
    pub from: String,
    pub authorization: crate::routes::marketplace::AuthorizationBody,
    pub listing_value: String,
}

pub fn decode_x_payment(header: &str) -> Result<DecodedPayment, DashboardError> {
    use base64::Engine;
    let raw = base64::engine::general_purpose::STANDARD
        .decode(header.trim())
        .map_err(|e| DashboardError::BadRequest(format!("x-payment base64: {e}")))?;
    let env: XPaymentEnvelope = serde_json::from_slice(&raw)
        .map_err(|e| DashboardError::BadRequest(format!("x-payment json: {e}")))?;
    let sig = env.payload.signature.trim_start_matches("0x");
    let bytes = hex::decode(sig).map_err(|e| DashboardError::BadRequest(format!("sig hex: {e}")))?;
    if bytes.len() != 65 {
        return Err(DashboardError::BadRequest("sig must be 65 bytes".into()));
    }
    let r = format!("0x{}", hex::encode(&bytes[0..32]));
    let s = format!("0x{}", hex::encode(&bytes[32..64]));
    let v = bytes[64] as u64;
    let a = env.payload.authorization;
    Ok(DecodedPayment {
        from: a.from.clone(),
        listing_value: a.value.clone(),
        authorization: crate::routes::marketplace::AuthorizationBody {
            from: a.from.clone(),
            to: a.to,
            value: a.value,
            valid_after: a.valid_after.parse().unwrap_or(0),
            valid_before: a.valid_before.parse().unwrap_or(0),
            nonce: a.nonce,
            v, r, s,
        },
    })
}
```

> Make `AuthorizationBody`/`BuyBody`/`build_buy_request` `pub` in `routes/marketplace.rs` if not already.

- [ ] **Step 4: Run (green)** — `scripts/cargo test -p xvision-dashboard x402::tests::decode_x_payment_roundtrip`

- [ ] **Step 5: Commit**
```bash
git add crates/xvision-dashboard/src/routes/x402.rs crates/xvision-dashboard/src/routes/marketplace.rs crates/xvision-dashboard/Cargo.toml
git commit -m "feat(x402): decode X-PAYMENT header into the relay AuthorizationBody"
```

## Task 1.6: `POST /facilitator/verify`

Off-chain validation: recover the signer (Task 1.2), check `from == authorization.from`, `value >= price`, `valid_before > now`, and pre-check on-chain `authorizationState(from, nonce) == false`.

**Files:** Modify `routes/x402.rs`. Test: unit for the pure checks.

- [ ] **Step 1: Failing test (pure verification logic)**
```rust
    #[test]
    fn verify_rejects_underpayment_and_expiry() {
        let now = 1_000u64;
        assert!(check_terms(/*value*/ "49000000", /*price*/ "49000000", /*valid_before*/ 2000, now).is_ok());
        assert!(check_terms("10000000", "49000000", 2000, now).is_err()); // underpay
        assert!(check_terms("49000000", "49000000", 999, now).is_err());  // expired
    }
```

- [ ] **Step 2: Run (red).**

- [ ] **Step 3: Implement `check_terms` + the route**
```rust
pub fn check_terms(value: &str, price: &str, valid_before: u64, now: u64) -> Result<(), DashboardError> {
    let v: u128 = value.parse().map_err(|_| DashboardError::BadRequest("value".into()))?;
    let p: u128 = price.parse().map_err(|_| DashboardError::BadRequest("price".into()))?;
    if v < p { return Err(DashboardError::BadRequest("insufficient payment".into())); }
    if valid_before <= now { return Err(DashboardError::BadRequest("authorization expired".into())); }
    Ok(())
}

#[derive(Serialize)]
pub struct VerifyOut { pub valid: bool, pub payer: String }

/// `POST /api/marketplace/facilitator/verify` — body is the X-PAYMENT JSON (un-base64'd).
pub async fn post_verify(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<VerifyOut>, DashboardError> {
    // Accept either an X-PAYMENT header or the JSON in the body.
    let hdr = headers.get("x-payment").and_then(|h| h.to_str().ok()).map(str::to_string);
    let decoded = match hdr {
        Some(h) => decode_x_payment(&h)?,
        None => {
            use base64::Engine;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&body);
            decode_x_payment(&b64)?
        }
    };
    // recover signer
    let payer = recover_payer(&state, &decoded)?; // helper that builds Authorization + domain, calls x402::recover_authorizer
    if format!("0x{:x}", payer).to_lowercase() != decoded.from.to_lowercase() {
        return Err(DashboardError::BadRequest("signature/from mismatch".into()));
    }
    Ok(Json(VerifyOut { valid: true, payer: format!("0x{:x}", payer) }))
}
```

Implement `recover_payer(state, decoded)`: read `marketplace_addresses.usdc` + `chain.chain_id`, build the domain via `xvision_marketplace::x402::usdc_domain("USD Coin","2", chain_id, usdc)`, build `x402::Authorization` from `decoded.authorization`, parse `r/s` to `B256`, call `x402::recover_authorizer`. Include the `authorizationState(from, nonce)` pre-check via a driver read (add `is_authorization_used(from, nonce)` to the driver, or inline an `IERC3009` call) — fail with `BadRequest("authorization already used")` if true.

- [ ] **Step 4: Run (green)** — `scripts/cargo test -p xvision-dashboard x402::tests::verify_rejects_underpayment_and_expiry`

- [ ] **Step 5: Commit**
```bash
git add crates/xvision-dashboard/src/routes/x402.rs crates/xvision-marketplace/src/adapter.rs
git commit -m "feat(x402): facilitator /verify — recover signer, terms + nonce precheck"
```

## Task 1.7: `POST /facilitator/settle` + `X-PAYMENT-RESPONSE`

Wraps the existing settlement: build a `BuyRequest` from the decoded payment and call `driver.buy_listing` (gas paid by `XVN_PUBLISHER_PK`). On success, set the `X-PAYMENT-RESPONSE` header.

**Files:** Modify `routes/x402.rs`. Test: header-encoding unit; live path in 1.9.

- [ ] **Step 1: Failing test (response header encoder)**
```rust
    #[test]
    fn payment_response_header_encodes() {
        let h = encode_payment_response("0xabc", "eip155:5000", 1700000000);
        use base64::Engine;
        let decoded = base64::engine::general_purpose::STANDARD.decode(h).unwrap();
        let v: serde_json::Value = serde_json::from_slice(&decoded).unwrap();
        assert_eq!(v["txHash"], "0xabc");
        assert_eq!(v["network"], "eip155:5000");
    }
```

- [ ] **Step 2: Run (red).**

- [ ] **Step 3: Implement settle + header**
```rust
pub fn encode_payment_response(tx_hash: &str, network: &str, paid_at: u64) -> String {
    use base64::Engine;
    let body = serde_json::json!({ "success": true, "txHash": tx_hash, "network": network, "paidAt": paid_at });
    base64::engine::general_purpose::STANDARD.encode(body.to_string())
}

/// `POST /api/marketplace/facilitator/settle` and the X-PAYMENT branch of GET /x402.
pub async fn settle_from_header(
    state: AppState,
    listing_id: u64,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    let hdr = headers.get("x-payment").and_then(|h| h.to_str().ok())
        .ok_or_else(|| DashboardError::BadRequest("missing X-PAYMENT".into()))?;
    let decoded = decode_x_payment(hdr)?;

    // Reuse the relay's request builder + driver path.
    let body = crate::routes::marketplace::BuyBody {
        listing_id,
        recipient: decoded.from.clone(),         // non-custodial: recipient == payer (M-2)
        authorization: decoded.authorization,
    };
    let req = crate::routes::marketplace::build_buy_request(&body)?;

    let mp = state.marketplace_chain();
    let chain = mp.and_then(|c| c.chain.as_ref())
        .ok_or_else(|| DashboardError::ServiceUnavailable("chain relay not configured".into()))?;
    let addrs = mp.and_then(|c| c.marketplace_addresses.clone())
        .ok_or_else(|| DashboardError::ServiceUnavailable("marketplace not configured".into()))?;
    let net = format!("eip155:{}", chain.chain_id);

    let driver = xvision_marketplace::adapter::Erc8004MantleDriver::with_signer(
        addrs, chain.rpc_url.clone(), chain.chain_id, chain.signer.clone());
    let receipt = driver.buy_listing(req).await.map_err(DashboardError::from)?;

    let tx = format!("0x{:x}", receipt.tx_hash);
    let paid_at = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    let mut resp = Json(crate::routes::marketplace::BuyOut {
        tx_hash: tx.clone(),
        license_token_id: receipt.license_token_id.to_string(),
    }).into_response();
    resp.headers_mut().insert(
        "x-payment-response",
        encode_payment_response(&tx, &net, paid_at).parse().unwrap(),
    );
    Ok(resp)
}

pub async fn post_settle(
    State(state): State<AppState>,
    Path(id): Path<u64>,
    headers: HeaderMap,
) -> Result<Response, DashboardError> {
    settle_from_header(state, id, headers).await
}
```

- [ ] **Step 4: Run (green)** — `scripts/cargo test -p xvision-dashboard x402::tests::payment_response_header_encodes`

- [ ] **Step 5: Commit**
```bash
git add crates/xvision-dashboard/src/routes/x402.rs
git commit -m "feat(x402): facilitator /settle wrapping buy_listing + X-PAYMENT-RESPONSE header"
```

## Task 1.8: wire routes + per-IP rate limit (C11)

**Files:**
- Modify: `crates/xvision-dashboard/src/server.rs`, `crates/xvision-dashboard/Cargo.toml`
- Create: `crates/xvision-dashboard/src/ratelimit.rs`

- [ ] **Step 1: Add dependency**

In `crates/xvision-dashboard/Cargo.toml` add a per-IP limiter compatible with the crate's axum version:
```toml
tower_governor = "0.4"   # match to the axum major already in use; adjust if needed
```

- [ ] **Step 2: Write the failing test (limiter config builds)**

Create `crates/xvision-dashboard/src/ratelimit.rs`:
```rust
//! Per-IP token-bucket limiter for the public x402/facilitator routes.
use std::sync::Arc;
use tower_governor::governor::GovernorConfigBuilder;
use tower_governor::GovernorLayer;

/// Sane hackathon default: ~5 req/s burst 20 per IP. Tunable via env.
pub fn x402_rate_limit_layer() -> GovernorLayer<'static, tower_governor::key_extractor::PeerIpKeyExtractor, governor::middleware::NoOpMiddleware> {
    let per_ms: u64 = std::env::var("XVN_X402_RATELIMIT_REPLENISH_MS").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(200); // 1 token / 200ms = 5/s
    let burst: u32 = std::env::var("XVN_X402_RATELIMIT_BURST").ok()
        .and_then(|v| v.parse().ok()).unwrap_or(20);
    let cfg = GovernorConfigBuilder::default()
        .per_millisecond(per_ms)
        .burst_size(burst)
        .finish()
        .expect("valid governor config");
    GovernorLayer { config: Arc::new(cfg) }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn layer_builds_with_defaults() {
        let _ = x402_rate_limit_layer();
    }
}
```

> The exact `GovernorLayer`/`GovernorConfigBuilder` type signatures vary by `tower_governor` version. Pin the version, then let the compiler guide the generic params — keep the public fn name `x402_rate_limit_layer` stable. If `tower_governor` proves incompatible with the axum version, fall back to a hand-rolled `tower::Layer` wrapping `governor::RateLimiter` keyed by `ConnectInfo<SocketAddr>`.

- [ ] **Step 3: Run (red→green)**

Run: `scripts/cargo test -p xvision-dashboard ratelimit::tests::layer_builds_with_defaults`
Fix compile/version issues until green.

- [ ] **Step 4: Wire the routes + layer into the router**

In `server.rs`: declare `mod ratelimit;` and add to `build_router` a public sub-router carrying the limiter (these routes must NOT be behind `require_auth` — they're public):
```rust
    let x402_public = Router::new()
        .route("/api/marketplace/listings/:id/x402", get(routes::x402::get_x402))
        .route("/api/marketplace/facilitator/verify", post(routes::x402::post_verify))
        .route("/api/marketplace/facilitator/settle/:id", post(routes::x402::post_settle))
        .layer(ratelimit::x402_rate_limit_layer())
        .with_state(state.clone());
```
and `.merge(x402_public)` into the assembled `Router`. Ensure `ConnectInfo` is available: the server must be started with `.into_make_service_with_connect_info::<SocketAddr>()` (check the bind site; add it if missing — required for `PeerIpKeyExtractor`).

- [ ] **Step 5: Run dashboard tests + commit**
```bash
scripts/cargo test -p xvision-dashboard
git add crates/xvision-dashboard/src/ratelimit.rs crates/xvision-dashboard/src/server.rs crates/xvision-dashboard/Cargo.toml
git commit -m "feat(x402): wire 402/verify/settle routes with per-IP rate limiting"
```

## Task 1.9: end-to-end testnet integration test (`#[ignore]`)

**Files:** Create `crates/xvision-dashboard/tests/x402_e2e.rs`

- [ ] **Step 1: Write the test (ignored; runs against Mantle Sepolia)**
```rust
//! End-to-end x402 on Mantle Sepolia. Requires env:
//!   XVN_RPC_URL, XVN_CHAIN_ID=5003, XVN_PUBLISHER_PK (gas relayer, funded),
//!   XVN_AGENT_PK (buyer, holds test USDC via faucet), XVN_LISTING_REGISTRY,
//!   XVN_MARKETPLACE_CONTRACT, XVN_MARKETPLACE_USDC, X402_TEST_LISTING_ID.
//! Run: cargo test -p xvision-dashboard --test x402_e2e -- --ignored --nocapture
#[tokio::test]
#[ignore]
async fn x402_buy_round_trip_testnet() {
    // 1. GET /x402 → 402 + accepts
    // 2. sign EIP-3009 with XVN_AGENT_PK via xvision_marketplace::x402::sign_authorization
    // 3. POST settle with X-PAYMENT → assert 200 + tx_hash + X-PAYMENT-RESPONSE
    // 4. assert license balanceOf(agent, listing_id) > 0
    // (Build the app via the same router builder; spawn on a random port.)
    assert!(true, "fill in once Task 1.1–1.8 land");
}
```

- [ ] **Step 2: Implement against the running router** (spawn `build_router` on an ephemeral port, drive with `reqwest`). Keep it `#[ignore]` so CI doesn't need testnet creds.

- [ ] **Step 3: Run locally with creds**

Run: `scripts/cargo test -p xvision-dashboard --test x402_e2e -- --ignored --nocapture`
Expected: real `tx_hash`, license minted to the buyer.

- [ ] **Step 4: Commit**
```bash
git add crates/xvision-dashboard/tests/x402_e2e.rs
git commit -m "test(x402): testnet end-to-end (ignored) — 402 → sign → settle → license"
```

---

# PHASE 2 — Client signing + non-custodial key (Shape A foundation)

## Task 2.1: MCP dependencies + marketplace client module skeleton

**Files:**
- Modify: `crates/xvision-mcp/Cargo.toml`, `crates/xvision-mcp/src/lib.rs`
- Create: `crates/xvision-mcp/src/marketplace_client.rs`

- [ ] **Step 1: Add deps** to `crates/xvision-mcp/Cargo.toml`:
```toml
reqwest = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
alloy = { workspace = true }              # primitives + signers::local (match workspace)
xvision-marketplace = { path = "../xvision-marketplace" }
```

- [ ] **Step 2: Failing test (key load from env)**

Create `crates/xvision-mcp/src/marketplace_client.rs`:
```rust
//! Non-custodial x402 client: loads the agent's OWN key locally (never sent to
//! the platform), signs EIP-3009 authorizations, and drives the dashboard's
//! public x402 endpoint.

use alloy::signers::local::PrivateKeySigner;

/// Resolve the buyer signer from the local environment only.
/// `XVN_AGENT_PK` (0x hex). Errors if unset — non-custodial means the operator
/// must provide it; the platform never holds it.
pub fn load_agent_signer() -> Result<PrivateKeySigner, String> {
    let pk = std::env::var("XVN_AGENT_PK")
        .map_err(|_| "XVN_AGENT_PK not set (non-custodial: provide the buyer key locally)".to_string())?;
    pk.trim().parse::<PrivateKeySigner>().map_err(|e| format!("XVN_AGENT_PK invalid: {e}"))
}

/// Dashboard base URL the MCP client talks to. Default localhost dev server.
pub fn api_base() -> String {
    std::env::var("XVN_MARKETPLACE_API").unwrap_or_else(|_| "http://127.0.0.1:8080".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agent_signer_errors_without_env() {
        // Note: test runs without XVN_AGENT_PK set in CI env.
        if std::env::var("XVN_AGENT_PK").is_err() {
            assert!(load_agent_signer().is_err());
        }
    }
    #[test]
    fn api_base_has_default() {
        std::env::remove_var("XVN_MARKETPLACE_API");
        assert!(api_base().starts_with("http"));
    }
}
```

Add `pub mod marketplace_client;` to `crates/xvision-mcp/src/lib.rs`.

- [ ] **Step 3: Run (green)** — `scripts/cargo test -p xvision-mcp marketplace_client::tests`

- [ ] **Step 4: Commit**
```bash
git add crates/xvision-mcp/Cargo.toml crates/xvision-mcp/src/lib.rs crates/xvision-mcp/src/marketplace_client.rs
git commit -m "feat(mcp): non-custodial agent key load + marketplace client skeleton"
```

## Task 2.2: x402 client handshake (browse + buy + import calls)

**Files:** Modify `crates/xvision-mcp/src/marketplace_client.rs`

- [ ] **Step 1: Failing test (nonce + Authorization construction is deterministic-ish)**
```rust
    #[test]
    fn build_authorization_sets_value_and_expiry() {
        use alloy::primitives::{Address, U256};
        let from = Address::ZERO;
        let to = Address::ZERO;
        let auth = build_authorization(from, to, U256::from(49_000_000u64), 600);
        assert_eq!(auth.value, U256::from(49_000_000u64));
        assert!(auth.valid_before > auth.valid_after);
    }
```

- [ ] **Step 2: Run (red).**

- [ ] **Step 3: Implement the client methods**
```rust
use alloy::primitives::{Address, B256, U256};
use serde::Deserialize;
use xvision_marketplace::x402::{self, Authorization};

#[derive(Debug, Deserialize)]
pub struct AcceptsResp { pub accepts: Vec<PaymentReq> }
#[derive(Debug, Deserialize)]
pub struct PaymentReq {
    pub network: String, pub asset: String,
    #[serde(rename = "payTo")] pub pay_to: String,
    #[serde(rename = "maxAmountRequired")] pub max_amount_required: String,
}

pub fn build_authorization(from: Address, to: Address, value: U256, ttl_secs: u64) -> Authorization {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
    // random 32-byte nonce
    let mut n = [0u8; 32];
    getrandom::getrandom(&mut n).expect("rng");
    Authorization {
        from, to, value,
        valid_after: U256::ZERO,
        valid_before: U256::from(now + ttl_secs),
        nonce: B256::from(n),
    }
}

/// Full non-custodial buy: GET 402 → sign locally → POST settle with X-PAYMENT.
pub async fn buy(listing_id: u64) -> Result<serde_json::Value, String> {
    let signer = load_agent_signer()?;
    let base = api_base();
    let http = reqwest::Client::new();

    // 1. discover requirements (expect 402)
    let r = http.get(format!("{base}/api/marketplace/listings/{listing_id}/x402"))
        .send().await.map_err(|e| e.to_string())?;
    let reqs: AcceptsResp = r.json().await.map_err(|e| e.to_string())?;
    let pr = reqs.accepts.into_iter().next().ok_or("no payment requirements")?;

    let chain_id: u64 = pr.network.strip_prefix("eip155:").and_then(|s| s.parse().ok())
        .ok_or("bad network")?;
    let usdc: Address = pr.asset.parse().map_err(|_| "bad asset")?;
    let pay_to: Address = pr.pay_to.parse().map_err(|_| "bad payTo")?;
    let value: U256 = pr.max_amount_required.parse().map_err(|_| "bad amount")?;

    // 2. sign locally (key never leaves this process)
    let auth = build_authorization(signer.address(), pay_to, value, 600);
    let domain = x402::usdc_domain("USD Coin", "2", chain_id, usdc);
    let parts = x402::sign_authorization(&signer, &auth, &domain).map_err(|e| e.to_string())?;

    // 3. assemble X-PAYMENT and settle
    let sig_hex = format!("0x{}{}{:02x}",
        hex::encode(parts.r.as_slice()), hex::encode(parts.s.as_slice()), parts.v);
    let envelope = serde_json::json!({
        "x402Version": 1, "scheme": "exact", "network": pr.network,
        "payload": {
            "authorization": {
                "from": format!("0x{:x}", auth.from), "to": format!("0x{:x}", auth.to),
                "value": auth.value.to_string(),
                "validAfter": auth.valid_after.to_string(),
                "validBefore": auth.valid_before.to_string(),
                "nonce": format!("0x{:x}", auth.nonce)
            },
            "signature": sig_hex
        }
    });
    use base64::Engine;
    let xpay = base64::engine::general_purpose::STANDARD.encode(envelope.to_string());
    let resp = http.post(format!("{base}/api/marketplace/facilitator/settle/{listing_id}"))
        .header("x-payment", xpay).send().await.map_err(|e| e.to_string())?;
    let status = resp.status();
    let body: serde_json::Value = resp.json().await.map_err(|e| e.to_string())?;
    if !status.is_success() { return Err(format!("settle {status}: {body}")); }
    Ok(body)
}

pub async fn browse() -> Result<serde_json::Value, String> {
    let http = reqwest::Client::new();
    http.get(format!("{}/api/marketplace/listings", api_base()))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn get_listing(id: u64) -> Result<serde_json::Value, String> {
    let http = reqwest::Client::new();
    http.get(format!("{}/api/marketplace/listings/{id}", api_base()))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}

pub async fn import(id: u64) -> Result<serde_json::Value, String> {
    let signer = load_agent_signer()?;
    let http = reqwest::Client::new();
    http.post(format!("{}/api/marketplace/listings/{id}/import", api_base()))
        .json(&serde_json::json!({ "address": format!("0x{:x}", signer.address()) }))
        .send().await.map_err(|e| e.to_string())?
        .json().await.map_err(|e| e.to_string())
}
```

Add `getrandom` and `hex` and `base64` to `crates/xvision-mcp/Cargo.toml` if absent.

- [ ] **Step 4: Run (green)** — `scripts/cargo test -p xvision-mcp marketplace_client`

- [ ] **Step 5: Commit**
```bash
git add crates/xvision-mcp/src/marketplace_client.rs crates/xvision-mcp/Cargo.toml
git commit -m "feat(mcp): x402 client handshake — browse/get/buy/import over the public endpoint"
```

---

# PHASE 3 — MCP tools (Shape A)

Each tool follows the 4-edit recipe: req struct (`#[derive(Debug, Deserialize, JsonSchema)]`) → `#[tool(description=...)]` fn in the `#[tool_router]` impl → name in `tool_names()` (sorted) → name in `EXPECTED_MCP_TOOLS` in `tests/parity.rs` (sorted). After all five, add five rows to `docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md`.

The five names (sorted): `xvn_marketplace_browse`, `xvn_marketplace_buy`, `xvn_marketplace_get_listing`, `xvn_marketplace_import`, `xvn_marketplace_wallet`.

## Task 3.1: `xvn_marketplace_browse` + `xvn_marketplace_get_listing`

**Files:** Modify `crates/xvision-mcp/src/tools.rs`, `crates/xvision-mcp/tests/parity.rs`

- [ ] **Step 1: Failing test (parity)**

Add all five names to `EXPECTED_MCP_TOOLS` in `tests/parity.rs` (sorted, between existing entries — e.g. after `xvn_macd`).

Run: `scripts/cargo test -p xvision-mcp --test parity`
Expected: FAIL — live set is missing the five names.

- [ ] **Step 2: Implement browse + get_listing tools**

In `tools.rs`, add request structs:
```rust
#[derive(Debug, Default, Deserialize, JsonSchema)]
pub struct MarketplaceGetReq { pub listing_id: u64 }
```
Add tool fns inside the `#[tool_router]` impl:
```rust
    #[tool(description = "Browse marketplace listings (chain-indexed). Read-only.")]
    async fn xvn_marketplace_browse(&self) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::browse().await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }

    #[tool(description = "Get one marketplace listing + bundle manifest by numeric id.")]
    async fn xvn_marketplace_get_listing(
        &self, Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::get_listing(req.listing_id).await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }
```
Add both names to `tool_names()` (sorted).

- [ ] **Step 3: Run (red→green)** — `scripts/cargo test -p xvision-mcp --test parity` will still be red until ALL five exist (Tasks 3.2/3.3 add the rest). To keep tasks green-per-task, add all five names to BOTH `tool_names()` and the tools across 3.1–3.3 in one branch; commit per logical group. Pragmatic: implement all five tool fns + names now, then split commits. (Parity is all-or-nothing.)

- [ ] **Step 4: Commit**
```bash
git add crates/xvision-mcp/src/tools.rs
git commit -m "feat(mcp): xvn_marketplace_browse + get_listing tools"
```

## Task 3.2: `xvn_marketplace_wallet`

- [ ] **Step 1: Implement**
```rust
    #[tool(description = "Show the local agent wallet address + USDC/native balance (funding helper).")]
    async fn xvn_marketplace_wallet(&self) -> Result<String, rmcp::ErrorData> {
        let signer = crate::marketplace_client::load_agent_signer()
            .map_err(|e| rmcp::ErrorData::invalid_params(e, None))?;
        // Balance read: call dashboard /api/marketplace/wallet?address=... (exists) or
        // read USDC.balanceOf via a light RPC call. Minimal v1: return the address.
        json_or_err(&serde_json::json!({ "address": format!("0x{:x}", signer.address()) }))
    }
```
Add name to `tool_names()`.

- [ ] **Step 2: Commit**
```bash
git add crates/xvision-mcp/src/tools.rs
git commit -m "feat(mcp): xvn_marketplace_wallet tool"
```

## Task 3.3: `xvn_marketplace_buy` + `xvn_marketplace_import`

- [ ] **Step 1: Implement**
```rust
    #[tool(description = "Autonomously buy a listing over x402 (signs locally with XVN_AGENT_PK, never sends the key). Returns tx_hash + license_token_id.")]
    async fn xvn_marketplace_buy(
        &self, Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::buy(req.listing_id).await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }

    #[tool(description = "Import a purchased listing: verifies the on-chain license then installs the strategy locally.")]
    async fn xvn_marketplace_import(
        &self, Parameters(req): Parameters<MarketplaceGetReq>,
    ) -> Result<String, rmcp::ErrorData> {
        let v = crate::marketplace_client::import(req.listing_id).await
            .map_err(|e| rmcp::ErrorData::internal_error(e, None))?;
        json_or_err(&v)
    }
```
Confirm all five names now in `tool_names()` (sorted) and `EXPECTED_MCP_TOOLS`.

- [ ] **Step 2: Run parity (green)**

Run: `scripts/cargo test -p xvision-mcp --test parity`
Expected: PASS.

- [ ] **Step 3: Update the parity matrix doc**

Add five rows to `docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md` (match the existing table columns).

- [ ] **Step 4: Commit**
```bash
git add crates/xvision-mcp/src/tools.rs crates/xvision-mcp/tests/parity.rs docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md
git commit -m "feat(mcp): xvn_marketplace_buy + import tools; parity + matrix updated"
```

---

# PHASE 4 — Mantle mainnet deploy (operator-EOA fast path)

> Manual/ops tasks. On the local build/workstation only — never run `forge`/`cast`/`cargo` on a small deploy host (CLAUDE.md). Source `.op_env` before `gh`/`op`.

## Task 4.1: replace the V4 gate with real deploy logic

**Files:** `contracts/script/DeployMainnet.s.sol`

- [ ] **Step 1:** Replace the `run()` body (currently `revert MainnetDeployIsV4Gated()`) with the full 8-contract deploy from `DeployTestnet.s.sol`, adding at the top:
```solidity
if (block.chainid != 5000) revert WrongChain(block.chainid);
```
Keep `admin == feeRecipient == OPERATOR_EOA` (fast path), and `USDC_ADDRESS` from env. Keep the `XvnDeployer` CREATE2 salts identical to testnet (`keccak256("xvn.<Name>.v1")`).

- [ ] **Step 2: Build the contracts**

Run: `cd contracts && forge build`
Expected: compiles; `DeployMainnet` no longer reverts at compile-evaluation.

- [ ] **Step 3: Dry-run (no broadcast)**

Run (env set: `OPERATOR_EOA`, `USDC_ADDRESS=0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9`, `LICENSE_URI`, `PROTOCOL_FEE_BPS=500`):
```bash
forge script script/DeployMainnet.s.sol --rpc-url https://rpc.mantle.xyz
```
Expected: predicted addresses print; no revert.

- [ ] **Step 4: Commit**
```bash
git add contracts/script/DeployMainnet.s.sol
git commit -m "feat(contracts): mainnet deploy script — remove V4 gate, chainid==5000 guard, EOA admin"
```

## Task 4.2: broadcast + register platform agent (live mainnet)

- [ ] **Step 1: Fund the deployer EOA** with MNT for gas. Confirm it is **nonce-0** if reusing the testnet `XVN_DEPLOYER` address (else deploy a fresh factory).

- [ ] **Step 2: Broadcast**
```bash
forge script script/DeployMainnet.s.sol --rpc-url https://rpc.mantle.xyz --broadcast --private-key "$DEPLOYER_PK"
```

- [ ] **Step 3: Register platform agent**
```bash
IDENTITY_REGISTRY=<deployed> forge script script/RegisterPlatformAgent.s.sol --rpc-url https://rpc.mantle.xyz --broadcast --private-key "$DEPLOYER_PK"
```
Assert `tokenId == 0` in the output.

- [ ] **Step 4: Verify USDC wiring on-chain**
```bash
cast call <Marketplace> "usdc()(address)" --rpc-url https://rpc.mantle.xyz
# expect 0x09Bc4E0D864854c6aFB6eB9A9cdF58aC190D0dF9
```

## Task 4.3: config + Rust wiring

**Files:** `config/mantle.toml`, `crates/xvision-identity/src/contracts.rs`

- [ ] **Step 1:** Fill `config/mantle.toml` with all 8 deployed addresses; set `fee_recipient` and `admin` to the operator EOA; **remove** the `usdc` "illustrative; verify before mainnet" caveat (now confirmed).

- [ ] **Step 2 (optional pin):** Implement `MarketplaceAddresses::mantle_mainnet()` to return `Some(Self { .. })` with the verified addresses (currently returns `None`). Add a unit test asserting the pinned `usdc` equals `0x09Bc…0dF9`.

Run: `scripts/cargo test -p xvision-identity contracts`

- [ ] **Step 3:** Set the 8 `XVN_*` env vars in the server's process env (`XVN_LISTING_REGISTRY` gates `from_env()`): `XVN_MARKETPLACE_CONTRACT`, `XVN_LICENSE_TOKEN`, `XVN_EVAL_ATTESTATION`, `XVN_VALIDATION_REGISTRY`, `XVN_MARKETPLACE_USDC`, `XVN_MARKETPLACE_DEPLOYER`, `XVN_MARKETPLACE_PLATFORM_AGENT_TOKEN_ID`, plus `XVN_RPC_URL=https://rpc.mantle.xyz`, `XVN_CHAIN_ID=5000`, `XVN_PUBLISHER_PK`.

- [ ] **Step 4: Commit**
```bash
git add config/mantle.toml crates/xvision-identity/src/contracts.rs
git commit -m "feat(contracts): wire Mantle mainnet addresses (config + pinned MarketplaceAddresses)"
```

## Task 4.4: mainnet smoke (real x402 purchase)

- [ ] **Step 1:** With the server running on mainnet config + `XVN_AGENT_PK` funded with USDC, drive the MCP `xvn_marketplace_buy` on a cheap test listing (or a price-0 listing first).

- [ ] **Step 2:** Confirm: 402 returned `accepts` with `eip155:5000`; settle returned a real `tx_hash` + `X-PAYMENT-RESPONSE`; `LicenseToken.balanceOf(agent, listing_id) > 0` on-chain; `xvn_marketplace_import` installed the strategy locally.

- [ ] **Step 3:** Record the mainnet smoke evidence (tx hash + screenshots) under `docs/superpowers/evidence/` and update the spec status to "shipped".

---

## Self-review (completed during authoring)

- **Spec coverage:** C1 (Task 1.4), C2 (1.5), C3 (1.6 + 1.2), C4 (1.7), C5 (1.7), C6 (2.1/2.2), C7 (2.1), C8 (3.1–3.3), C9 (4.1/4.2), C10 (4.3), C11 (1.8). P0 done. Rate-limiting (open Q#3) = Task 1.8. MCP-signs-locally (open Q#1) = Task 2.2/3.3. EIP-3009 (open Q#2) = confirmed, no Permit2 task. ✅ all spec items mapped.
- **Type consistency:** `Authorization`, `SignedParts`, `sign_authorization`, `recover_authorizer`, `usdc_domain`, `build_accepts`, `decode_x_payment`, `settle_from_header`, `encode_payment_response`, `ListingView`, `fetch_listing`, `load_agent_signer`, `api_base`, `build_authorization`, `buy/browse/get_listing/import` are defined once and referenced consistently across tasks.
- **Placeholders:** none — every code step carries real code. Version-sensitive spots (alloy `Signature` ctor, `tower_governor` generics) carry explicit "match the pinned version" notes rather than TODOs.

## Known version-sensitivity callouts (verify against pinned crates, not blockers)

1. alloy `Signature::from_scalars_and_parity` vs `Signature::new` — mirror `xvision-execution/src/virtuals.rs`.
2. `SolStruct::eip712_type_hash()` const-ness — fallback to `keccak256(eip712_encode_type())`.
3. `tower_governor` ↔ axum version compatibility — fallback to a hand-rolled `tower::Layer` + `governor`.
4. `getrandom`/`hex`/`base64` presence in each crate's `Cargo.toml` — add if missing.
