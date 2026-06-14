//! Virtuals **Degen Arena** venue — Hyperliquid perps executed by a
//! **trade-only HL agent wallet that XVN holds directly and signs natively in
//! Rust**. No npm subprocess, no fund-capable key — see the custody invariant
//! (§2) and "no shell-out" rule (§6) in
//! `docs/superpowers/plans/2026-06-13-virtuals-degen-deploy-plan.md`.
//!
//! ## Why not reuse `byreal.rs`?
//! `byreal.rs` also reaches Hyperliquid, but by shelling out to
//! `npx @byreal-io/byreal-perps-cli@latest`, which reads the private key from
//! the inherited process env. That violates §6 (the unpinned package + its
//! transitive deps can read the key and place adversarial trades) and isn't
//! Arena-eligible. This module reuses byreal's *learnings* — the HL agent-wallet
//! model, bare-ticker symbols, signed-size convention — but signs natively.
//!
//! ## Naming
//! - **Trait** [`HyperliquidApi`] (+ [`ReqwestHyperliquidApi`] real impl,
//!   [`MockHyperliquidApi`] test seam) — the venue is *mechanically* Hyperliquid,
//!   so the trait is named for what it actually does (matching `OrderlyApi` /
//!   `ByrealPerpsApi`). The plan's `AcpRail` name is avoided here precisely
//!   because trade signing is native HL EIP-712, **not** ACP — ACP is only ever
//!   an eligibility/attribution seam, added later, never the signer.
//! - **Surface** [`DegenArenaSurface`] (+ `BrokerKind::DegenArena`) — "Degen
//!   Arena" is the product/venue surfaced to operators.
//!
//! Structure mirrors [`crate::orderly::OrderlyLiveSurface`] and
//! [`crate::byreal::ByrealLiveSurface`]: an inner [`HyperliquidApi`] trait is the
//! mockable seam, wrapped by [`DegenArenaSurface`] which implements the
//! venue-agnostic [`BrokerSurface`] the live-eval engine drives.

use std::str::FromStr;
use std::sync::Mutex;
use std::time::Duration;

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use async_trait::async_trait;

use xvision_core::AssetSymbol;

use crate::broker_surface::{BrokerSurface, OrderConfirmation, OrderRequest, Side};
use crate::executor::ExecutorError;

/// Hyperliquid mainnet REST base. Reads + `/exchange` share the host.
pub const HL_MAINNET_BASE: &str = "https://api.hyperliquid.xyz";
/// Hyperliquid testnet REST base.
pub const HL_TESTNET_BASE: &str = "https://api.hyperliquid-testnet.xyz";

/// Default aggressive-IOC slippage for "market" orders, in basis points. HL has
/// no native market type; a market order is an IOC limit priced through the book
/// by this much. 500 bps (5%) matches the HL Python SDK's `slippage` default.
const DEFAULT_SLIPPAGE_BPS: f64 = 500.0;

// ── Trait seam types ────────────────────────────────────────────────────────

/// A reduce-only trigger (stop-loss / take-profit) bracket leg.
#[derive(Debug, Clone, PartialEq)]
pub struct HlTrigger {
    /// Price at which the market trigger fires.
    pub trigger_px: f64,
    /// `"tp"` (take-profit) or `"sl"` (stop-loss).
    pub tpsl: String,
}

/// One order to place on Hyperliquid. `coin` is the bare perp ticker (`"BTC"`).
/// The real impl resolves the perp-universe asset index internally.
#[derive(Debug, Clone, PartialEq)]
pub struct HlOrderReq {
    pub coin: String,
    pub is_buy: bool,
    /// Limit price (for a "market" order this is the slippage-adjusted price; for
    /// a trigger order it's the post-trigger marketable price).
    pub px: f64,
    /// Base-asset size (e.g. 0.05 BTC).
    pub sz: f64,
    pub reduce_only: bool,
    /// Optional 128-bit client order id (`0x` + 32 hex) for venue-side dedupe.
    pub cloid: Option<String>,
    /// When set, this is a market **trigger** order (a reduce-only TP/SL bracket
    /// leg) instead of a plain IOC limit.
    pub trigger: Option<HlTrigger>,
}

/// Acknowledgement of a placed order.
#[derive(Debug, Clone, PartialEq)]
pub struct HlOrderAck {
    pub oid: u64,
    /// `"filled"` or `"resting"`.
    pub status: String,
    pub avg_px: Option<f64>,
    pub filled_sz: f64,
}

/// One open perp position. `szi` is signed (positive long, negative short) —
/// Hyperliquid's native convention.
#[derive(Debug, Clone, PartialEq)]
pub struct HlPosition {
    pub coin: String,
    pub szi: f64,
}

/// The mockable seam. Each method maps to one Hyperliquid REST primitive
/// (`/exchange` for `place_order`; `/info clearinghouseState` for reads).
#[async_trait]
pub trait HyperliquidApi: Send + Sync {
    /// Sign + place an order via `POST /exchange`.
    async fn place_order(&self, order: HlOrderReq) -> Result<HlOrderAck, ExecutorError>;
    /// Signed positions for the account address (`clearinghouseState`).
    async fn positions(&self) -> Result<Vec<HlPosition>, ExecutorError>;
    /// Account equity in USD (`marginSummary.accountValue`).
    async fn account_value(&self) -> Result<f64, ExecutorError>;
}

// ── Native HL L1-action signing ─────────────────────────────────────────────
//
// Verified byte-for-byte against the Hyperliquid Python SDK's published test
// vectors (tests/signing_test.py) — see the `sign::tests` module below.
mod sign {
    use super::*;
    use alloy::primitives::{keccak256, B256, U256};
    use alloy::signers::SignerSync;
    use alloy::sol;
    use alloy::sol_types::{eip712_domain, SolStruct};
    use serde::Serialize;

    sol! {
        /// The EIP-712 "phantom agent" Hyperliquid signs for an L1 action.
        struct Agent {
            string source;
            bytes32 connectionId;
        }
    }

    /// `float_to_wire` — HL prices/sizes are strings: max 8 decimals, trailing
    /// zeros stripped, `-0` normalised to `0`. Mirrors the Python SDK exactly.
    pub(super) fn float_to_wire(x: f64) -> String {
        let mut s = format!("{x:.8}");
        while s.ends_with('0') {
            s.pop();
        }
        if s.ends_with('.') {
            s.pop();
        }
        if s == "-0" {
            "0".to_string()
        } else {
            s
        }
    }

    #[derive(Serialize, Clone)]
    pub(super) struct LimitParams {
        pub tif: String,
    }

    /// Trigger (stop / take-profit) order params. Field order
    /// `isMarket,triggerPx,tpsl` matches the HL Python SDK's
    /// `order_type_to_wire`.
    #[derive(Serialize, Clone)]
    pub(super) struct TriggerParams {
        #[serde(rename = "isMarket")]
        pub is_market: bool,
        #[serde(rename = "triggerPx")]
        pub trigger_px: String,
        /// `"tp"` or `"sl"`.
        pub tpsl: String,
    }

