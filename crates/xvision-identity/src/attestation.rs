//! AM4 bridge — connect the off-chain attestation verdict to the on-chain
//! ERC-8004 reputation post (Task C6, spec §3.6).
//!
//! ## Two attestation systems, bridged (the AM4 decision)
//!
//! The codebase has two previously-unconnected attestation systems:
//!
//! 1. **Off-chain Ed25519 `attest()`** (`xvision-engine::eval::attestation`,
//!    persisted to `eval_attestations`). A locally-signed record of a run's
//!    metrics.
//! 2. **On-chain ERC-8004 `giveFeedback`** via
//!    [`crate::IdentityClient::post_reputation`] (real alloy).
//!
//! **C6 decision (implemented here):** KEEP the off-chain Ed25519 attestation
//! as a local *pre-anchor* signed record (it is **not** retired), and ADD the
//! on-chain reputation post for the §3.6 attestation. The end-to-end flow is:
//!
//! ```text
//! 20-trade trigger  →  compute §3.6 verdict (xvision-engine)
//!                   →  record off-chain Ed25519 pre-anchor (always; engine side)
//!                   →  IF listing + license + configured registries:
//!                          submit on-chain via post_reputation
//!                          (tag1=tradingYield, tag2=month, value=verdict)
//! ```
//!
//! This module owns the **on-chain half**: the license gate and the gated
//! submission. The off-chain pre-anchor stays in `xvision-engine` (which has
//! the signing key and the `eval_attestations` store and no alloy dependency).
//!
//! ## License gate (spec §3.6)
//!
//! Only ERC-1155 license holders may submit feedback. This is enforced
//! **on-chain** by C1's ReputationRegistry. The engine ALSO checks the
//! operator holds a license *before* submitting (via
//! [`IdentityClient::holds_license`]) to avoid a guaranteed on-chain revert
//! and wasted gas. When no license is held, submission is skipped and the
//! reason recorded.
//!
//! ## Deploy-gated boundary
//!
//! [`crate::RegistryAddresses::mantle_testnet`] returns `Some` only when the
//! `MANTLE_TESTNET_{IDENTITY,REPUTATION}_REGISTRY` env vars are set. Pre-deploy
//! they are absent, so [`decide_submission`] returns
//! [`AttestationDecision::NotConfigured`] and the on-chain path no-ops
//! cleanly. The off-chain pre-anchor (engine side) always happens regardless.

use alloy::primitives::Address;
use alloy::signers::local::PrivateKeySigner;
use chrono::Utc;
use uuid::Uuid;

use crate::client::{IdentityClient, IdentityError, RegistryAddresses, TokenId, TxHash};
use crate::contracts::ILicenseToken;
use crate::manifest::TradeOutcome;

/// Platform-fixed ERC-8004 `tag1` for §3.6 trading-yield attestations.
///
/// Mirrors `xvision_engine::eval::attestation_verdict::TAG1_TRADING_YIELD`;
/// duplicated here so the identity crate has no engine dependency.
pub const TAG1_TRADING_YIELD: &str = "tradingYield";
/// Platform-fixed ERC-8004 `tag2` (rolling-window approximation).
pub const TAG2_MONTH: &str = "month";

/// The pre-submission gating decision (pure, chain-free, exhaustively tested).
///
/// Computed from "are the registries configured?" and "does the operator hold
/// a license?". Only [`AttestationDecision::Submit`] leads to an on-chain
/// `giveFeedback`; the other variants are recorded reasons for *not*
/// submitting (the off-chain pre-anchor still happened upstream).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttestationDecision {
    /// All gates pass — submit on-chain with this `tradingYield` value.
    Submit { value: u8 },
    /// Registries are not configured (pre-deploy). No-op cleanly.
    NotConfigured,
    /// Operator does not hold an ERC-1155 license for the listing. Skip to
    /// avoid a guaranteed on-chain revert.
    NoLicense,
}

