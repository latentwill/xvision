//! MCP tool-set guard test.
//!
//! Asserts that the set of MCP tool names returned by
//! `XvisionTools::tool_names()` exactly matches the committed inventory
//! `EXPECTED_MCP_TOOLS`.
//!
//! When this test fails it means a tool was added or removed from the
//! `#[tool_router]` impl in `crates/xvision-mcp/src/tools.rs` without
//! updating the constant here **and** the parity matrix document at
//! `docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md`.
//!
//! To fix:
//!   1. Update `XvisionTools::tool_names()` in `src/tools.rs` to match the
//!      live tool list.
//!   2. Update `EXPECTED_MCP_TOOLS` in this file to the same sorted list.
//!   3. Update the parity matrix document to reflect the change.

use xvision_mcp::tools::XvisionTools;

/// The committed set of MCP tool names, sorted alphabetically.
///
/// This is the source of truth for the parity guard. When a new tool is
/// added to `XvisionTools`, add its name here (in sorted order) and update
/// the parity matrix document.
const EXPECTED_MCP_TOOLS: &[&str] = &[
    "xvn_atr",
    "xvn_bollinger",
    "xvn_create_strategy",
    "xvn_donchian",
    "xvn_ema",
    "xvn_eval_batch_run",
    "xvn_eval_batch_status",
    "xvn_eval_behavior",
    "xvn_eval_compare",
    "xvn_eval_compare_ext",
    "xvn_eval_compare_report",
    "xvn_eval_findings",
    "xvn_eval_get",
    "xvn_eval_list",
    "xvn_eval_metrics",
    "xvn_eval_scenarios",
    "xvn_fib_retracements",
    "xvn_get_strategy",
    "xvn_health",
    "xvn_list_templates",
    "xvn_macd",
    "xvn_marketplace_browse",
    "xvn_marketplace_buy",
    "xvn_marketplace_get_listing",
    "xvn_marketplace_import",
    "xvn_marketplace_wallet",
    "xvn_rsi",
    "xvn_scenario_inspect_card",
    "xvn_scenarios_select",
    "xvn_set_risk_config",
    "xvn_sma",
    "xvn_strategy_create_atomic",
    "xvn_strategy_validate_preflight",
    "xvn_update_slot",
    "xvn_validate_draft",
];

#[test]
fn mcp_tool_set_matches_committed_inventory() {
    let mut live = XvisionTools::tool_names();
    live.sort_unstable();

    let mut expected: Vec<&str> = EXPECTED_MCP_TOOLS.to_vec();
    expected.sort_unstable();

    // Compute diff for a useful failure message.
    let added: Vec<&str> = live.iter().copied().filter(|n| !expected.contains(n)).collect();
    let removed: Vec<&str> = expected.iter().copied().filter(|n| !live.contains(n)).collect();

    assert!(
        added.is_empty() && removed.is_empty(),
        "MCP tool set changed — update EXPECTED_MCP_TOOLS and \
        docs/superpowers/evidence/2026-05-25-agent-cli-press-audit/mcp-parity-matrix.md\n\
        added (in live, missing from constant): {added:?}\n\
        removed (in constant, missing from live): {removed:?}"
    );
}