    /// The order `t` field — exactly one of `limit` / `trigger` is set, so the
    /// msgpack is a single-key map (`{"limit":{…}}` or `{"trigger":{…}}`),
    /// matching HL's wire.
    #[derive(Serialize, Clone)]
    pub(super) struct OrderTypeWire {
        #[serde(skip_serializing_if = "Option::is_none")]
        pub limit: Option<LimitParams>,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub trigger: Option<TriggerParams>,
    }

    impl OrderTypeWire {
        /// A limit order with the given time-in-force (`"Ioc"`, `"Gtc"`, `"Alo"`).
        pub(super) fn limit(tif: &str) -> Self {
            Self {
                limit: Some(LimitParams { tif: tif.into() }),
                trigger: None,
            }
        }

        /// A market trigger order (stop / take-profit) at `trigger_px`.
        pub(super) fn trigger(trigger_px: String, tpsl: &str) -> Self {
            Self {
                limit: None,
                trigger: Some(TriggerParams {
                    is_market: true,
                    trigger_px,
                    tpsl: tpsl.into(),
                }),
            }
        }
    }

    /// One order's wire shape. **Field declaration order is the msgpack key
    /// order** the HL server re-hashes against — do NOT reorder (`a,b,p,s,r,t,c`).
    #[derive(Serialize, Clone)]
    pub(super) struct OrderWire {
        pub a: u32,
        pub b: bool,
        pub p: String,
        pub s: String,
        pub r: bool,
        pub t: OrderTypeWire,
        #[serde(rename = "c", skip_serializing_if = "Option::is_none")]
        pub cloid: Option<String>,
    }

    /// The `order` action envelope. Field order `type,orders,grouping`.
    #[derive(Serialize)]
    pub(super) struct OrderAction {
        #[serde(rename = "type")]
        pub type_: String,
        pub orders: Vec<OrderWire>,
        pub grouping: String,
    }

    /// secp256k1 signature decomposed into the HL wire fields.
    #[derive(Debug, Clone, Copy)]
    pub(super) struct SigParts {
        pub r: U256,
        pub s: U256,
        /// Legacy Ethereum `v` (27 or 28) — HL wire format, not 0/1 parity.
        pub v: u64,
    }

    /// keccak256( msgpack(action) ++ nonce_be8 ++ vault_suffix ) → connectionId.
    /// `vault_suffix` is `0x00` with no vault, else `0x01 ++ 20-byte address`.
    pub(super) fn action_hash<T: Serialize>(
        action: &T,
        nonce: u64,
        vault: Option<Address>,
    ) -> Result<B256, ExecutorError> {
        let mut data = rmp_serde::to_vec_named(action)
            .map_err(|e| ExecutorError::Internal(format!("hl msgpack: {e}")))?;
        data.extend_from_slice(&nonce.to_be_bytes());
        match vault {
            None => data.push(0),
            Some(a) => {
                data.push(1);
                data.extend_from_slice(a.as_slice());
            }
        }
        Ok(keccak256(&data))
    }

    /// EIP-712-sign the phantom agent. Domain is fixed
    /// (`Exchange`/`1`/chainId `1337`/zero contract) for BOTH networks; the
    /// mainnet/testnet distinction is carried only by `source` (`"a"`/`"b"`).
    pub(super) fn sign_connection_id(
        signer: &PrivateKeySigner,
        connection_id: B256,
        is_mainnet: bool,
    ) -> Result<SigParts, ExecutorError> {
        let agent = Agent {
            source: if is_mainnet { "a" } else { "b" }.to_string(),
            connectionId: connection_id,
        };
        let domain = eip712_domain! {
            name: "Exchange",
            version: "1",
            chain_id: 1337u64,
            verifying_contract: Address::ZERO,
        };
        let hash = agent.eip712_signing_hash(&domain);
        let sig = signer
            .sign_hash_sync(&hash)
            .map_err(|e| ExecutorError::Auth(format!("hl sign: {e}")))?;
        Ok(SigParts {
            r: sig.r(),
            s: sig.s(),
            v: 27 + sig.v() as u64,
        })
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        // Python SDK test key (tests/signing_test.py).
        const TEST_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";

        fn order(a: u32, p: &str, s: &str, tif: &str) -> OrderWire {
            OrderWire {
                a,
                b: true,
                p: p.into(),
                s: s.into(),
                r: false,
                t: OrderTypeWire::limit(tif),
                cloid: None,
            }
        }

        #[test]
        fn float_to_wire_matches_python_sdk() {
            assert_eq!(float_to_wire(1670.1), "1670.1");
            assert_eq!(float_to_wire(100.0), "100");
            assert_eq!(float_to_wire(0.0147), "0.0147");
            assert_eq!(float_to_wire(-0.0), "0");
        }

        // No published signing vector exists for trigger orders, so guard the
        // wire SHAPE: exactly one of limit/trigger present, camelCase renames.
        #[test]
        fn order_type_wire_shapes() {
            assert_eq!(
                serde_json::to_value(OrderTypeWire::limit("Ioc")).unwrap(),
                serde_json::json!({"limit": {"tif": "Ioc"}})
            );
            assert_eq!(
                serde_json::to_value(OrderTypeWire::trigger("103.5".into(), "sl")).unwrap(),
                serde_json::json!({"trigger": {"isMarket": true, "triggerPx": "103.5", "tpsl": "sl"}})
            );
        }

        // Vector 1 — ETH IOC order, nonce 1677777606040, no vault.
        #[test]
        fn vector1_order_connection_id() {
            let action = OrderAction {
                type_: "order".into(),
                orders: vec![order(4, "1670.1", "0.0147", "Ioc")],
                grouping: "na".into(),
            };
            let h = action_hash(&action, 1677777606040, None).unwrap();
            assert_eq!(
                h,
                B256::from_str("0x0fcbeda5ae3c4950a548021552a4fea2226858c4453571bf3f24ba017eac2908").unwrap()
            );
        }

        // Vector 2 — dummy action, nonce 0, no vault. Validates the full
        // msgpack→hash→EIP-712→v pipeline against a known signature.
        #[test]
        fn vector2_dummy_signature_mainnet_and_testnet() {
            #[derive(Serialize)]
            struct Dummy {
                #[serde(rename = "type")]
                type_: String,
                num: u64,
            }
            let signer = TEST_KEY.parse::<PrivateKeySigner>().unwrap();
            let action = Dummy {
                type_: "dummy".into(),
                num: 100000000000,
            };
            let h = action_hash(&action, 0, None).unwrap();

            let m = sign_connection_id(&signer, h, true).unwrap();
            assert_eq!(
                m.r,
                U256::from_str("0x53749d5b30552aeb2fca34b530185976545bb22d0b3ce6f62e31be961a59298").unwrap()
            );
            assert_eq!(
                m.s,
                U256::from_str("0x755c40ba9bf05223521753995abb2f73ab3229be8ec921f350cb447e384d8ed8").unwrap()
            );
            assert_eq!(m.v, 27);

            let t = sign_connection_id(&signer, h, false).unwrap();
            assert_eq!(
                t.r,
                U256::from_str("0x542af61ef1f429707e3c76c5293c80d01f74ef853e34b76efffcb57e574f9510").unwrap()
            );
            assert_eq!(
                t.s,
                U256::from_str("0x17b8b32f086e8cdede991f1e2c529f5dd5297cbe8128500e00cbaf766204a613").unwrap()
            );
            assert_eq!(t.v, 28);
        }

        // Vector 3 — GTC limit order (a=1, p=100, s=100), nonce 0. Validates the
        // order-wire encoding specifically.
        #[test]
        fn vector3_order_signature_mainnet_and_testnet() {
            let signer = TEST_KEY.parse::<PrivateKeySigner>().unwrap();
            let action = OrderAction {
                type_: "order".into(),
                orders: vec![order(1, "100", "100", "Gtc")],
                grouping: "na".into(),
            };
            let h = action_hash(&action, 0, None).unwrap();

            let m = sign_connection_id(&signer, h, true).unwrap();
            assert_eq!(
                m.r,
                U256::from_str("0xd65369825a9df5d80099e513cce430311d7d26ddf477f5b3a33d2806b100d78e").unwrap()
            );
            assert_eq!(
                m.s,
                U256::from_str("0x2b54116ff64054968aa237c20ca9ff68000f977c93289157748a3162b6ea940e").unwrap()
            );
            assert_eq!(m.v, 28);

            let t = sign_connection_id(&signer, h, false).unwrap();
            assert_eq!(
                t.r,
                U256::from_str("0x82b2ba28e76b3d761093aaded1b1cdad4960b3af30212b343fb2e6cdfa4e3d54").unwrap()
            );
            assert_eq!(t.v, 27);
        }
    }
}

