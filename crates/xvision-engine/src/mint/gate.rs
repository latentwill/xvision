//! Pure accept-gate + marketplace-mint-gate decision logic (Phase 4.3 + 4.4).
//!
//! Everything here is a *pure function over already-fetched facts* — no DB, no
//! I/O, no `xvision-dspy`. The dashboard routes fetch the optimization run,
//! snapshot, holdout result, lineage edge, and eval proof, hand them to these
//! functions, and translate the typed refusals to HTTP. Keeping the gate pure
//! means the discipline is exhaustively testable without a server.
//!
//! ## The two gates
//!
//! 1. [`check_accept`] — may a snapshot be promoted into a child agent? Refuses
//!    (typed) when there is no holdout result UNLESS a non-empty `override_reason`
//!    is supplied (the reason is recorded by the caller). This is the
//!    holdout-presence discipline.
//!
//! 2. [`check_marketplace_mint`] — may a (already-minted) child agent be minted
//!    to MARKETPLACE metadata? Refuses (typed) without all of:
//!    (a) optimization lineage, (b) eval proof, (c) no UNWAIVED overfit warning,
//!    (d) the capability's required-metric set covered by the holdout proof.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use super::holdout::HoldoutResult;
use super::metrics::missing_metrics;

/// The facts the accept gate needs. The caller fetches these from the
/// optimization store before promoting a snapshot.
#[derive(Clone, Debug)]
pub struct AcceptInputs<'a> {
    /// The snapshot the operator wants to accept.
    pub snapshot_id: &'a str,
    /// The recorded holdout result for this snapshot, if any.
    pub holdout: Option<&'a HoldoutResult>,
    /// A non-empty operator-supplied reason to bypass the holdout-presence
    /// requirement. Recorded by the caller alongside the accept.
    pub override_reason: Option<&'a str>,
}

/// Why an accept was refused. Carries enough context for an operator-facing
/// message.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum AcceptRefusal {
    /// No holdout result was recorded for the snapshot and no override reason was
    /// supplied. The accept is refused: a candidate cannot be accepted without
    /// out-of-sample evidence.
    #[error(
        "snapshot {snapshot_id} has no holdout result; accept refused without a holdout \
         (supply an override_reason to bypass)"
    )]
    MissingHoldout { snapshot_id: String },
}

impl AcceptRefusal {
    /// Stable machine code for the refusal.
    pub fn machine_code(&self) -> &'static str {
        match self {
            AcceptRefusal::MissingHoldout { .. } => "accept_missing_holdout",
        }
    }
}

/// The accept decision: either allowed (recording whether a holdout was present
/// or the accept was overridden), or a typed refusal.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AcceptDecision {
    /// `true` when a holdout result backed the accept.
    pub holdout_present: bool,
    /// The override reason that bypassed a missing holdout, if used.
    pub override_reason: Option<String>,
    /// `true` when the backing holdout result carried an (possibly waived)
    /// overfit warning. Surfaced so the accept response can warn the operator
    /// that a marketplace mint will be blocked until waived.
    pub overfit_warning: bool,
}

/// Decide whether a snapshot may be accepted into a child agent.
///
/// * A recorded holdout result ⇒ allowed (carrying its overfit flag).
/// * No holdout result + a non-empty `override_reason` ⇒ allowed (override
///   recorded).
/// * No holdout result + no/empty `override_reason` ⇒ [`AcceptRefusal::MissingHoldout`].
pub fn check_accept(inputs: &AcceptInputs<'_>) -> Result<AcceptDecision, AcceptRefusal> {
    match inputs.holdout {
        Some(h) => Ok(AcceptDecision {
            holdout_present: true,
            override_reason: None,
            overfit_warning: h.overfit_warning,
        }),
        None => {
            let reason = inputs
                .override_reason
                .map(str::trim)
                .filter(|r| !r.is_empty());
            match reason {
                Some(r) => Ok(AcceptDecision {
                    holdout_present: false,
                    override_reason: Some(r.to_string()),
                    overfit_warning: false,
                }),
                None => Err(AcceptRefusal::MissingHoldout {
                    snapshot_id: inputs.snapshot_id.to_string(),
                }),
            }
        }
    }
}

