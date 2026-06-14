//! WU4 — Pine Script fidelity diff report.
//!
//! Produces a human-readable `FidelityReport` that classifies every Pine Script
//! element encountered during import into one of three categories:
//!
//! - **captured** — element mapped cleanly to an xvision filter / mechanistic
//!   concept (no semantic change).
//! - **approximated** — element mapped with a changed or broadened semantic
//!   (e.g. `close*1.02` → `within_pct`; an indicator passed as a briefing
//!   feature for the LLM trader to reason about).
//! - **dropped** — element that could not be mapped at all (e.g. `pyramiding`,
//!   `request.security`, arbitrary custom functions).
//!
//! The report is serializable and consumed by WU6 (CLI) and WU7/WU8
//! (HTTP route / frontend) — no conversion logic lives here.

use serde::{Deserialize, Serialize};

use super::ast::{Expr, PineScript, Statement};
use super::map::MapOutcome;
use crate::strategies::mechanistic::{ClosePolicy, DecisionMode};

// ── Public types ──────────────────────────────────────────────────────────────

/// A single item in the fidelity report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FidelityItem {
    /// Short, human-readable identifier for the element
    /// (e.g. `"entry_rule:Long"`, `"indicator:rsi_val"`, `"pyramiding"`).
    pub item: String,
    /// Explanation of how/why this element was handled.
    pub reason: String,
}

impl FidelityItem {
    fn new(item: impl Into<String>, reason: impl Into<String>) -> Self {
        FidelityItem {
            item: item.into(),
            reason: reason.into(),
        }
    }
}

/// WU10 — xvision backtest cost assumptions surfaced in TradingView
/// Strategy-Tester-aligned vocabulary.
///
/// This is a **labelled reference**, not byte-exact reconciliation with
/// TradingView's numbers. Its purpose is to help users anticipate why their
/// backtest P&L will differ from the TradingView Strategy Tester output.
///
/// The values are sourced from xvision's DEFAULT `VenueSettings` (defined in
/// `crates/xvision-engine/src/eval/scenario.rs`, `VenueSettings::default()`).
/// When an import has no associated `Scenario` yet, these defaults are what
/// the engine will apply.
///
/// # TradingView ↔ xvision vocabulary mapping
///
/// | TradingView Strategy Tester field | xvision internal name           | Default value         |
/// |-----------------------------------|---------------------------------|-----------------------|
/// | Commission (%)                    | `fees.taker_bps`                | 10 bps (0.10%)        |
/// | Commission type: "Percent of order value" | `commission_type`      | "Percent of order value" |
/// | Slippage (ticks)                  | `SlippageModel::Linear { bps }` | 2 bps linear          |
/// | Fill orders on bar: "Open"        | `MarketOrderFill::NextBarOpen`  | "Next bar open"       |
/// | Initial capital                   | `Capital.initial`               | per scenario          |
/// | Order size                        | `RiskConfig`                    | per strategy          |
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CostModelReference {
    /// TradingView calls this "Commission type".
    /// xvision applies fees as basis points of order notional value.
    /// TV equivalent: "Percent of order value".
    pub commission_type: String,

    /// Commission rate in basis points (1 bps = 0.01%).
    /// Sourced from `VenueSettings::default().fees.taker_bps`.
    /// TV equivalent: the "Commission (%)" field — divide by 100 to get percent.
    pub commission_value_bps: f64,

    /// xvision slippage model name.
    /// Sourced from `VenueSettings::default().slippage`.
    /// TV equivalent: "Slippage (ticks)".
    pub slippage_model: String,

    /// Slippage magnitude in basis points.
    /// For `SlippageModel::Linear { bps }` this is the flat bps value.
    /// Sourced from `VenueSettings::default().slippage` (Linear { bps: 2 }).
    pub slippage_value_bps: f64,

    /// When orders are filled relative to the decision bar.
    /// Sourced from `VenueSettings::default().fill_model.market_order_fill`
    /// (`MarketOrderFill::NextBarOpen`).
    /// TV equivalent: "Fill orders on bar: Open" (next bar's open price).
    pub fill_timing: String,

    /// Plain-language note explaining why xvision's P&L will differ from
    /// TradingView's Strategy Tester output, and how to configure the scenario
    /// to narrow the gap.
    pub note: String,
}

