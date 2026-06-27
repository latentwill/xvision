//! Per-tool forward-only mode policy. `ToolDescriptor` lives in
//! `xvision_agent_client::protocol` (a different crate), so the policy table
//! is engine-local — keyed by the operator-facing tool name. `None` => an
//! unrestricted built-in (ohlcv, submit_decision, …). Consulted in two places:
//! the advertisement filter (execute path) and the dispatch chokepoint guard
//! (`ToolRegistryDispatch::invoke`).

use crate::eval::run::RunMode;
use chrono::{DateTime, Duration, NaiveDate, Utc};

/// Default lookahead lag (days). `as_of_date` is day-granular; same-day data
/// can leak post-decision flows, so we anchor to a completed UTC day.
pub const DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS: i64 = 1;

/// Backtest anchor date for a Nansen historical call: floor the simulated
/// clock to its UTC calendar day, then subtract `lag_days`. The model cannot
/// influence this — it is computed from the framework clock and overwrites any
/// model-supplied `as_of_date` (lookahead-safety invariant, D4).
pub fn nansen_as_of_date(sim_now: DateTime<Utc>, lag_days: i64) -> NaiveDate {
    sim_now.date_naive() - Duration::days(lag_days)
}

/// Whether a signal tool is advertised + callable in each run mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolModePolicy {
    /// Callable in `RunMode::Live` (forward/live runs).
    pub live: bool,
    /// Callable in `RunMode::Backtest` — only true for tools with a
    /// lookahead-safe point-in-time binding (Nansen `/v1beta1`).
    pub backtest: bool,
}

const NANSEN: ToolModePolicy = ToolModePolicy {
    live: true,
    backtest: true,
};
const ELFA: ToolModePolicy = ToolModePolicy {
    live: true,
    backtest: false,
};

/// All Nansen tool names (live + backtest; backtest routes to the v1beta1
/// historical binding with an injected `as_of_date`).
pub const NANSEN_TOOLS: [&str; 3] = [
    "nansen_smart_money_flow",
    "nansen_token_screener",
    "nansen_flow_intel",
];
/// All Elfa tool names (forward-only).
pub const ELFA_TOOLS: [&str; 3] = [
    "elfa_smart_mentions",
    "elfa_trending_tokens",
    "elfa_trending_narratives",
];

/// Which external signal provider owns a given tool. Used to route per-provider
/// credit budget accounting (xvision-im2r.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalProvider {
    Nansen,
    Elfa,
}

/// Return the provider that owns `name`, or `None` for builtins / unknown tools.
pub fn tool_provider(name: &str) -> Option<SignalProvider> {
    if NANSEN_TOOLS.contains(&name) {
        Some(SignalProvider::Nansen)
    } else if ELFA_TOOLS.contains(&name) {
        Some(SignalProvider::Elfa)
    } else {
        None
    }
}

/// Policy for a signal tool, or `None` for an unrestricted built-in.
pub fn signal_tool_policy(name: &str) -> Option<&'static ToolModePolicy> {
    if NANSEN_TOOLS.contains(&name) {
        Some(&NANSEN)
    } else if ELFA_TOOLS.contains(&name) {
        Some(&ELFA)
    } else {
        None
    }
}

/// True for Nansen tools — the only tools that get the backtest `as_of_date`
/// anchor injected into their input.
pub fn is_nansen_tool(name: &str) -> bool {
    NANSEN_TOOLS.contains(&name)
}

/// Resolved per-run signal-tool configuration: the enabled Nansen and Elfa
/// entries from `xvn.toml`, already parsed and ready to use.  Built once per
/// run start by `build_tool_registry`; stored in the `ToolRegistry` so
/// `spawn_cline_ctx` can read lag/budgets from the registry it already holds
/// rather than re-parsing `xvn.toml` (xvision-im2r.6).
#[derive(Debug, Clone, Default)]
pub struct SignalToolConfig {
    /// First enabled Nansen entry, or `None` when Nansen is not configured.
    pub nansen_entry: Option<xvision_core::config::DataToolEntry>,
    /// First enabled Elfa entry, or `None` when Elfa is not configured.
    pub elfa_entry: Option<xvision_core::config::DataToolEntry>,
}

impl SignalToolConfig {
    /// Lookahead lag from the Nansen entry, or the engine default.
    pub fn nansen_lag_days(&self) -> i64 {
        self.nansen_entry
            .as_ref()
            .and_then(|e| e.nansen_lookahead_lag_days)
            .map(|d| d as i64)
            .unwrap_or(DEFAULT_NANSEN_LOOKAHEAD_LAG_DAYS)
    }

    /// Per-run Nansen credit budget as a fresh `AtomicU32` Arc, or `None` if
    /// uncapped.
    pub fn nansen_budget_arc(&self) -> Option<std::sync::Arc<std::sync::atomic::AtomicU32>> {
        self.nansen_entry
            .as_ref()
            .and_then(|e| e.budget_credits_per_run)
            .map(|n| std::sync::Arc::new(std::sync::atomic::AtomicU32::new(n)))
    }