/// A reference to an eval proof backing a marketplace mint. The engine treats it
/// as an opaque pointer (an eval run id + its scored metric) — the dashboard
/// resolves the run; the mint gate only checks PRESENCE, not contents.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvalProof {
    /// The eval run id that scored the child agent's strategy.
    pub eval_run_id: String,
    /// The metric the eval proof reports (mirrors the optimization objective).
    pub metric: String,
}

/// The facts the marketplace-mint gate needs. The caller fetches these for the
/// child agent being minted.
#[derive(Clone, Debug)]
pub struct MintInputs<'a> {
    /// The child agent id being minted to marketplace metadata.
    pub child_agent_id: &'a str,
    /// The capability the optimized slot plays (`trader`, `filter`, …). Drives
    /// the required-metric coverage check.
    pub capability: &'a str,
    /// `true` ⇒ the child has a recorded lineage edge (parent + producing run).
    pub has_lineage: bool,
    /// The eval proof backing the mint, if any.
    pub eval_proof: Option<&'a EvalProof>,
    /// The holdout result backing the optimization, if any.
    pub holdout: Option<&'a HoldoutResult>,
    /// The metric names the holdout proof actually carries (the per-capability
    /// required set is checked against this).
    pub metrics_present: &'a [String],
}

/// Why a marketplace mint was refused. Multiple deficiencies can co-exist; the
/// gate returns the first in a fixed precedence so the message is deterministic:
/// lineage → eval proof → unwaived overfit → metric coverage.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, Error)]
#[serde(tag = "code", rename_all = "snake_case")]
pub enum MintRefusal {
    /// The child agent has no optimization lineage — it was not produced by an
    /// accepted optimization run, so its provenance is unverifiable.
    #[error("child agent {child_agent_id} has no optimization lineage; marketplace mint refused")]
    MissingLineage { child_agent_id: String },
    /// No eval proof was supplied — the child's strategy was never scored, so the
    /// marketplace listing would carry no evidence.
    #[error("child agent {child_agent_id} has no eval proof; marketplace mint refused")]
    MissingEvalProof { child_agent_id: String },
    /// The backing holdout result carries an overfit warning that has not been
    /// waived. Minting is blocked until an operator records a waiver reason.
    #[error(
        "child agent {child_agent_id} has an unwaived overfit warning (ratio {ratio:?}); \
         marketplace mint blocked until waived"
    )]
    UnwaivedOverfit {
        child_agent_id: String,
        ratio: Option<f64>,
    },
    /// The holdout proof does not cover the capability's required-metric set.
    #[error(
        "child agent {child_agent_id} ({capability}) holdout proof is missing required \
         metrics {missing:?}; marketplace mint refused"
    )]
    IncompleteMetrics {
        child_agent_id: String,
        capability: String,
        missing: Vec<String>,
    },
}

impl MintRefusal {
    /// Stable machine code for the refusal.
    pub fn machine_code(&self) -> &'static str {
        match self {
            MintRefusal::MissingLineage { .. } => "mint_missing_lineage",
            MintRefusal::MissingEvalProof { .. } => "mint_missing_eval_proof",
            MintRefusal::UnwaivedOverfit { .. } => "mint_unwaived_overfit",
            MintRefusal::IncompleteMetrics { .. } => "mint_incomplete_metrics",
        }
    }
}

/// The mint-allowed verdict. Records what backed the mint so the marketplace
/// metadata can attest to provenance.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MintDecision {
    pub child_agent_id: String,
    pub capability: String,
    pub eval_run_id: String,
    /// `true` when an overfit warning was present but waived.
    pub overfit_waived: bool,
    /// The holdout snapshot id that backed the mint, if a holdout was present.
    pub holdout_snapshot_id: Option<String>,
}