/// Pure gating logic: decide whether to submit the §3.6 attestation on-chain.
///
/// - `registries`: `Some` when [`RegistryAddresses::mantle_testnet`] (or a
///   custom config) yielded addresses; `None` pre-deploy.
/// - `holds_license`: whether `balanceOf(operator, tokenId) > 0` for the
///   listing's ERC-1155 license token.
/// - `verdict_value`: the §3.6 `tradingYield` value (100 | 50 | 0) computed by
///   `xvision-engine`.
///
/// Order of gates: registries first (cheapest, deploy-gated), then license.
/// This is a pure function so the gating decision is testable without a chain.
pub fn decide_submission(
    registries: Option<&RegistryAddresses>,
    holds_license: bool,
    verdict_value: u8,
) -> AttestationDecision {
    if registries.is_none() {
        return AttestationDecision::NotConfigured;
    }
    if !holds_license {
        return AttestationDecision::NoLicense;
    }
    AttestationDecision::Submit {
        value: verdict_value,
    }
}

/// Build the [`TradeOutcome`] payload that carries a §3.6 attestation verdict
/// into [`IdentityClient::post_reputation_raw`].
///
/// The §3.6 attestation is a *verdict* SCORE (100/50/0), not a single trade.
/// The `realized_pnl_usd` field carries the verdict value purely so the JSON
/// payload (serialised into `feedbackURI`) records it; the on-chain numeric
/// `value` is set explicitly by [`IdentityClient::submit_attestation`] via the
/// RAW path as `value = verdict_value`, `valueDecimals = 0` — NOT through the
/// PnL `*1e6` / `valueDecimals = 6` encoding. `action` is fixed to `"attest"`
/// to distinguish attestation posts from per-trade outcome posts. `cycle_id`
/// keys the post to the deployment's attestation cycle.
pub fn build_attestation_outcome(cycle_id: Uuid, verdict_value: u8) -> TradeOutcome {
    TradeOutcome {
        cycle_id,
        realized_pnl_usd: verdict_value as f64,
        action: "attest".to_string(),
        closed_at: Utc::now(),
    }
}

impl IdentityClient {
    /// Check whether `operator` holds at least one ERC-1155 license token for
    /// the listing identified by `listing_token_id`.
    ///
    /// Reads `balanceOf(operator, listing_token_id) > 0` on the
    /// `ILicenseToken` contract. The engine calls this BEFORE submitting an
    /// attestation so it can skip the on-chain `giveFeedback` (which C1's
    /// ReputationRegistry would revert) when no license is held.
    pub async fn holds_license(
        &self,
        license_token: Address,
        operator: Address,
        listing_token_id: alloy::primitives::U256,
    ) -> Result<bool, IdentityError> {
        let contract = ILicenseToken::new(license_token, self.provider_ref());
        let balance = contract
            .balanceOf(operator, listing_token_id)
            .call()
            .await
            .map_err(|e| IdentityError::Contract(e.to_string()))?;
        Ok(balance > alloy::primitives::U256::ZERO)
    }