    /// Per-run Elfa credit budget as a fresh `AtomicU32` Arc, or `None` if
    /// uncapped.
    pub fn elfa_budget_arc(&self) -> Option<std::sync::Arc<std::sync::atomic::AtomicU32>> {
        self.elfa_entry
            .as_ref()
            .and_then(|e| e.budget_credits_per_run)
            .map(|n| std::sync::Arc::new(std::sync::atomic::AtomicU32::new(n)))
    }
}

/// Build a structured degrade response: `{ "available": false, "reason": … }`.
///
/// This is the canonical, single definition.  The per-file `degrade()` helpers
/// in `tools/nansen.rs` and `tools/elfa.rs` and the private `signal_degrade`
/// in `api/eval.rs` all delegate here (xvision-im2r.9).
pub fn signal_unavailable(reason: impl Into<String>) -> serde_json::Value {
    serde_json::json!({ "available": false, "reason": reason.into() })
}

/// Drop any tool whose forward-only policy forbids it in `mode`. Unrestricted
/// built-ins (policy `None`) always pass. This is the advertisement filter:
/// the trader never even sees a tool it isn't allowed to call this run.
pub fn filter_tools_for_mode(tools: &[String], mode: RunMode) -> Vec<String> {
    tools
        .iter()
        .filter(|name| match signal_tool_policy(name) {
            Some(p) => match mode {
                RunMode::Forward => p.live,
                RunMode::Backtest => p.backtest,
            },
            None => true,
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::eval::run::RunMode;
    use chrono::{TimeZone, Utc};

    #[test]
    fn nansen_tools_allowed_in_both_modes() {
        for n in [
            "nansen_smart_money_flow",
            "nansen_token_screener",
            "nansen_flow_intel",
        ] {
            let p = signal_tool_policy(n).expect("nansen tool has a policy");
            assert!(p.live && p.backtest, "{n} must be live+backtest");
        }
    }

    #[test]
    fn elfa_tools_are_forward_only() {
        for n in [
            "elfa_smart_mentions",
            "elfa_trending_tokens",
            "elfa_trending_narratives",
        ] {
            let p = signal_tool_policy(n).expect("elfa tool has a policy");
            assert!(p.live && !p.backtest, "{n} must be live-only (forward-only)");
        }
    }

    #[test]
    fn builtins_are_unrestricted() {
        assert!(signal_tool_policy("ohlcv").is_none());
        assert!(signal_tool_policy("submit_decision").is_none());
    }

    #[test]
    fn nansen_tools_are_recognized_for_as_of_injection() {
        assert!(is_nansen_tool("nansen_smart_money_flow"));
        assert!(!is_nansen_tool("elfa_smart_mentions"));
        assert!(!is_nansen_tool("ohlcv"));
    }

    #[test]
    fn as_of_floors_to_completed_utc_day_minus_lag() {
        // Decision mid-day 2024-03-15T14:00Z, lag 1 ⇒ anchor 2024-03-14.
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 15, 14, 0, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 1).to_string(), "2024-03-14");
    }

    #[test]
    fn as_of_lag_zero_is_same_day_floor() {
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 15, 23, 59, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 0).to_string(), "2024-03-15");
    }

    #[test]
    fn as_of_handles_month_boundary() {
        let sim_now = Utc.with_ymd_and_hms(2024, 3, 1, 0, 0, 0).unwrap();
        assert_eq!(nansen_as_of_date(sim_now, 1).to_string(), "2024-02-29"); // leap year
    }

    #[test]
    fn tool_provider_maps_correctly() {
        use super::{tool_provider, SignalProvider};
        assert_eq!(
            tool_provider("nansen_smart_money_flow"),
            Some(SignalProvider::Nansen)
        );
        assert_eq!(
            tool_provider("nansen_token_screener"),
            Some(SignalProvider::Nansen)
        );
        assert_eq!(tool_provider("nansen_flow_intel"), Some(SignalProvider::Nansen));
        assert_eq!(tool_provider("elfa_smart_mentions"), Some(SignalProvider::Elfa));
        assert_eq!(tool_provider("elfa_trending_tokens"), Some(SignalProvider::Elfa));
        assert_eq!(
            tool_provider("elfa_trending_narratives"),
            Some(SignalProvider::Elfa)
        );
        assert_eq!(tool_provider("ohlcv"), None);
        assert_eq!(tool_provider("submit_decision"), None);
        assert_eq!(tool_provider("unknown_tool"), None);
    }

    #[test]
    fn backtest_strips_elfa_keeps_nansen_and_builtins() {
        let tools = vec![
            "ohlcv".to_string(),
            "nansen_smart_money_flow".to_string(),
            "elfa_smart_mentions".to_string(),
            "submit_decision".to_string(),
        ];
        let out = filter_tools_for_mode(&tools, RunMode::Backtest);
        assert!(out.contains(&"ohlcv".to_string()));
        assert!(out.contains(&"nansen_smart_money_flow".to_string()));
        assert!(out.contains(&"submit_decision".to_string()));
        assert!(
            !out.contains(&"elfa_smart_mentions".to_string()),
            "elfa stripped in backtest"
        );
    }

    #[test]
    fn live_keeps_everything() {
        let tools = vec!["elfa_smart_mentions".to_string(), "nansen_flow_intel".to_string()];
        assert_eq!(filter_tools_for_mode(&tools, RunMode::Forward), tools);
    }
}
