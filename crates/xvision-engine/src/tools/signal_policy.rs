//! Per-tool forward-only mode policy. `ToolDescriptor` lives in
//! `xvision_agent_client::protocol` (a different crate), so the policy table
//! is engine-local — keyed by the operator-facing tool name. `None` => an
//! unrestricted built-in (ohlcv, submit_decision, …). Consulted in two places:
//! the advertisement filter (execute path) and the dispatch chokepoint guard
//! (`ToolRegistryDispatch::invoke`).

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

#[cfg(test)]
mod tests {
    use super::*;

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
}