// ── Response parsing (pure, unit-tested without the network) ─────────────────

/// Parse a `/exchange` order response into an [`HlOrderAck`]. Handles the
/// `status:"ok"` happy path (`filled` / `resting`), per-order `error` strings,
/// and the top-level `status:"err"` envelope.
fn parse_order_response(v: &serde_json::Value) -> Result<HlOrderAck, ExecutorError> {
    let status = v.get("status").and_then(|s| s.as_str()).unwrap_or("");
    if status != "ok" {
        let msg = v
            .get("response")
            .and_then(|r| r.as_str())
            .unwrap_or("unknown error");
        return Err(ExecutorError::Rejected(format!("hyperliquid: {msg}")));
    }
    let statuses = v
        .pointer("/response/data/statuses")
        .and_then(|s| s.as_array())
        .ok_or_else(|| ExecutorError::Internal("hl: missing response.data.statuses".into()))?;
    let first = statuses
        .first()
        .ok_or_else(|| ExecutorError::Internal("hl: empty statuses".into()))?;
    if let Some(err) = first.get("error").and_then(|e| e.as_str()) {
        return Err(ExecutorError::Rejected(format!("hyperliquid: {err}")));
    }
    if let Some(filled) = first.get("filled") {
        return Ok(HlOrderAck {
            oid: filled.get("oid").and_then(|o| o.as_u64()).unwrap_or(0),
            status: "filled".into(),
            avg_px: filled
                .get("avgPx")
                .and_then(|p| p.as_str())
                .and_then(|s| s.parse::<f64>().ok()),
            filled_sz: filled
                .get("totalSz")
                .and_then(|p| p.as_str())
                .and_then(|s| s.parse::<f64>().ok())
                .unwrap_or(0.0),
        });
    }
    if let Some(resting) = first.get("resting") {
        return Ok(HlOrderAck {
            oid: resting.get("oid").and_then(|o| o.as_u64()).unwrap_or(0),
            status: "resting".into(),
            avg_px: None,
            filled_sz: 0.0,
        });
    }
    Err(ExecutorError::Internal(format!(
        "hl: unrecognized order status: {first}"
    )))
}

/// Parse `clearinghouseState.assetPositions[].position` into signed positions.
fn parse_positions(v: &serde_json::Value) -> Vec<HlPosition> {
    v.get("assetPositions")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|ap| {
                    let p = ap.get("position")?;
                    let coin = p.get("coin")?.as_str()?.to_string();
                    let szi = p.get("szi")?.as_str()?.parse::<f64>().ok()?;
                    Some(HlPosition { coin, szi })
                })
                .collect()
        })
        .unwrap_or_default()
}

/// Parse `clearinghouseState.marginSummary.accountValue` (USD equity).
fn parse_account_value(v: &serde_json::Value) -> f64 {
    v.pointer("/marginSummary/accountValue")
        .and_then(|s| s.as_str())
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0)
}

/// Sum the USDC balance from a `spotClearinghouseState` response. On a
/// Hyperliquid **unified account** the perp `marginSummary.accountValue` is not
/// meaningful — the USDC that collateralizes perps lives in the spot state — so
/// this is the equity fallback (see [`HyperliquidApi::account_value`]).
fn parse_spot_usdc(v: &serde_json::Value) -> f64 {
    v.get("balances")
        .and_then(|b| b.as_array())
        .map(|arr| {
            arr.iter()
                .filter(|b| b.get("coin").and_then(|c| c.as_str()) == Some("USDC"))
                .filter_map(|b| {
                    b.get("total")
                        .and_then(|t| t.as_str())
                        .and_then(|s| s.parse::<f64>().ok())
                })
                .sum()
        })
        .unwrap_or(0.0)
}

// ── ReqwestHyperliquidApi (production) ───────────────────────────────────────

/// Production [`HyperliquidApi`]: native EIP-712 signing with the trade-only HL
/// agent wallet + raw `reqwest` to `/exchange` and `/info`. No subprocess.
pub struct ReqwestHyperliquidApi {
    http: reqwest::Client,
    base_url: String,
    /// Master account address — reads (`clearinghouseState`) are by address;
    /// the agent wallet is sign-only.
    address: Address,
    /// Trade-only HL agent wallet. Cannot withdraw (HL protocol-enforced).
    signer: PrivateKeySigner,
    is_mainnet: bool,
    /// Cached perp-universe coin names (index = asset id for the order wire).
    meta: Mutex<Option<Vec<String>>>,
}

impl ReqwestHyperliquidApi {
    pub fn new(
        http: reqwest::Client,
        base_url: impl Into<String>,
        address: Address,
        signer: PrivateKeySigner,
        is_mainnet: bool,
    ) -> Self {
        Self {
            http,
            base_url: base_url.into(),
            address,
            signer,
            is_mainnet,
            meta: Mutex::new(None),
        }
    }