/// Decide whether a child agent may be minted to marketplace metadata.
///
/// Refuses (in fixed precedence) without: (a) lineage, (b) eval proof, (c) a
/// waived-or-absent overfit warning, (d) the capability's required-metric set
/// covered by the holdout proof. A `None` holdout does NOT itself block the mint
/// here — accept already enforced holdout-presence (or recorded an override);
/// but when a holdout IS present, its overfit warning must be waived and its
/// metric coverage must be complete.
pub fn check_marketplace_mint(inputs: &MintInputs<'_>) -> Result<MintDecision, MintRefusal> {
    // (a) lineage.
    if !inputs.has_lineage {
        return Err(MintRefusal::MissingLineage {
            child_agent_id: inputs.child_agent_id.to_string(),
        });
    }

    // (b) eval proof.
    let proof = inputs.eval_proof.ok_or_else(|| MintRefusal::MissingEvalProof {
        child_agent_id: inputs.child_agent_id.to_string(),
    })?;

    // (c) no unwaived overfit warning.
    let mut overfit_waived = false;
    if let Some(h) = inputs.holdout {
        if h.overfit_warning {
            let waived = h
                .overfit_waiver_reason
                .as_deref()
                .map(str::trim)
                .filter(|r| !r.is_empty())
                .is_some();
            if !waived {
                return Err(MintRefusal::UnwaivedOverfit {
                    child_agent_id: inputs.child_agent_id.to_string(),
                    ratio: h.overfit_ratio,
                });
            }
            overfit_waived = true;
        }
    }

    // (d) required-metric coverage for the capability.
    let missing = missing_metrics(inputs.capability, inputs.metrics_present);
    if !missing.is_empty() {
        return Err(MintRefusal::IncompleteMetrics {
            child_agent_id: inputs.child_agent_id.to_string(),
            capability: inputs.capability.to_string(),
            missing: missing.into_iter().map(|s| s.to_string()).collect(),
        });
    }

    Ok(MintDecision {
        child_agent_id: inputs.child_agent_id.to_string(),
        capability: inputs.capability.to_string(),
        eval_run_id: proof.eval_run_id.clone(),
        overfit_waived,
        holdout_snapshot_id: inputs.holdout.map(|h| h.snapshot_id.clone()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holdout(overfit: bool, waived: Option<&str>) -> HoldoutResult {
        HoldoutResult {
            snapshot_id: "snap1".into(),
            run_id: "run1".into(),
            metric: "sharpe".into(),
            train_metric_value: 1.0,
            holdout_metric_value: if overfit { 0.4 } else { 0.9 },
            overfit_warning: overfit,
            overfit_ratio: Some(if overfit { 0.6 } else { 0.1 }),
            overfit_waiver_reason: waived.map(|s| s.to_string()),
            created_at: "2026-05-24T00:00:00Z".into(),
        }
    }

    fn all_trader_metrics() -> Vec<String> {
        super::super::metrics::TRADER_REQUIRED_METRICS
            .iter()
            .map(|s| s.to_string())
            .collect()
    }

    // ── accept gate ──────────────────────────────────────────────────────────

    #[test]
    fn accept_refused_without_holdout_or_override() {
        let err = check_accept(&AcceptInputs {
            snapshot_id: "snap1",
            holdout: None,
            override_reason: None,
        })
        .unwrap_err();
        assert_eq!(err.machine_code(), "accept_missing_holdout");
        match err {
            AcceptRefusal::MissingHoldout { snapshot_id } => assert_eq!(snapshot_id, "snap1"),
        }
    }

    #[test]
    fn accept_refused_with_blank_override() {
        let err = check_accept(&AcceptInputs {
            snapshot_id: "snap1",
            holdout: None,
            override_reason: Some("   "),
        })
        .unwrap_err();
        assert_eq!(err.machine_code(), "accept_missing_holdout");
    }

    #[test]
    fn accept_allowed_with_override_reason() {
        let d = check_accept(&AcceptInputs {
            snapshot_id: "snap1",
            holdout: None,
            override_reason: Some("manual review by quant lead"),
        })
        .unwrap();
        assert!(!d.holdout_present);
        assert_eq!(d.override_reason.as_deref(), Some("manual review by quant lead"));
    }

    #[test]
    fn accept_allowed_with_holdout_carries_overfit_flag() {
        let h = holdout(true, None);
        let d = check_accept(&AcceptInputs {
            snapshot_id: "snap1",
            holdout: Some(&h),
            override_reason: None,
        })
        .unwrap();
        assert!(d.holdout_present);
        assert!(d.overfit_warning);
    }

    // ── marketplace mint gate ─────────────────────────────────────────────────

    fn mint_inputs<'a>(
        has_lineage: bool,
        proof: Option<&'a EvalProof>,
        h: Option<&'a HoldoutResult>,
        metrics: &'a [String],
    ) -> MintInputs<'a> {
        MintInputs {
            child_agent_id: "child1",
            capability: "trader",
            has_lineage,
            eval_proof: proof,
            holdout: h,
            metrics_present: metrics,
        }
    }

    #[test]
    fn mint_refused_without_lineage() {
        let proof = EvalProof { eval_run_id: "ev1".into(), metric: "sharpe".into() };
        let metrics = all_trader_metrics();
        let err = check_marketplace_mint(&mint_inputs(false, Some(&proof), None, &metrics))
            .unwrap_err();
        assert_eq!(err.machine_code(), "mint_missing_lineage");
    }

    #[test]
    fn mint_refused_without_eval_proof() {
        let metrics = all_trader_metrics();
        let err =
            check_marketplace_mint(&mint_inputs(true, None, None, &metrics)).unwrap_err();
        assert_eq!(err.machine_code(), "mint_missing_eval_proof");
    }

    #[test]
    fn mint_blocked_by_unwaived_overfit() {
        let proof = EvalProof { eval_run_id: "ev1".into(), metric: "sharpe".into() };
        let h = holdout(true, None);
        let metrics = all_trader_metrics();
        let err =
            check_marketplace_mint(&mint_inputs(true, Some(&proof), Some(&h), &metrics))
                .unwrap_err();
        assert_eq!(err.machine_code(), "mint_unwaived_overfit");
    }

    #[test]
    fn mint_allowed_when_overfit_waived() {
        let proof = EvalProof { eval_run_id: "ev1".into(), metric: "sharpe".into() };
        let h = holdout(true, Some("acceptable for high-vol regime; reviewed 2026-05-24"));
        let metrics = all_trader_metrics();
        let d = check_marketplace_mint(&mint_inputs(true, Some(&proof), Some(&h), &metrics))
            .unwrap();
        assert!(d.overfit_waived);
        assert_eq!(d.eval_run_id, "ev1");
    }

    #[test]
    fn mint_refused_with_incomplete_metric_coverage() {
        let proof = EvalProof { eval_run_id: "ev1".into(), metric: "sharpe".into() };
        let h = holdout(false, None);
        let metrics = vec!["sharpe".to_string()]; // far short of the trader battery
        let err =
            check_marketplace_mint(&mint_inputs(true, Some(&proof), Some(&h), &metrics))
                .unwrap_err();
        assert_eq!(err.machine_code(), "mint_incomplete_metrics");
        match err {
            MintRefusal::IncompleteMetrics { missing, .. } => {
                assert!(missing.contains(&"max_drawdown".to_string()));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn mint_allowed_full_proof_no_overfit() {
        let proof = EvalProof { eval_run_id: "ev1".into(), metric: "sharpe".into() };
        let h = holdout(false, None);
        let metrics = all_trader_metrics();
        let d = check_marketplace_mint(&mint_inputs(true, Some(&proof), Some(&h), &metrics))
            .unwrap();
        assert!(!d.overfit_waived);
        assert_eq!(d.holdout_snapshot_id.as_deref(), Some("snap1"));
    }
}
