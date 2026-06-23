//! Eval-run hard limits — per-run caps that stop a runaway token burn.
//!
//! Source: `team/contracts/cli-operator-safety-p0.md` slice 2/3.
//!
//! Limits are propagated from the CLI / dashboard launch request through
//! `EvalRunRequest` into the executor builder. The executor checks
//! `EvalLimits::check(...)` after each decision's token usage is
//! accumulated. On breach, the executor records a reason and calls
//! `RunStore::cancel_active`, which the next-bar `is_terminal` gate
//! converts into a `RunStatus::Cancelled` exit.
//!
//! Choice of "cancelled" terminal status (over a new
//! `cancelled_limit` enum variant) keeps the migration footprint at
//! zero — the existing `error` column carries the breach reason and the
//! dashboard's status filter already understands `cancelled`. If the
//! product later wants to distinguish "operator-cancelled" from
//! "limit-breach-cancelled" in the inspector, that's a follow-on
//! migration.

use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Per-run caps. Every field is optional; `None` means "no cap".
///
/// Backend payload of the launch verb (`xvn eval run --max-decisions …`).
/// Threaded through `EvalRunRequest` → executor builder.
#[cfg_attr(feature = "ts-export", derive(ts_rs::TS))]
#[cfg_attr(
    feature = "ts-export",
    ts(export, export_to = "../../../frontend/web/src/api/types.gen/")
)]
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvalLimits {
    /// Max number of decision cycles the strategy may produce. Each
    /// cadence-gated bar that the trader emits a decision for counts
    /// as one. Warmup bars never count.
    #[serde(default)]
    pub max_decisions: Option<u32>,
    /// Max cumulative input tokens across all model calls in the run.
    #[serde(default)]
    pub max_input_tokens: Option<u64>,
    /// Max cumulative output tokens across all model calls in the run.
    #[serde(default)]
    pub max_output_tokens: Option<u64>,
    /// Max wall-clock seconds the run may take from start to terminal.
    /// Serialized as seconds for wire stability across language clients.
    #[serde(default)]
    pub max_wall_clock_secs: Option<u64>,
    /// When `true`, an `max_input_tokens` breach lands the run as
    /// `Cancelled`; when `false`, that input-token breach is logged but
    /// the run continues (advisory mode).
    ///
    /// NOTE (strict output cap): `max_output_tokens` no longer respects
    /// this flag — when `max_output_tokens` IS set it is ALWAYS a hard
    /// cap (a runaway output is misconfiguration, not something to log
    /// and continue). This flag now only gates `max_input_tokens`.
    /// `max_decisions` and `max_wall_clock_secs` are, as before, always
    /// hard caps.
    #[serde(default)]
    pub cancel_on_token_limit: bool,
    /// Flag decisions as delayed when the decision bar's age exceeds
    /// this many milliseconds. Only for live/forward-test mode.
    /// `None` = never flag (all decisions accepted, none delayed).
    #[serde(default)]
    pub stale_data_max_age_ms: Option<u64>,
    /// Hang belt: cancel an agent that has been running longer than
    /// this many milliseconds without producing a decision.
    /// Only for live/forward-test mode. Opt-in.
    #[serde(default)]
    pub max_agent_ms: Option<u64>,
    /// Maximum consecutive skips before emitting a Degraded health
    /// event. Default 5. Only for live/forward-test mode.
    #[serde(default)]
    pub max_consecutive_skips: u32,
}

impl EvalLimits {
    /// Returns `true` if every cap is None (the default — pre-limits
    /// behavior). Callers may skip the per-bar check when this is true.
    pub fn is_empty(&self) -> bool {
        self.max_decisions.is_none()
            && self.max_input_tokens.is_none()
            && self.max_output_tokens.is_none()
            && self.max_wall_clock_secs.is_none()
            && self.stale_data_max_age_ms.is_none()
            && self.max_agent_ms.is_none()
    }

    /// Wall-clock cap parsed as a `Duration`. Returns `None` when
    /// `max_wall_clock_secs` is unset.
    pub fn max_wall_clock(&self) -> Option<Duration> {
        self.max_wall_clock_secs.map(Duration::from_secs)
    }

    /// Evaluate every cap against the current counters. Returns the
    /// first breach encountered, or `None` if every cap is either
    /// unset or still under its threshold.
    ///
    /// Cap semantics:
    /// - `max_decisions` and `max_wall_clock_secs`: always hard caps.
    /// - `max_output_tokens`: **strict when set** — always a hard cap,
    ///   independent of `cancel_on_token_limit`. A 172k-token response
    ///   is misconfiguration, not something to silently log and
    ///   continue, so when an operator sets this cap it is enforced.
    /// - `max_input_tokens`: still respects `cancel_on_token_limit`
    ///   (advisory unless the flag is set), preserving prior behaviour.
    pub fn check_for_cancel(
        &self,
        decisions: u32,
        input_tokens: u64,
        output_tokens: u64,
        started_at: Instant,
    ) -> Option<LimitBreach> {
        if let Some(cap) = self.max_decisions {
            if decisions >= cap {
                return Some(LimitBreach::MaxDecisions { cap });
            }
        }
        if let Some(d) = self.max_wall_clock() {
            if started_at.elapsed() >= d {
                return Some(LimitBreach::MaxWallClock {
                    cap_secs: d.as_secs(),
                });
            }
        }
        // Strict output cap: enforced whenever set, regardless of the
        // advisory flag.
        if let Some(cap) = self.max_output_tokens {
            if output_tokens >= cap {
                return Some(LimitBreach::MaxOutputTokens { cap });
            }
        }
        // Input-token cap remains advisory unless the flag opts in.
        if self.cancel_on_token_limit {
            if let Some(cap) = self.max_input_tokens {
                if input_tokens >= cap {
                    return Some(LimitBreach::MaxInputTokens { cap });
                }
            }
        }
        None
    }
}