    async fn info(&self, body: serde_json::Value) -> Result<serde_json::Value, ExecutorError> {
        let resp = self
            .http
            .post(format!("{}/info", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| ExecutorError::Network(format!("hl info: {e}")))?;
        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| ExecutorError::Network(format!("hl info decode: {e}")))
    }

    async fn asset_index_for(&self, coin: &str) -> Result<u32, ExecutorError> {
        {
            let g = self.meta.lock().unwrap();
            if let Some(names) = g.as_ref() {
                return names
                    .iter()
                    .position(|n| n.eq_ignore_ascii_case(coin))
                    .map(|i| i as u32)
                    .ok_or_else(|| {
                        ExecutorError::Rejected(format!(
                            "hyperliquid unsupported asset (not in perp universe): {coin}"
                        ))
                    });
            }
        }
        let v = self.info(serde_json::json!({ "type": "meta" })).await?;
        let names: Vec<String> = v
            .get("universe")
            .and_then(|u| u.as_array())
            .ok_or_else(|| ExecutorError::Internal("hl meta missing universe".into()))?
            .iter()
            .filter_map(|a| a.get("name").and_then(|n| n.as_str()).map(String::from))
            .collect();
        let idx = names
            .iter()
            .position(|n| n.eq_ignore_ascii_case(coin))
            .map(|i| i as u32);
        *self.meta.lock().unwrap() = Some(names);
        idx.ok_or_else(|| {
            ExecutorError::Rejected(format!(
                "hyperliquid unsupported asset (not in perp universe): {coin}"
            ))
        })
    }
}

/// Millisecond UNIX timestamp for the action nonce.
fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[async_trait]
impl HyperliquidApi for ReqwestHyperliquidApi {
    async fn place_order(&self, order: HlOrderReq) -> Result<HlOrderAck, ExecutorError> {
        let a = self.asset_index_for(&order.coin).await?;
        // A trigger order (TP/SL bracket leg) uses the `trigger` order type;
        // everything else is an aggressive IOC limit ("market").
        let order_type = match &order.trigger {
            Some(t) => sign::OrderTypeWire::trigger(sign::float_to_wire(t.trigger_px), &t.tpsl),
            None => sign::OrderTypeWire::limit("Ioc"),
        };
        let action = sign::OrderAction {
            type_: "order".into(),
            orders: vec![sign::OrderWire {
                a,
                b: order.is_buy,
                p: sign::float_to_wire(order.px),
                s: sign::float_to_wire(order.sz),
                r: order.reduce_only,
                t: order_type,
                cloid: order.cloid.clone(),
            }],
            grouping: "na".into(),
        };
        let nonce = now_ms();
        let h = sign::action_hash(&action, nonce, None)?;
        let sig = sign::sign_connection_id(&self.signer, h, self.is_mainnet)?;
        let body = serde_json::json!({
            "action": serde_json::to_value(&action)
                .map_err(|e| ExecutorError::Internal(format!("hl action encode: {e}")))?,
            "nonce": nonce,
            "signature": {
                "r": format!("0x{:x}", sig.r),
                "s": format!("0x{:x}", sig.s),
                "v": sig.v,
            },
            "vaultAddress": serde_json::Value::Null,
        });
        let resp = self
            .http
            .post(format!("{}/exchange", self.base_url))
            .json(&body)
            .send()
            .await
            .map_err(|e| ExecutorError::Network(format!("hl exchange: {e}")))?;
        let v = resp
            .json::<serde_json::Value>()
            .await
            .map_err(|e| ExecutorError::Network(format!("hl exchange decode: {e}")))?;
        parse_order_response(&v)
    }

    async fn positions(&self) -> Result<Vec<HlPosition>, ExecutorError> {
        let v = self
            .info(serde_json::json!({
                "type": "clearinghouseState",
                "user": format!("{:#x}", self.address),
            }))
            .await?;
        Ok(parse_positions(&v))
    }

    async fn account_value(&self) -> Result<f64, ExecutorError> {
        let perp = self
            .info(serde_json::json!({
                "type": "clearinghouseState",
                "user": format!("{:#x}", self.address),
            }))
            .await?;
        let perp_value = parse_account_value(&perp);
        if perp_value > 0.0 {
            return Ok(perp_value);
        }
        // Unified account: the perp marginSummary reads 0 / "not meaningful"
        // (HL docs) — the real USDC collateral lives in the spot clearinghouse
        // state. Fall back to it so equity/sizing isn't reported as $0 on a
        // funded unified account (the bug the testnet smoke surfaced).
        let spot = self
            .info(serde_json::json!({
                "type": "spotClearinghouseState",
                "user": format!("{:#x}", self.address),
            }))
            .await?;
        Ok(parse_spot_usdc(&spot))
    }
}

// ── DegenArenaSurface (BrokerSurface) ────────────────────────────────────────

/// Map a venue asset string (`"BTC"`, `"BTC/USD"`) to the bare Hyperliquid coin
/// ticker. Failures carry "unsupported asset" so
/// [`crate::broker_surface::classify_broker_error_message`] lands on
/// `UnsupportedAsset`.
fn hl_coin_for(asset: &str) -> anyhow::Result<String> {
    let sym = AssetSymbol::from_str(asset)
        .map_err(|e| anyhow::anyhow!("degen arena unsupported asset '{asset}': {e}"))?;
    Ok(sym.to_string())
}

/// Round a price to 5 significant figures (Hyperliquid's perp price precision
/// rule). Integer-valued prices are always accepted, so values ≥ 1 with no
/// fractional significance pass through.
fn round_px(px: f64) -> f64 {
    if !(px > 0.0) || !px.is_finite() {
        return px;
    }
    let digits = px.abs().log10().floor() as i32;
    let factor = 10f64.powi(4 - digits);
    (px * factor).round() / factor
}

/// Derive an HL `cloid` (128-bit, `0x` + 32 hex) from the idempotency key when
/// it is a UUID (the cycle_id). Non-UUID keys get no cloid (best-effort dedupe).
fn cloid_from(key: &str) -> Option<String> {
    uuid::Uuid::parse_str(key)
        .ok()
        .map(|u| format!("0x{:032x}", u.as_u128()))
}

/// `BrokerSurface` over Hyperliquid for the Virtuals Degen Arena venue.
pub struct DegenArenaSurface<A = ReqwestHyperliquidApi> {
    api: A,
    /// Market-order slippage in basis points (aggressive IOC pricing).
    slippage_bps: f64,
}