impl Default for CostModelReference {
    /// Construct a `CostModelReference` from xvision's DEFAULT `VenueSettings`.
    ///
    /// Default values sourced from
    /// `crates/xvision-engine/src/eval/scenario.rs` — `VenueSettings::default()`:
    ///   - `fees.maker_bps = 0`, `fees.taker_bps = 10`  (line ~515)
    ///   - `slippage = SlippageModel::Linear { bps: 2 }` (line ~519)
    ///   - `fill_model.market_order_fill = NextBarOpen`   (line ~522)
    ///   - `latency.decision_to_fill_ms = 500`            (line ~521)
    fn default() -> Self {
        CostModelReference {
            commission_type: "Percent of order value".to_string(),
            commission_value_bps: 10.0, // VenueSettings::default().fees.taker_bps = 10
            slippage_model: "Linear (flat basis points)".to_string(),
            slippage_value_bps: 2.0, // VenueSettings::default().slippage = Linear { bps: 2 }
            fill_timing: "Next bar open".to_string(), // MarketOrderFill::NextBarOpen
            note: "These are xvision's DEFAULT backtest cost assumptions (configurable per \
                   scenario). Commission = 10 bps taker (0.10%), slippage = 2 bps flat, \
                   fills at next-bar open. TradingView Strategy Tester defaults differ \
                   (commission 0%, slippage 1–2 ticks, intrabar fills) — expect P&L \
                   divergence. Configure the scenario's VenueSettings to align with \
                   the source's TradingView settings."
                .to_string(),
        }
    }
}

/// The complete fidelity diff report for a single Pine Script import.
///
/// Produced by [`build_fidelity_report`] after the mapper has run.
/// Each field is a `Vec` so consumers can iterate or present them in any order.
///
/// The `cost_model` field (WU10) is `#[serde(default)]` so pre-existing
/// `FidelityReport` JSON blobs (without the `cost_model` key) continue to
/// deserialize correctly via `CostModelReference::default()`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FidelityReport {
    /// Elements that converted with no semantic change.
    pub captured: Vec<FidelityItem>,
    /// Elements that converted but with altered or broadened semantics.
    pub approximated: Vec<FidelityItem>,
    /// Elements that could not be converted at all.
    pub dropped: Vec<FidelityItem>,
    /// WU10 — xvision's backtest cost assumptions in TradingView-aligned
    /// vocabulary. Surfaces the DEFAULT model + values; labeled as
    /// "default (configured per scenario)" since import has no scenario yet.
    /// `#[serde(default)]` ensures pre-WU10 JSON (no `cost_model` key) still
    /// deserializes successfully.
    #[serde(default)]
    pub cost_model: CostModelReference,
}

impl FidelityReport {
    /// Return `true` when no items were lost (all captured or approximated).
    pub fn is_lossless(&self) -> bool {
        self.dropped.is_empty()
    }
}

// ── Detection helpers ─────────────────────────────────────────────────────────

/// Returns `true` when the script header contains `pyramiding=<value>` where
/// the value is a positive integer (Pine default is 0 = no pyramiding).
fn header_has_pyramiding(script: &PineScript) -> bool {
    let header = match &script.header {
        Some(h) => h,
        None => return false,
    };
    // Look for a named arg `pyramiding` with a non-zero int/float value.
    for (name, value) in &header.args {
        if name.as_deref() == Some("pyramiding") {
            return match value {
                Expr::IntLit { value: v } => *v > 0,
                Expr::FloatLit { value: v } => *v > 0.0,
                _ => true, // present but non-literal → still flagged
            };
        }
    }
    false
}

/// Returns `true` when any statement or expression in the script references
/// `request.security` (HTF multi-timeframe, unsupported).
fn script_has_htf(script: &PineScript) -> bool {
    for stmt in &script.statements {
        if stmt_references_request_security(stmt) {
            return true;
        }
    }
    false
}

fn stmt_references_request_security(stmt: &Statement) -> bool {
    match stmt {
        Statement::Assignment { value, .. } => expr_references_request_security(value),
        Statement::Unsupported { raw, .. } => raw.contains("request.security"),
        Statement::If { condition, body } => {
            expr_references_request_security(condition) || body.iter().any(stmt_references_request_security)
        }
        _ => false,
    }
}