/// Which cap fired and at what threshold. Serialized into the run's
/// `error` field so the inspector can render a human-readable reason.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimitBreach {
    MaxDecisions { cap: u32 },
    MaxInputTokens { cap: u64 },
    MaxOutputTokens { cap: u64 },
    MaxWallClock { cap_secs: u64 },
}

impl LimitBreach {
    /// Stable reason string written to the run's `error` column. Format:
    /// `"cancelled by limit: <key>=<cap>"`. The prefix lets the
    /// dashboard distinguish operator cancels (`"cancelled by user"`)
    /// from limit cancels.
    pub fn reason(&self) -> String {
        match self {
            LimitBreach::MaxDecisions { cap } => {
                format!("cancelled by limit: max_decisions={cap}")
            }
            LimitBreach::MaxInputTokens { cap } => {
                format!("cancelled by limit: max_input_tokens={cap}")
            }
            LimitBreach::MaxOutputTokens { cap } => {
                format!("cancelled by limit: max_output_tokens={cap}")
            }
            LimitBreach::MaxWallClock { cap_secs } => {
                format!("cancelled by limit: max_wall_clock_secs={cap_secs}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t0() -> Instant {
        Instant::now()
    }

    #[test]
    fn default_is_empty_and_never_breaches() {
        let limits = EvalLimits::default();
        assert!(limits.is_empty());
        assert!(limits
            .check_for_cancel(1_000, 1_000_000, 1_000_000, t0())
            .is_none());
    }

    #[test]
    fn max_decisions_breaches_at_cap() {
        let limits = EvalLimits {
            max_decisions: Some(5),
            ..Default::default()
        };
        assert!(limits.check_for_cancel(4, 0, 0, t0()).is_none());
        assert_eq!(
            limits.check_for_cancel(5, 0, 0, t0()),
            Some(LimitBreach::MaxDecisions { cap: 5 })
        );
        assert_eq!(
            limits.check_for_cancel(99, 0, 0, t0()),
            Some(LimitBreach::MaxDecisions { cap: 5 })
        );
    }

    #[test]
    fn input_token_cap_requires_cancel_on_token_limit_flag() {
        // Input-token cap is still advisory unless the flag is set.
        let advisory = EvalLimits {
            max_input_tokens: Some(100),
            cancel_on_token_limit: false,
            ..Default::default()
        };
        assert!(advisory.check_for_cancel(0, 1_000, 0, t0()).is_none());

        let strict = EvalLimits {
            max_input_tokens: Some(100),
            cancel_on_token_limit: true,
            ..Default::default()
        };
        assert_eq!(
            strict.check_for_cancel(0, 100, 0, t0()),
            Some(LimitBreach::MaxInputTokens { cap: 100 })
        );
    }

    #[test]
    fn output_token_cap_is_strict_when_set_without_flag() {
        // Strict-when-set: with cancel_on_token_limit == false, exceeding
        // max_output_tokens now STOPS the run. (Before the strict change
        // this returned None and the run continued.)
        let limits = EvalLimits {
            max_output_tokens: Some(100),
            cancel_on_token_limit: false,
            ..Default::default()
        };
        // Under the cap → no breach.
        assert!(limits.check_for_cancel(0, 0, 99, t0()).is_none());
        // At/over the cap → breach, even though the flag is false.
        assert_eq!(
            limits.check_for_cancel(0, 0, 100, t0()),
            Some(LimitBreach::MaxOutputTokens { cap: 100 })
        );
        assert_eq!(
            limits.check_for_cancel(0, 0, 1_000, t0()),
            Some(LimitBreach::MaxOutputTokens { cap: 100 })
        );
    }

    #[test]
    fn unset_output_cap_never_breaches() {
        // When max_output_tokens is None, even huge output is fine —
        // strict-when-SET, not strict-always.
        let limits = EvalLimits {
            max_output_tokens: None,
            cancel_on_token_limit: false,
            ..Default::default()
        };
        assert!(limits.check_for_cancel(0, 0, 1_000_000, t0()).is_none());
    }

    #[test]
    fn wall_clock_is_always_hard_cap() {
        let limits = EvalLimits {
            max_wall_clock_secs: Some(0), // breach immediately
            cancel_on_token_limit: false,
            ..Default::default()
        };
        // A zero-duration cap breaches on the very first check.
        assert!(matches!(
            limits.check_for_cancel(0, 0, 0, t0()),
            Some(LimitBreach::MaxWallClock { cap_secs: 0 })
        ));
    }

    #[test]
    fn reason_strings_are_stable() {
        assert_eq!(
            LimitBreach::MaxDecisions { cap: 100 }.reason(),
            "cancelled by limit: max_decisions=100"
        );
        assert_eq!(
            LimitBreach::MaxOutputTokens { cap: 60_000 }.reason(),
            "cancelled by limit: max_output_tokens=60000"
        );
    }
}