impl DegenArenaSurface<ReqwestHyperliquidApi> {
    /// Build from environment:
    /// - `DEGEN_HL_API_KEY` — trade-only HL agent-wallet private key (`0x…`).
    /// - `DEGEN_HL_ACCOUNT_ADDRESS` — master account address (for reads).
    /// - `DEGEN_HL_NETWORK` — `mainnet` (default) or `testnet`.
    pub fn from_env() -> Result<Self, ExecutorError> {
        let key = std::env::var("DEGEN_HL_API_KEY")
            .map_err(|_| ExecutorError::Auth("DEGEN_HL_API_KEY not set".into()))?;
        let addr = std::env::var("DEGEN_HL_ACCOUNT_ADDRESS")
            .map_err(|_| ExecutorError::Auth("DEGEN_HL_ACCOUNT_ADDRESS not set".into()))?;
        let network = std::env::var("DEGEN_HL_NETWORK").unwrap_or_else(|_| "mainnet".into());
        Self::from_credentials(&key, &addr, &network)
    }

    /// Build from explicit credentials (the engine resolves these store-then-env
    /// via `resolve_degen_arena_credentials`, so UI-deployed keys drive the live
    /// path identically to env vars). `network` containing "testnet" selects the
    /// testnet host; anything else is mainnet.
    pub fn from_credentials(
        api_key: &str,
        account_address: &str,
        network: &str,
    ) -> Result<Self, ExecutorError> {
        let is_mainnet = !network.to_ascii_lowercase().contains("testnet");
        let base_url = if is_mainnet {
            HL_MAINNET_BASE
        } else {
            HL_TESTNET_BASE
        };
        let signer = api_key
            .trim()
            .parse::<PrivateKeySigner>()
            .map_err(|e| ExecutorError::Auth(format!("DEGEN_HL_API_KEY invalid: {e}")))?;
        let address = Address::from_str(account_address.trim())
            .map_err(|e| ExecutorError::Auth(format!("DEGEN_HL_ACCOUNT_ADDRESS invalid: {e}")))?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ExecutorError::Network(format!("hl reqwest client: {e}")))?;
        Ok(Self {
            api: ReqwestHyperliquidApi::new(http, base_url, address, signer, is_mainnet),
            slippage_bps: DEFAULT_SLIPPAGE_BPS,
        })
    }
}

impl<A: HyperliquidApi> DegenArenaSurface<A> {
    /// Build from any [`HyperliquidApi`] impl. Used by tests with mocks.
    pub fn with_api(api: A) -> Self {
        Self {
            api,
            slippage_bps: DEFAULT_SLIPPAGE_BPS,
        }
    }
}

#[async_trait]
impl<A: HyperliquidApi + 'static> BrokerSurface for DegenArenaSurface<A> {
    async fn submit_order(&self, req: OrderRequest) -> anyhow::Result<OrderConfirmation> {
        let coin = hl_coin_for(&req.asset)?;
        if !(req.size > 0.0) {
            anyhow::bail!(
                "degen arena order amount is too small: size {} for {} (min order size)",
                req.size,
                req.asset
            );
        }
        let anchor = req.reference_price_usd;
        if !(anchor > 0.0) || !anchor.is_finite() {
            anyhow::bail!(
                "degen arena order missing positive reference_price_usd for {}",
                req.asset
            );
        }
        let is_buy = matches!(req.side, Side::Buy);
        // "Market" = aggressive IOC limit: price through the book by slippage.
        let slip = self.slippage_bps / 10_000.0;
        let px = round_px(if is_buy {
            anchor * (1.0 + slip)
        } else {
            anchor * (1.0 - slip)
        });

        let ack = self
            .api
            .place_order(HlOrderReq {
                coin: coin.clone(),
                is_buy,
                px,
                sz: req.size,
                reduce_only: false,
                cloid: cloid_from(&req.idempotency_key),
                trigger: None,
            })
            .await
            .map_err(|e| anyhow::anyhow!("degen arena place_order: {e}"))?;

        // Best-effort reduce-only TP/SL bracket legs (HL market trigger orders),
        // derived from the fill (or reference) anchor. Direction-aware: a long
        // takes profit above / stops below; a short is inverted. Bracket
        // failures never fail the entry — same policy as Orderly/Byreal.
        let bracket_sz = if ack.filled_sz > 0.0 {
            ack.filled_sz
        } else {
            req.size
        };
        let fill_anchor = ack.avg_px.filter(|p| *p > 0.0 && p.is_finite()).unwrap_or(anchor);
        let dir = if is_buy { 1.0 } else { -1.0 };
        let close_is_buy = !is_buy;
        for (pct, tpsl, sign) in [
            (req.take_profit_pct, "tp", 1.0_f64),
            (req.stop_loss_pct, "sl", -1.0_f64),
        ] {
            let Some(pct) = pct.filter(|p| *p > 0.0) else {
                continue;
            };
            let trigger_px = round_px(fill_anchor * (1.0 + dir * sign * pct as f64 / 100.0));
            // Marketable cap past the trigger in the close direction.
            let order_px = round_px(if close_is_buy {
                trigger_px * (1.0 + slip)
            } else {
                trigger_px * (1.0 - slip)
            });
            if let Err(e) = self
                .api
                .place_order(HlOrderReq {
                    coin: coin.clone(),
                    is_buy: close_is_buy,
                    px: order_px,
                    sz: bracket_sz,
                    reduce_only: true,
                    cloid: None,
                    trigger: Some(HlTrigger {
                        trigger_px,
                        tpsl: tpsl.into(),
                    }),
                })
                .await
            {
                tracing::warn!(
                    target: "xvision::degen",
                    asset = %req.asset,
                    tpsl,
                    "degen arena {tpsl} bracket leg failed (entry stands): {e}"
                );
            }
        }

        Ok(OrderConfirmation {
            broker_order_id: ack.oid.to_string(),
            fill_price: ack.avg_px,
            fill_size: ack.filled_sz,
            fee: None,
        })
    }

    async fn position(&self, asset: &str) -> anyhow::Result<f64> {
        let coin = hl_coin_for(asset)?;
        let positions = self
            .api
            .positions()
            .await
            .map_err(|e| anyhow::anyhow!("degen arena positions: {e}"))?;
        Ok(positions
            .iter()
            .find(|p| p.coin.eq_ignore_ascii_case(&coin))
            .map(|p| p.szi)
            .unwrap_or(0.0))
    }

    async fn balance(&self) -> anyhow::Result<f64> {
        self.api
            .account_value()
            .await
            .map_err(|e| anyhow::anyhow!("degen arena account_value: {e}"))
    }
}

// ── ReqwestHyperliquidApi network tests (mockito) ────────────────────────────

#[cfg(test)]
mod reqwest_tests {
    use super::*;
    use mockito::{Matcher, Server};

    /// Test-only private key (HL Python SDK test vector key).
    const TEST_KEY: &str = "0x0123456789012345678901234567890123456789012345678901234567890123";
    /// Any well-formed 20-byte hex address works for reads.
    const TEST_ADDR: &str = "0x1234567890123456789012345678901234567890";