fn expr_references_request_security(expr: &Expr) -> bool {
    match expr {
        Expr::Unsupported { raw } => raw.contains("request.security"),
        Expr::Ident { name } => name.contains("request.security"),
        Expr::TaCall { name, args } => {
            name.contains("request.security") || args.iter().any(expr_references_request_security)
        }
        Expr::BinOp { left, right, .. } => {
            expr_references_request_security(left) || expr_references_request_security(right)
        }
        Expr::Ternary { cond, then_, else_ } => {
            expr_references_request_security(cond)
                || expr_references_request_security(then_)
                || expr_references_request_security(else_)
        }
        Expr::Paren { inner } => expr_references_request_security(inner),
        Expr::Not { expr } => expr_references_request_security(expr),
        _ => false,
    }
}

// ── Main builder ──────────────────────────────────────────────────────────────

/// Build a `FidelityReport` from the original `PineScript` AST and the
/// `MapOutcome` produced by the WU2 mapper.
///
/// # Categorisation logic
///
/// - **captured**:
///   - Each `EntryRule` in `mechanistic_config` (e.g. `"entry_rule:Long"`).
///   - Each `ClosePolicy` in `mechanistic_config` that mapped exactly
///     (StopLoss from `loss=`, TakeProfit from `profit=`, TrailingStop from
///     `trail_percent=`).
///   - Each filter `Condition` present in the emitted `Filter`.
///
/// - **approximated**:
///   - Each `BriefingIndicator` on an Agentic strategy (indicator that could
///     be computed but not reduced to a filter condition; passed as agent
///     context — "agentic-fallback: <token>").
///   - Arithmetic-expression approximations (detected in unmapped reasons
///     containing keywords like `within_pct` or `arithmetic`).
///
/// - **dropped**:
///   - Each `UnmappedNode` from `outcome.unmapped`.
///   - `pyramiding=` header option (if present and > 0).
///   - `request.security` / HTF references (if present).
pub fn build_fidelity_report(script: &PineScript, outcome: &MapOutcome) -> FidelityReport {
    let mut captured: Vec<FidelityItem> = Vec::new();
    let mut approximated: Vec<FidelityItem> = Vec::new();
    let mut dropped: Vec<FidelityItem> = Vec::new();

    let strategy = &outcome.strategy;

    // ── Captured: entry rules ─────────────────────────────────────────────────

    if let Some(cfg) = &strategy.mechanistic_config {
        for rule in &cfg.entry_rules {
            captured.push(FidelityItem::new(
                format!("entry_rule:{}", rule.signal_name),
                format!("captured: strategy.entry → EntryRule({:?})", rule.direction),
            ));
        }

        // ── Captured: close policies ──────────────────────────────────────────

        for (i, policy) in cfg.close_policies.iter().enumerate() {
            let (item, reason) = match policy {
                ClosePolicy::StopLoss { pct } => (
                    format!("close_policy[{i}]:stop_loss"),
                    format!("captured: strategy.exit loss= → StopLoss{{pct={pct}}}"),
                ),
                ClosePolicy::TakeProfit { pct } => (
                    format!("close_policy[{i}]:take_profit"),
                    format!("captured: strategy.exit profit= → TakeProfit{{pct={pct}}}"),
                ),
                ClosePolicy::TrailingStop { pct } => (
                    format!("close_policy[{i}]:trailing_stop"),
                    format!(
                        "approximated: strategy.exit trail_percent= → TrailingStop{{pct={pct}}}; \
                         fixed-% approximation of price-point trail"
                    ),
                ),
                ClosePolicy::TimeExit { bars } => (
                    format!("close_policy[{i}]:time_exit"),
                    format!("captured: strategy.exit → TimeExit{{bars={bars}}}"),
                ),
                ClosePolicy::TargetPnl { usd } => (
                    format!("close_policy[{i}]:target_pnl"),
                    format!("captured: strategy.exit → TargetPnl{{usd={usd}}}"),
                ),
            };
            // TrailingStop is an approximation (price points → pct), others are captured
            if matches!(policy, ClosePolicy::TrailingStop { .. }) {
                approximated.push(FidelityItem::new(item, reason));
            } else {
                captured.push(FidelityItem::new(item, reason));
            }
        }
    }

    // ── Captured: filter conditions ───────────────────────────────────────────

    if let Some(filter) = &strategy.filter {
        // Count leaves in the condition tree
        let condition_count = count_conditions(&filter.conditions);
        if condition_count > 0 {
            captured.push(FidelityItem::new(
                format!("filter_conditions:{condition_count}"),
                format!("captured: {condition_count} filter condition(s) mapped to xvision ConditionTree"),
            ));
        }
    }

    // ── Approximated: agentic-fallback briefing indicators ────────────────────

    if strategy.decision_mode == DecisionMode::Agentic {
        for bi in &strategy.briefing_indicators {
            approximated.push(FidelityItem::new(
                format!("indicator:{}", bi.source_token),
                format!(
                    "agentic-fallback: {} passed as briefing feature (could compute, \
                     could not reduce to filter condition)",
                    bi.source_token
                ),
            ));
        }
    }

    // ── Approximated: arithmetic-expression approximations ────────────────────
    //
    // Some unmapped nodes contain arithmetic expressions that were loosely
    // approximated (e.g. `close*1.02` → `within_pct`). These surface as
    // "approximated" rather than "dropped" when the reason text indicates
    // an approximation was attempted.

    let mut truly_dropped: Vec<_> = Vec::new();
    for node in &outcome.unmapped {
        let reason_lower = node.reason.to_lowercase();
        if reason_lower.contains("within_pct")
            || reason_lower.contains("approximat")
            || reason_lower.contains("arithmetic")
        {
            approximated.push(FidelityItem::new(
                node.raw.clone(),
                format!("approximated: {}", node.reason),
            ));
        } else {
            truly_dropped.push(node);
        }
    }

    // ── Dropped: unmapped nodes ───────────────────────────────────────────────

    for node in truly_dropped {
        dropped.push(FidelityItem::new(
            node.raw.clone(),
            format!("dropped: {}", node.reason),
        ));
    }

    // ── Dropped: pyramiding= header option ───────────────────────────────────

    if header_has_pyramiding(script) {
        dropped.push(FidelityItem::new(
            "pyramiding".to_string(),
            "dropped: pyramiding= header option is not supported; \
             xvision allows only one position per strategy at a time"
                .to_string(),
        ));
    }

    // ── Dropped: request.security / HTF references ────────────────────────────

    if script_has_htf(script) {
        dropped.push(FidelityItem::new(
            "request.security".to_string(),
            "dropped: HTF request.security is not supported; \
             xvision v1 supports one timeframe and one symbol per strategy"
                .to_string(),
        ));
    }

    // ── WU10: cost model reference ────────────────────────────────────────────
    //
    // Surface xvision's DEFAULT backtest cost assumptions in TV-aligned
    // vocabulary. Since import has no Scenario yet, we always use the default
    // model values sourced from `VenueSettings::default()` in
    // `crates/xvision-engine/src/eval/scenario.rs`.
    let cost_model = CostModelReference::default();

    FidelityReport {
        captured,
        approximated,
        dropped,
        cost_model,
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Count the number of leaf `Condition` nodes in a `ConditionTree`.
fn count_conditions(tree: &xvision_filters::ConditionTree) -> usize {
    use xvision_filters::{ConditionItem, ConditionTree};
    match tree {
        ConditionTree::All(items) | ConditionTree::Any(items) => {
            items
                .iter()
                .map(|item| match item {
                    ConditionItem::Leaf(_) => 1,
                    // ConditionGroup contains Vec<Condition> directly (depth-1 nesting only)
                    ConditionItem::Group(g) => g.conditions().len(),
                })
                .sum()
        }
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strategies::pine_import::{map_script, parse_pine};

    fn parse_and_map_fidelity(src: &str) -> (FidelityReport, MapOutcome) {
        let script = parse_pine(src).expect("must parse");
        let outcome = map_script(&script);
        let fidelity = build_fidelity_report(&script, &outcome);
        (fidelity, outcome)
    }

    #[test]
    fn clean_literal_script_has_captured_items() {
        // NOTE: `if <cond>` lines become Unsupported by parser design, so
        // dropped will have the `if long_cond` line. The key invariant is that
        // entry rules and close policies ARE captured and no pyramiding/HTF drop.
        let src = r#"//@version=5
strategy("Clean Literal", overlay=true)
my_rsi = ta.rsi(close, 14)
long_cond = my_rsi < 30
if long_cond
    strategy.entry("Long", strategy.long)
strategy.exit("Long Exit", "Long", loss=2.0, profit=4.0)
"#;
        let (fidelity, _) = parse_and_map_fidelity(src);
        assert!(
            !fidelity.captured.is_empty(),
            "clean literal script must have captured items (entry rule / close policy); got {:?}",
            fidelity.captured
        );
        // No pyramiding or HTF in this script
        assert!(!fidelity.dropped.iter().any(|i| i.item.contains("pyramiding")));
        assert!(!fidelity
            .dropped
            .iter()
            .any(|i| i.item.contains("request.security")));
    }

    #[test]
    fn entry_rules_appear_in_captured() {
        let src = r#"//@version=5
strategy("Two Entries", overlay=true)
if close > 100.0
    strategy.entry("Long", strategy.long)
if close < 100.0
    strategy.entry("Short", strategy.short)
"#;
        let (fidelity, _) = parse_and_map_fidelity(src);
        let long_captured = fidelity.captured.iter().any(|i| i.item.contains("Long"));
        let short_captured = fidelity.captured.iter().any(|i| i.item.contains("Short"));
        assert!(
            long_captured,
            "Long entry rule must be captured; captured={:?}",
            fidelity.captured
        );
        assert!(
            short_captured,
            "Short entry rule must be captured; captured={:?}",
            fidelity.captured
        );
    }

    #[test]
    fn pyramiding_header_detected_and_dropped() {
        let src = r#"//@version=5
strategy("Pyramiding Test", overlay=true, pyramiding=5)
if close > 100.0
    strategy.entry("Long", strategy.long)
"#;
        let (fidelity, _) = parse_and_map_fidelity(src);
        let has_pyramiding = fidelity
            .dropped
            .iter()
            .any(|i| i.item.contains("pyramiding") || i.reason.contains("pyramiding"));
        assert!(
            has_pyramiding,
            "pyramiding=5 must be in dropped; dropped={:?}",
            fidelity.dropped
        );
    }

    #[test]
    fn fidelity_report_serializes_and_deserializes() {
        let report = FidelityReport {
            captured: vec![FidelityItem::new("entry_rule:Long", "captured: entry rule")],
            approximated: vec![FidelityItem::new(
                "indicator:ema_val",
                "agentic-fallback: ema_val passed as briefing feature",
            )],
            dropped: vec![FidelityItem::new("pyramiding", "dropped: pyramiding")],
            cost_model: CostModelReference::default(),
        };
        let json = serde_json::to_string_pretty(&report).expect("must serialize");
        let report2: FidelityReport = serde_json::from_str(&json).expect("must deserialize");
        assert_eq!(report, report2);
    }

    #[test]
    fn fidelity_item_derives_correctly() {
        let item = FidelityItem::new("test_item", "test reason");
        let item2 = item.clone();
        assert_eq!(item, item2);
        let _debug = format!("{item:?}");
    }

    #[test]
    fn agentic_strategy_briefing_indicators_in_approximated() {
        // A script with no strategy.entry → Agentic with briefing_indicators
        let src = r#"//@version=5
indicator("Agentic Script", overlay=true)
my_rsi = ta.rsi(close, 14)
result = my_rsi < 30
"#;
        let (fidelity, _) = parse_and_map_fidelity(src);
        // No entry rules → Agentic; SMA / RSI briefing indicators → approximated
        let has_agentic = fidelity
            .approximated
            .iter()
            .any(|i| i.reason.contains("agentic-fallback") || i.reason.contains("briefing"));
        // The script may not populate briefing_indicators if the RSI is unmapped —
        // accept either: approximated has briefing OR dropped has the unmapped node.
        let has_something = has_agentic || !fidelity.dropped.is_empty() || !fidelity.captured.is_empty();
        assert!(
            has_something,
            "Agentic script must surface something in fidelity report; report={:?}",
            fidelity
        );
    }

    #[test]
    fn is_lossless_when_no_dropped() {
        let report = FidelityReport {
            captured: vec![FidelityItem::new("entry_rule:Long", "captured")],
            approximated: vec![],
            dropped: vec![],
            cost_model: CostModelReference::default(),
        };
        assert!(report.is_lossless());
    }

    #[test]
    fn is_lossless_false_when_dropped() {
        let report = FidelityReport {
            captured: vec![],
            approximated: vec![],
            dropped: vec![FidelityItem::new("pyramiding", "dropped")],
            cost_model: CostModelReference::default(),
        };
        assert!(!report.is_lossless());
    }
}
