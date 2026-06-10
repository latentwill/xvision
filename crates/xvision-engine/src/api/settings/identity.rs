//! `/api/settings/identity` — on-chain identity snapshot. v1 ships without
//! the wallet flow (it's gated behind the `xvision-identity` crate +
//! WITH_IDENTITY=1 builds). This stub surfaces "what would be configured if
//! identity were on" plus the build-time gate, so the Settings tab can
//! render meaningfully rather than falling back to the placeholder.

use std::env;
use std::time::Instant;

use serde::{Deserialize, Serialize};

use crate::api::{
    audit::{self, Outcome},
    ApiContext, ApiResult,
};

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IdentityReport {
    /// Was the build compiled with the `xvision-identity` member? In default
    /// `cargo build` (no `--workspace`) this is false; in
    /// `cargo build --workspace` or the `:identity` docker tag, true.
    pub feature_compiled_in: bool,
    /// Wallet config snapshot (env-var presence; values never returned).
    pub wallet: WalletStatus,
    /// Note explaining the v1 stance (read-only, no minting).
    pub note: String,
    /// NFT token id for the platform agent on the Mantle registry.
    /// Read from env `XVN_PLATFORM_AGENT_TOKEN_ID` (parse as u64).
    /// `None` when the env var is absent or unparseable.
    pub agent_token_id: Option<u64>,
    /// On-chain address of the identity registry contract.
    /// Read from env `XVN_IDENTITY_REGISTRY` (hex address string).
    /// `None` when the env var is absent.
    pub identity_registry: Option<String>,
    /// Tx hash of the most recent chain attestation recorded in the local
    /// store. `None` when no attestation has been posted yet or when the
    /// chain-attest feature is inactive.
    pub last_attestation_tx: Option<String>,
    /// Base URL for Mantlescan block explorer links.
    pub mantlescan_base_url: String,
}

#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WalletStatus {
    /// Mantle RPC URL configured? (env var presence).
    pub rpc_url_set: bool,
    /// Operator-controlled wallet private key configured? (env var presence
    /// only — values never leave the process).
    pub wallet_key_set: bool,
}

pub async fn get(ctx: &ApiContext) -> ApiResult<IdentityReport> {
    let started = Instant::now();

    let agent_token_id: Option<u64> = env::var("XVN_PLATFORM_AGENT_TOKEN_ID")
        .ok()
        .and_then(|s| s.parse().ok());

    let identity_registry: Option<String> = env::var("XVN_IDENTITY_REGISTRY")
        .ok()
        .filter(|v| !v.is_empty());

    // `last_attestation_tx` — the chain-attest module (T1.3) is fire-and-forget
    // and does not yet persist the tx hash back to SQLite. Querying
    // `eval_attestations` would return the off-chain Ed25519 attestation, not
    // an on-chain tx hash. Leave as None until T1.3 wires a persistence path.
    let last_attestation_tx: Option<String> = None;
    let _ = &ctx.db; // silence unused-field warning while query is deferred

    let report = IdentityReport {
        // The `WITH_IDENTITY` toggle lives at the docker / workspace-build
        // level, not at the engine-crate feature level — so from here we
        // don't have a `cfg!(feature = ...)` to read. Hardcoded `false` for
        // v1; the wallet plan will replace this with a real check.
        feature_compiled_in: false,
        wallet: WalletStatus {
            rpc_url_set: env::var("MANTLE_RPC_URL").map(|v| !v.is_empty()).unwrap_or(false),
            wallet_key_set: env::var("XVN_WALLET_KEY").map(|v| !v.is_empty()).unwrap_or(false),
        },
        note: "v1 surfaces are read-only. ERC-8004 mint, attestation, and \
               reputation flows live in the blockchain wallet plan."
            .into(),
        agent_token_id,
        identity_registry,
        last_attestation_tx,
        mantlescan_base_url: "https://sepolia.mantlescan.xyz".into(),
    };

    let _ = audit::record(
        ctx,
        "settings",
        "identity.get",
        None,
        None,
        Outcome::Ok,
        started.elapsed().as_millis() as i64,
    )
    .await;

    Ok(report)
}