    fn make_api(server: &Server) -> ReqwestHyperliquidApi {
        let signer = TEST_KEY.parse::<PrivateKeySigner>().unwrap();
        let address = TEST_ADDR.parse::<Address>().unwrap();
        let http = reqwest::Client::new();
        ReqwestHyperliquidApi::new(http, server.url(), address, signer, false)
    }

    /// Minimal `/info?type=meta` stub with a universe containing `BTC` at index 0.
    async fn stub_meta(server: &mut Server) -> mockito::Mock {
        server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJsonString(r#"{"type":"meta"}"#.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"universe":[{"name":"BTC"},{"name":"ETH"},{"name":"SOL"}]}"#)
            .create_async()
            .await
    }

    /// Minimal `/info?type=clearinghouseState` stub with one open position.
    async fn stub_clearinghouse(server: &mut Server, body: &str) -> mockito::Mock {
        server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJsonString(
                r#"{"type":"clearinghouseState"}"#.into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(body.to_string())
            .create_async()
            .await
    }

    // ─── Task 1: place_order happy path ─────────────────────────────────────

    #[tokio::test]
    async fn place_order_happy_path_filled() {
        let mut server = Server::new_async().await;

        let _meta = stub_meta(&mut server).await;

        let _exchange = server
            .mock("POST", "/exchange")
            .match_body(Matcher::AllOf(vec![
                // action.type == "order"
                Matcher::Regex(r#""type"\s*:\s*"order""#.into()),
                // signature object with r, s, v fields
                Matcher::Regex(r#""signature"\s*:"#.into()),
                Matcher::Regex(r#""r"\s*:\s*"0x"#.into()),
                Matcher::Regex(r#""s"\s*:\s*"0x"#.into()),
                Matcher::Regex(r#""v"\s*:"#.into()),
            ]))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "status": "ok",
                "response": {
                    "type": "order",
                    "data": {
                        "statuses": [{
                            "filled": {
                                "oid": 99,
                                "avgPx": "70350.0",
                                "totalSz": "0.05"
                            }
                        }]
                    }
                }
            }"#,
            )
            .create_async()
            .await;

        let api = make_api(&server);
        let ack = api
            .place_order(HlOrderReq {
                coin: "BTC".into(),
                is_buy: true,
                px: 70_350.0,
                sz: 0.05,
                reduce_only: false,
                cloid: None,
                trigger: None,
            })
            .await
            .expect("place_order must succeed");

        assert_eq!(ack.oid, 99);
        assert_eq!(ack.status, "filled");
        assert_eq!(ack.avg_px, Some(70_350.0));
        assert_eq!(ack.filled_sz, 0.05);
    }

    // ─── Task 2: place_order venue error ────────────────────────────────────

    #[tokio::test]
    async fn place_order_venue_error_propagates_message() {
        let mut server = Server::new_async().await;

        let _meta = stub_meta(&mut server).await;

        let _exchange = server
            .mock("POST", "/exchange")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
                "status": "ok",
                "response": {
                    "data": {
                        "statuses": [{
                            "error": "Order must have minimum value of $10"
                        }]
                    }
                }
            }"#,
            )
            .create_async()
            .await;

        let api = make_api(&server);
        let err = api
            .place_order(HlOrderReq {
                coin: "BTC".into(),
                is_buy: true,
                px: 0.001,
                sz: 0.000001,
                reduce_only: false,
                cloid: None,
                trigger: None,
            })
            .await
            .expect_err("venue error must propagate");

        let msg = format!("{err}");
        assert!(
            msg.contains("minimum value"),
            "error must contain venue text; got: {msg}"
        );
    }

    // ─── Task 3: unsupported asset + meta caching ────────────────────────────

    #[tokio::test]
    async fn place_order_unsupported_asset_errors() {
        let mut server = Server::new_async().await;

        // Meta has only BTC — DOGE is absent.
        let _meta = server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJsonString(r#"{"type":"meta"}"#.into()))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"universe":[{"name":"BTC"}]}"#)
            .expect(1) // must be called exactly once even with two place_order calls
            .create_async()
            .await;

        let api = make_api(&server);

        let err = api
            .place_order(HlOrderReq {
                coin: "DOGE".into(),
                is_buy: true,
                px: 0.1,
                sz: 10.0,
                reduce_only: false,
                cloid: None,
                trigger: None,
            })
            .await
            .expect_err("unsupported asset must error");

        let msg = format!("{err}");
        assert!(
            msg.contains("unsupported asset"),
            "error must mention unsupported asset; got: {msg}"
        );

        // Second call with same unsupported coin — meta MUST NOT be re-fetched.
        // The expect(1) on the meta mock above will fail if we hit /info again.
        let err2 = api
            .place_order(HlOrderReq {
                coin: "DOGE".into(),
                is_buy: false,
                px: 0.1,
                sz: 10.0,
                reduce_only: false,
                cloid: None,
                trigger: None,
            })
            .await
            .expect_err("second call must also error without re-fetching meta");
        let msg2 = format!("{err2}");
        assert!(
            msg2.contains("unsupported asset"),
            "second error must mention unsupported asset; got: {msg2}"
        );

        // Verify: mockito asserts the expect(1) constraint is met.
        _meta.assert_async().await;
    }

    // ─── Task 4: positions ───────────────────────────────────────────────────

    #[tokio::test]
    async fn positions_returns_signed_szi() {
        let mut server = Server::new_async().await;

        let _cs = stub_clearinghouse(
            &mut server,
            r#"{
                "assetPositions": [
                    {"position": {"coin": "BTC", "szi": "-0.12"}},
                    {"position": {"coin": "ETH", "szi": "3.0"}}
                ],
                "marginSummary": {"accountValue": "5000.0"}
            }"#,
        )
        .await;

        let api = make_api(&server);
        let ps = api.positions().await.expect("positions must succeed");

        assert_eq!(ps.len(), 2);
        assert_eq!(ps[0].coin, "BTC");
        assert!((ps[0].szi - (-0.12)).abs() < 1e-9, "BTC szi must be -0.12");
        assert_eq!(ps[1].coin, "ETH");
        assert!((ps[1].szi - 3.0).abs() < 1e-9, "ETH szi must be 3.0");
    }

    #[tokio::test]
    async fn positions_empty_returns_empty_vec() {
        let mut server = Server::new_async().await;

        let _cs = stub_clearinghouse(
            &mut server,
            r#"{"assetPositions": [], "marginSummary": {"accountValue": "0.0"}}"#,
        )
        .await;

        let api = make_api(&server);
        let ps = api.positions().await.expect("positions must succeed");
        assert!(ps.is_empty(), "empty assetPositions must yield empty vec");
    }

    // ─── Task 5: account_value ───────────────────────────────────────────────

    #[tokio::test]
    async fn account_value_parses_margin_summary() {
        let mut server = Server::new_async().await;

        let _cs = stub_clearinghouse(
            &mut server,
            r#"{"assetPositions": [], "marginSummary": {"accountValue": "12345.67"}}"#,
        )
        .await;

        let api = make_api(&server);
        let v = api.account_value().await.expect("account_value must succeed");
        assert!(
            (v - 12_345.67).abs() < 1e-6,
            "account_value must parse to 12345.67; got {v}"
        );
    }

    #[tokio::test]
    async fn account_value_falls_back_to_spot_usdc_on_unified() {
        // Unified account: perp marginSummary reads 0 (not meaningful), and the
        // real USDC collateral lives in the spot clearinghouse state. This is
        // the exact case the testnet smoke hit (E553: perp 0, spot 999).
        let mut server = Server::new_async().await;
        let _perp = stub_clearinghouse(
            &mut server,
            r#"{"assetPositions": [], "marginSummary": {"accountValue": "0.0"}}"#,
        )
        .await;
        let _spot = server
            .mock("POST", "/info")
            .match_body(Matcher::PartialJsonString(
                r#"{"type":"spotClearinghouseState"}"#.into(),
            ))
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"balances":[{"coin":"USDC","total":"999.0"},{"coin":"HYPE","total":"3.0"}]}"#)
            .create_async()
            .await;

        let api = make_api(&server);
        let v = api.account_value().await.expect("account_value must succeed");
        assert!(
            (v - 999.0).abs() < 1e-6,
            "unified account must fall back to spot USDC 999; got {v}"
        );
    }

    // ─── Task 6: network failure ─────────────────────────────────────────────

    #[tokio::test]
    async fn network_failure_on_info_surfaces_network_error() {
        let mut server = Server::new_async().await;

        // Return HTTP 500 for the /info (meta) call.
        let _meta = server
            .mock("POST", "/info")
            .with_status(500)
            .with_header("content-type", "application/json")
            .with_body(r#"not json"#)
            .create_async()
            .await;

        let api = make_api(&server);
        let err = api
            .place_order(HlOrderReq {
                coin: "BTC".into(),
                is_buy: true,
                px: 70_000.0,
                sz: 0.01,
                reduce_only: false,
                cloid: None,
                trigger: None,
            })
            .await
            .expect_err("500 response must propagate as error");

        // The body decode will fail with a Network error, or meta parse fails
        // as Internal. Either way it must not be a clean success.
        let msg = format!("{err}");
        assert!(!msg.is_empty(), "must have a non-empty error message; got: {msg}");
        // Verify it's a Network or Internal variant (not Rejected or Auth).
        assert!(
            matches!(err, ExecutorError::Network(_) | ExecutorError::Internal(_)),
            "expected Network or Internal, got: {err:?}"
        );
    }
}

// ── MockHyperliquidApi + surface tests ───────────────────────────────────────

/// Deterministic in-memory [`HyperliquidApi`] for tests. Records every
/// `place_order` call; returns configured acks / positions / equity.
#[derive(Default)]
pub struct MockHyperliquidApi {
    pub placed: Mutex<Vec<HlOrderReq>>,
    pub ack: Option<HlOrderAck>,
    pub positions: Vec<HlPosition>,
    pub equity: f64,
    pub place_err: Option<String>,
}

#[async_trait]
impl HyperliquidApi for MockHyperliquidApi {
    async fn place_order(&self, order: HlOrderReq) -> Result<HlOrderAck, ExecutorError> {
        if let Some(e) = &self.place_err {
            return Err(ExecutorError::Rejected(e.clone()));
        }
        let (px, sz) = (order.px, order.sz);
        self.placed.lock().unwrap().push(order);
        Ok(self.ack.clone().unwrap_or(HlOrderAck {
            oid: 1,
            status: "filled".into(),
            avg_px: Some(px),
            filled_sz: sz,
        }))
    }

    async fn positions(&self) -> Result<Vec<HlPosition>, ExecutorError> {
        Ok(self.positions.clone())
    }

    async fn account_value(&self) -> Result<f64, ExecutorError> {
        Ok(self.equity)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker_surface::{classify_broker_error_message, BrokerErrorClass};

    fn buy_req(asset: &str, size: f64) -> OrderRequest {
        OrderRequest {
            asset: asset.into(),
            side: Side::Buy,
            size,
            reference_price_usd: 70_000.0,
            stop_loss_pct: None,
            take_profit_pct: None,
            idempotency_key: "cycle-abc".into(),
        }
    }

    #[test]
    fn hl_coin_for_strips_quote() {
        assert_eq!(hl_coin_for("BTC").unwrap(), "BTC");
        assert_eq!(hl_coin_for("BTC/USD").unwrap(), "BTC");
        assert_eq!(hl_coin_for("ETH/USD").unwrap(), "ETH");
        assert!(hl_coin_for("not a symbol!!").is_err());
    }

    #[test]
    fn round_px_keeps_five_sig_figs() {
        assert_eq!(round_px(73_500.0), 73_500.0);
        assert!((round_px(1670.123) - 1670.1).abs() < 1e-6);
        assert_eq!(round_px(0.0), 0.0);
    }

    #[test]
    fn cloid_from_uuid_only() {
        assert!(cloid_from("cycle-abc").is_none());
        let c = cloid_from("123e4567-e89b-12d3-a456-426614174000").unwrap();
        assert!(c.starts_with("0x") && c.len() == 34);
    }

    #[tokio::test]
    async fn submit_buy_places_one_market_order_with_upward_slippage() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi::default());
        let conf = surface.submit_order(buy_req("BTC/USD", 0.05)).await.unwrap();

        let placed = surface.api.placed.lock().unwrap().clone();
        assert_eq!(placed.len(), 1, "exactly one perp_trade");
        assert_eq!(placed[0].coin, "BTC");
        assert!(placed[0].is_buy);
        assert_eq!(placed[0].sz, 0.05);
        assert!(placed[0].px > 70_000.0, "buy prices through the book upward");
        assert!(!placed[0].reduce_only);
        assert_eq!(conf.fill_size, 0.05);
    }

    #[tokio::test]
    async fn submit_sell_uses_downward_slippage() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi::default());
        let mut req = buy_req("BTC/USD", 0.05);
        req.side = Side::Sell;
        surface.submit_order(req).await.unwrap();
        let placed = surface.api.placed.lock().unwrap().clone();
        assert!(!placed[0].is_buy);
        assert!(placed[0].px < 70_000.0, "sell prices through the book downward");
    }

