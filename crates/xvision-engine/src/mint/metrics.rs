//! Required-metric registry per capability (Phase 4.4).
//!
//! The discipline this enforces is *which* metrics a capability's holdout proof
//! must carry before a candidate can be accepted / a child minted to the
//! marketplace. The actual metric VALUES are produced by the eval harness on the
//! CLI side and persisted as scalars (see `mint::holdout`); this module only
//! owns the closed, capability-keyed *set* of metric names that must be present.
//!
//! Pure + deterministic: no DB, no I/O, no `xvision-dspy`. The set is a
//! hardcoded mirror (same discipline as `diagnostics`' optimizable-set mirror),
//! so it is trivially testable and the gate logic can consult it without a
//! round-trip.

/// The capability classes the metric registry knows about. Free-text capability
/// strings flow through the optimization store (`OptimizationRun.capability`);
/// [`required_metrics`] maps the recognized ones to their required set and
/// returns an empty slice for anything it does not recognize (an unknown
/// capability imposes no extra metric requirement — the holdout-presence gate
/// still applies).
pub const CAPABILITY_TRADER: &str = "trader";
pub const CAPABILITY_FILTER: &str = "filter";

/// Trader-capability required metrics. A trader's holdout proof must report a
/// forward-return agreement signal plus the risk-adjusted return / drawdown /
/// quality battery so a strategy is never minted on a single cherry-picked
/// number.
pub const TRADER_REQUIRED_METRICS: &[&str] = &[
    "forward_return_agreement",
    "sharpe",
    "max_drawdown",
    "profit_factor",
    "calibration",
    "action_validity",
    "selectivity",
    "net_of_cost",
];

/// Filter-capability required metrics. A filter's holdout proof must report the
/// classification battery plus the wake-rate / token-savings / false-suppression
/// operating-point metrics that justify gating the downstream pipeline.
pub const FILTER_REQUIRED_METRICS: &[&str] = &[
    "precision",
    "recall",
    "f1",
    "auroc",
    "wake_rate",
    "token_savings",
    "false_suppression",
];

/// The required-metric set for a capability key. Returns an empty slice for an
/// unrecognized capability (no extra requirement beyond holdout presence).
///
/// Matching is exact on the lowercase capability string the optimization store
/// persists (`trader`, `filter`).
pub fn required_metrics(capability: &str) -> &'static [&'static str] {
    match capability {
        CAPABILITY_TRADER => TRADER_REQUIRED_METRICS,
        CAPABILITY_FILTER => FILTER_REQUIRED_METRICS,
        _ => &[],
    }
}

/// Whether the registry recognizes this capability (i.e. imposes a required set).
pub fn is_known_capability(capability: &str) -> bool {
    !required_metrics(capability).is_empty()
}

/// The subset of [`required_metrics`] for `capability` that is NOT present in
/// `provided` (the metric names the holdout proof actually carries). Empty ⇒ the
/// proof covers every required metric. The returned names are in registry order
/// so the refusal message is stable.
pub fn missing_metrics(capability: &str, provided: &[String]) -> Vec<&'static str> {
    required_metrics(capability)
        .iter()
        .copied()
        .filter(|req| !provided.iter().any(|p| p == req))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trader_and_filter_have_disjoint_nonempty_sets() {
        assert!(!TRADER_REQUIRED_METRICS.is_empty());
        assert!(!FILTER_REQUIRED_METRICS.is_empty());
        // Spec batteries are distinct axes.
        for m in TRADER_REQUIRED_METRICS {
            assert!(!FILTER_REQUIRED_METRICS.contains(m), "{m} in both sets");
        }
    }

    #[test]
    fn required_metrics_maps_known_capabilities() {
        assert_eq!(required_metrics("trader"), TRADER_REQUIRED_METRICS);
        assert_eq!(required_metrics("filter"), FILTER_REQUIRED_METRICS);
        assert!(required_metrics("router").is_empty());
        assert!(required_metrics("").is_empty());
    }

    #[test]
    fn is_known_capability_tracks_registry() {
        assert!(is_known_capability("trader"));
        assert!(is_known_capability("filter"));
    }

    #[test]
    fn missing_metrics_reports_gaps_in_registry_order() {
        // Provide only two of the eight trader metrics.
        let provided = vec!["sharpe".to_string(), "max_drawdown".to_string()];
        let missing = missing_metrics("trader", &provided);
        assert_eq!(
            missing,
            vec![
                "forward_return_agreement",
                "profit_factor",
                "calibration",
                "action_validity",
                "selectivity",
                "net_of_cost",
            ]
        );
    }

    #[test]
    fn missing_metrics_empty_when_all_present() {
        let provided: Vec<String> = FILTER_REQUIRED_METRICS.iter().map(|s| s.to_string()).collect();
        assert!(missing_metrics("filter", &provided).is_empty());
    }

    #[test]
    fn unknown_capability_imposes_no_metric_requirement() {
        assert!(missing_metrics("router", &[]).is_empty());
    }
}