    /// End-to-end on-chain half of the §3.6 attestation (AM4 bridge).
    ///
    /// Given an already-computed verdict value and the operator's license
    /// status, decides whether to submit (via [`decide_submission`]) and, if
    /// so, posts the verdict on-chain via [`Self::post_reputation_raw`] with
    /// `tag1=tradingYield`, `tag2=month`, `value=verdict_value`, and
    /// `valueDecimals=0` (the verdict is an integer score, not dollar PnL).
    ///
    /// Returns the [`AttestationDecision`] plus the tx hash when submitted.
    /// Callers should have ALREADY recorded the off-chain Ed25519 pre-anchor
    /// (engine side) before calling this; this method only governs the
    /// on-chain post.
    pub async fn submit_attestation(
        &self,
        agent: TokenId,
        cycle_id: Uuid,
        verdict_value: u8,
        holds_license: bool,
        signer: &PrivateKeySigner,
    ) -> Result<(AttestationDecision, Option<TxHash>), IdentityError> {
        // Registries are always present here: an `IdentityClient` embeds resolved
        // addresses at construction. The pre-deploy no-op (`NotConfigured`) is
        // enforced UPSTREAM — the engine only builds/calls the client when
        // `RegistryAddresses::mantle_testnet()` is `Some` — so inside this method
        // the gate collapses to license-only. The `NotConfigured` arm is still
        // exercised by `decide_submission`'s engine-side callers + unit tests.
        let decision = decide_submission(Some(self.addresses_ref()), holds_license, verdict_value);
        match decision {
            AttestationDecision::Submit { value } => {
                let outcome = build_attestation_outcome(cycle_id, value);
                // A §3.6 verdict is an integer SCORE (100|50|0), not a dollar
                // amount, so it must be posted via the RAW feedback path with
                // `value = verdict_value` and `value_decimals = 0` — NOT the PnL
                // path (which would scale by 1e6 and tag `valueDecimals = 6`,
                // landing 100_000_000 @ 6 decimals on-chain). The outcome JSON is
                // still carried in `feedbackURI` for context/cycle_id recovery.
                let tx = self
                    .post_reputation_raw(
                        agent,
                        cycle_id,
                        outcome,
                        i128::from(value),
                        0u8,
                        TAG1_TRADING_YIELD,
                        TAG2_MONTH,
                        signer,
                    )
                    .await?;
                Ok((AttestationDecision::Submit { value }, Some(tx)))
            }
            other => Ok((other, None)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- decide_submission: the deploy/license gate ----------------------

    fn addrs() -> RegistryAddresses {
        RegistryAddresses::custom(Address::from([0x11u8; 20]), Address::from([0x22u8; 20]))
    }

    #[test]
    fn not_configured_when_registries_absent() {
        // Pre-deploy: mantle_testnet() is None.
        assert_eq!(
            decide_submission(None, true, 100),
            AttestationDecision::NotConfigured
        );
        // Even without a license, the registry gate trips first.
        assert_eq!(
            decide_submission(None, false, 0),
            AttestationDecision::NotConfigured
        );
    }

    #[test]
    fn no_license_when_configured_but_no_balance() {
        let a = addrs();
        assert_eq!(
            decide_submission(Some(&a), false, 100),
            AttestationDecision::NoLicense
        );
    }

    #[test]
    fn submit_when_configured_and_licensed() {
        let a = addrs();
        assert_eq!(
            decide_submission(Some(&a), true, 100),
            AttestationDecision::Submit { value: 100 }
        );
        assert_eq!(
            decide_submission(Some(&a), true, 50),
            AttestationDecision::Submit { value: 50 }
        );
        assert_eq!(
            decide_submission(Some(&a), true, 0),
            AttestationDecision::Submit { value: 0 }
        );
    }

    #[test]
    fn registry_gate_precedes_license_gate() {
        // Documenting the gate order: no registries → NotConfigured even if
        // license were somehow known.
        assert_eq!(
            decide_submission(None, true, 50),
            AttestationDecision::NotConfigured
        );
    }

    // ---- build_attestation_outcome ---------------------------------------

    #[test]
    fn attestation_outcome_carries_verdict_value() {
        let cid = Uuid::nil();
        let o = build_attestation_outcome(cid, 100);
        assert_eq!(o.cycle_id, cid);
        assert_eq!(o.realized_pnl_usd, 100.0);
        assert_eq!(o.action, "attest");
    }

    #[test]
    fn attestation_outcome_round_trips_each_value() {
        for v in [0u8, 50, 100] {
            let o = build_attestation_outcome(Uuid::nil(), v);
            assert_eq!(o.realized_pnl_usd as u8, v);
        }
    }

    #[test]
    fn tags_match_spec() {
        assert_eq!(TAG1_TRADING_YIELD, "tradingYield");
        assert_eq!(TAG2_MONTH, "month");
    }
}