    #[tokio::test]
    async fn submit_buy_with_brackets_places_entry_plus_reduce_only_tp_sl() {
        // Fill at a clean 70k (mock would otherwise fill at the aggressive cap).
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi {
            ack: Some(HlOrderAck {
                oid: 1,
                status: "filled".into(),
                avg_px: Some(70_000.0),
                filled_sz: 0.05,
            }),
            ..Default::default()
        });
        let req = OrderRequest {
            asset: "BTC/USD".into(),
            side: Side::Buy,
            size: 0.05,
            reference_price_usd: 70_000.0,
            stop_loss_pct: Some(2.0),
            take_profit_pct: Some(5.0),
            idempotency_key: "cycle-xyz".into(),
        };
        surface.submit_order(req).await.unwrap();

        let placed = surface.api.placed.lock().unwrap().clone();
        assert_eq!(placed.len(), 3, "entry + TP + SL legs");

        // Entry: market IOC long, no trigger, not reduce-only.
        assert!(placed[0].is_buy && placed[0].trigger.is_none() && !placed[0].reduce_only);

        let leg = |tpsl: &str| {
            placed
                .iter()
                .find(|o| o.trigger.as_ref().map(|t| t.tpsl.as_str()) == Some(tpsl))
                .unwrap_or_else(|| panic!("missing {tpsl} leg"))
                .clone()
        };
        let tp = leg("tp");
        let sl = leg("sl");
        for b in [&tp, &sl] {
            assert!(!b.is_buy, "close a long with a sell");
            assert!(b.reduce_only, "bracket legs are reduce-only");
            assert!((b.sz - 0.05).abs() < 1e-9, "bracket size = filled size");
        }
        // Long: TP above the fill anchor, SL below.
        assert!(
            tp.trigger.unwrap().trigger_px > 70_000.0,
            "TP triggers above for a long"
        );
        assert!(
            sl.trigger.unwrap().trigger_px < 70_000.0,
            "SL triggers below for a long"
        );
    }

    #[tokio::test]
    async fn submit_zero_size_is_min_order_size_and_places_nothing() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi::default());
        let err = surface
            .submit_order(buy_req("BTC/USD", 0.0))
            .await
            .expect_err("zero size must error");
        assert_eq!(
            classify_broker_error_message(&format!("{err:#}")),
            BrokerErrorClass::MinOrderSize
        );
        assert!(surface.api.placed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn submit_unsupported_asset_errors_before_any_call() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi::default());
        let err = surface
            .submit_order(buy_req("not a symbol!!", 0.05))
            .await
            .expect_err("bad asset must error");
        assert_eq!(
            classify_broker_error_message(&format!("{err:#}")),
            BrokerErrorClass::UnsupportedAsset
        );
        assert!(surface.api.placed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn submit_missing_reference_price_errors() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi::default());
        let mut req = buy_req("BTC/USD", 0.05);
        req.reference_price_usd = 0.0;
        assert!(surface.submit_order(req).await.is_err());
        assert!(surface.api.placed.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn submit_venue_rejection_classifies_recoverable() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi {
            place_err: Some("insufficient balance for margin".into()),
            ..Default::default()
        });
        let err = surface
            .submit_order(buy_req("BTC/USD", 0.05))
            .await
            .expect_err("rejection must propagate");
        assert_eq!(
            classify_broker_error_message(&format!("{err:#}")),
            BrokerErrorClass::InsufficientFunds
        );
    }

    #[tokio::test]
    async fn position_returns_signed_qty_or_zero() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi {
            positions: vec![HlPosition {
                coin: "BTC".into(),
                szi: -0.25,
            }],
            ..Default::default()
        });
        assert_eq!(surface.position("BTC/USD").await.unwrap(), -0.25);
        assert_eq!(surface.position("ETH/USD").await.unwrap(), 0.0);
    }

    #[tokio::test]
    async fn balance_returns_account_value() {
        let surface = DegenArenaSurface::with_api(MockHyperliquidApi {
            equity: 1234.5,
            ..Default::default()
        });
        assert_eq!(surface.balance().await.unwrap(), 1234.5);
    }

    #[test]
    fn parse_order_response_filled() {
        let v = serde_json::json!({
            "status": "ok",
            "response": { "type": "order", "data": { "statuses": [
                { "filled": { "totalSz": "0.05", "avgPx": "70100.0", "oid": 42 } }
            ] } }
        });
        let ack = parse_order_response(&v).unwrap();
        assert_eq!(ack.oid, 42);
        assert_eq!(ack.avg_px, Some(70100.0));
        assert_eq!(ack.filled_sz, 0.05);
        assert_eq!(ack.status, "filled");
    }

    #[test]
    fn parse_order_response_resting() {
        let v = serde_json::json!({
            "status": "ok",
            "response": { "data": { "statuses": [ { "resting": { "oid": 7 } } ] } }
        });
        let ack = parse_order_response(&v).unwrap();
        assert_eq!(ack.oid, 7);
        assert_eq!(ack.status, "resting");
        assert_eq!(ack.filled_sz, 0.0);
    }

    #[test]
    fn parse_order_response_per_order_error() {
        let v = serde_json::json!({
            "status": "ok",
            "response": { "data": { "statuses": [ { "error": "Order must have minimum value of $10" } ] } }
        });
        let err = parse_order_response(&v).unwrap_err();
        assert!(format!("{err}").contains("minimum value"));
    }

    #[test]
    fn parse_order_response_top_level_err() {
        let v = serde_json::json!({ "status": "err", "response": "Insufficient margin to place order" });
        let err = parse_order_response(&v).unwrap_err();
        assert_eq!(
            classify_broker_error_message(&format!("{err}")),
            BrokerErrorClass::InsufficientFunds
        );
    }

    #[test]
    fn parse_positions_and_account_value() {
        let v = serde_json::json!({
            "assetPositions": [
                { "position": { "coin": "BTC", "szi": "-0.25", "entryPx": "70000" } },
                { "position": { "coin": "ETH", "szi": "1.5" } }
            ],
            "marginSummary": { "accountValue": "950.0" }
        });
        let ps = parse_positions(&v);
        assert_eq!(ps.len(), 2);
        assert_eq!(
            ps[0],
            HlPosition {
                coin: "BTC".into(),
                szi: -0.25
            }
        );
        assert_eq!(parse_account_value(&v), 950.0);
    }

    #[test]
    fn parse_positions_empty_when_missing() {
        assert!(parse_positions(&serde_json::json!({})).is_empty());
        assert_eq!(parse_account_value(&serde_json::json!({})), 0.0);
    }

    #[test]
    fn parse_spot_usdc_sums_usdc_only() {
        let v = serde_json::json!({
            "balances": [
                {"coin": "USDC", "total": "999.0"},
                {"coin": "HYPE", "total": "3.0"}
            ]
        });
        assert_eq!(parse_spot_usdc(&v), 999.0);
        assert_eq!(parse_spot_usdc(&serde_json::json!({})), 0.0);
        assert_eq!(parse_spot_usdc(&serde_json::json!({"balances": []})), 0.0);
    }
}
