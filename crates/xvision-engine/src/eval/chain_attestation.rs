//! T1.3 — post-finalize on-chain attestation hook (AM4 wiring).
//!
//! Compiled only when the `chain-attest` Cargo feature is active (i.e. the
//! `WITH_IDENTITY=1` deploy image). Active at runtime only when
//! `XVN_CHAIN_ATTEST=1` plus both registry env vars are set.
//!
//! After a run finalizes, this computes the §3.6 verdict from the run's
//! Sharpe ratio and posts `giveFeedback` on Mantle Sepolia via the
//! `xvision-identity` bridge. Fire-and-forget — any error is logged and
//! swallowed; the run's success status is never affected.
//!
//! ## Seam decisions (documented for T1.3 scope)
//!
//! - `listed_sharpe = 0.0`: no on-chain listing yet for the demo. Per spec,
//!   `listed ≤ 0 && live ≥ listed → Endorses`; any non-negative-Sharpe run
//!   endorses. This is correct for a successful demo run.
//! - `holds_license = true`: the operator is the deployer (bootstrap). The
//!   on-chain license gate exists for third-party buyers; the platform agent
//!   posting about itself bypasses that gate explicitly here. When a real
//!   listing + license flow exists, wire `client.holds_license(...)` instead.
//! - `cycle_id`: derived deterministically from the ULID run_id via UUID v5
//!   so the on-chain record is always traceable back to the eval run.

use uuid::Uuid;
use xvision_identity::{client::RegistryAddresses, IdentityClient};

use crate::eval::run::Run;
use crate::eval::attestation_verdict::verdict;

const RPC_URL: &str = "https://rpc.sepolia.mantle.xyz";
const CHAIN_ID: u64 = 5003;

/// Post-finalize hook called by `api::eval::start` after a run completes.
///
/// Reads `XVN_CHAIN_ATTEST`, registry env vars, and `MANTLE_PRIVATE_KEY`.
/// No-ops (debug log) when any gate is missing.
pub async fn fire_chain_attestation(run: &Run) {
    if std::env::var("XVN_CHAIN_ATTEST").unwrap_or_default() != "1" {
        return;
    }

    let Some(addresses) = RegistryAddresses::mantle_testnet() else {
        tracing::debug!(
            run_id = %run.id,
            "chain_attestation: registry env vars absent — skipping"
        );
        return;
    };

    let key_hex = match std::env::var("MANTLE_PRIVATE_KEY") {
        Ok(k) => k,
        Err(_) => {
            tracing::warn!(
                run_id = %run.id,
                "chain_attestation: MANTLE_PRIVATE_KEY not set — skipping"
            );
            return;
        }
    };

    // Platform agent token id: env-configurable, defaults to 0 (the id minted
    // by RegisterPlatformAgent.s.sol on deploy day).
    let token_id_raw: u64 = std::env::var("XVN_PLATFORM_AGENT_TOKEN_ID")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    // §3.6 verdict from finalized Sharpe. listed_sharpe = 0.0 (see module doc).
    // MetricsSummary.sharpe is f64 (not Option<f64>), so use .map not .and_then.
    let live_sharpe = run
        .metrics
        .as_ref()
        .map(|m| m.sharpe)
        .unwrap_or(0.0);
    let v = verdict(live_sharpe, 0.0);

    // Deterministic UUID from the ULID run_id so the reputation entry is
    // permanently traceable back to this eval run.
    let cycle_id = run_id_to_uuid(&run.id);

    tracing::info!(
        run_id = %run.id,
        sharpe = live_sharpe,
        verdict = v.value,
        label = v.label.label(),
        "chain_attestation: posting reputation to Mantle Sepolia"
    );

    let client = match IdentityClient::connect(RPC_URL, addresses, CHAIN_ID).await {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(run_id = %run.id, error = %e, "chain_attestation: RPC connect failed");
            return;
        }
    };

    match client
        .submit_attestation_with_key(token_id_raw, cycle_id, v.value, true, &key_hex)
        .await
    {
        Ok((_decision, Some(tx))) => {
            tracing::info!(
                run_id = %run.id,
                verdict = v.value,
                label = v.label.label(),
                tx = %tx,
                "chain_attestation: reputation posted on-chain"
            );
        }
        Ok((decision, None)) => {
            tracing::info!(
                run_id = %run.id,
                decision = ?decision,
                "chain_attestation: submission skipped by gate"
            );
        }
        Err(e) => {
            tracing::warn!(
                run_id = %run.id,
                error = %e,
                "chain_attestation: submit_attestation failed (run unaffected)"
            );
        }
    }
}

/// Derive a deterministic UUID v5 from a ULID run id. Uses OID namespace so
/// the UUID is globally unique without requiring a Mantle-specific namespace.
fn run_id_to_uuid(run_id: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_OID, run_id.as_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_id_to_uuid_is_deterministic() {
        let id = "01JXABCDEFGHJKMNPQRSTVWXYZ";
        assert_eq!(run_id_to_uuid(id), run_id_to_uuid(id));
    }

    #[test]
    fn different_run_ids_produce_different_uuids() {
        assert_ne!(
            run_id_to_uuid("01JXAAA0000000000000000000"),
            run_id_to_uuid("01JXBBB0000000000000000000"),
        );
    }
}
