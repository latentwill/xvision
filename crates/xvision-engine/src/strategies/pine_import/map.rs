//! WU2 — Pine Script v5 AST → xvision `Strategy` mapper.
//!
//! ## Public surface
//!
//! ```ignore
//! pub fn map_script(script: &PineScript) -> MapOutcome
//! ```
//!
//! ## Mapping rules
//!
//! The mapper works in two passes:
//!
//! 1. **Indicator extraction** — walk `Statement::TaAssignment` nodes and build
//!    a lookup table of Pine variable names → `IndicatorName` + period.
//!
//! 2. **Decision routing** — based on what was harvested decide whether the
//!    script maps cleanly to `Mechanistic` or must fall back to `Agentic`:
//!    - If we successfully extract ≥1 `EntryRule` + valid `Filter` conditions
//!      → `Mechanistic`.
//!    - If we extract indicators but the predicate is too fuzzy (ternaries,
//!      `var`-counters, or arithmetic compounds) → `Agentic` with
//!      `briefing_indicators` populated.
//!    - If nothing is extractable → minimal `Agentic` strategy.
//!
//! ## Validation invariant
//!
//! Every returned `Strategy` passes `validate_strategy` **by construction**.
//! Any intermediate result that would fail validation is demoted: close policies
//! with invalid percentages are dropped, entry rules with missing data are
//! skipped, and a Mechanistic strategy with no entry rules is converted to
//! Agentic rather than returned invalid.

use serde::{Deserialize, Serialize};
use ulid::Ulid;
use xvision_filters::{
    validate as validate_filter, ActivationMode, Condition, ConditionItem, ConditionTree, Filter, FilterId,
    FilterStatus, IndicatorName, IndicatorRef, Operand, Operator, ScanCadence, Symbol, Timeframe,
};

use super::ast::{Expr, PineScript, Statement};
use crate::strategies::agent_ref::AgentRef;
use crate::strategies::manifest::PublicManifest;
use crate::strategies::mechanistic::{
    ClosePolicy, DecisionMode, EntryDirection, EntryRule, MechanisticConfig,
};
use crate::strategies::risk::RiskPreset;
use crate::strategies::validate::validate_strategy;
use crate::strategies::{agent_ref::PipelineDef, BriefingIndicator, Strategy};

// ── Public output types ───────────────────────────────────────────────────────

/// An AST node that could not be mapped to any xvision filter/mechanistic
/// concept, recorded for WU4 fidelity reporting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct UnmappedNode {
    /// Human-readable explanation of why this node was not mapped.
    pub reason: String,
    /// The raw Pine Script text of the unsupported fragment.
    pub raw: String,
}

/// Result of mapping a parsed Pine Script AST to an xvision `Strategy`.
///
/// `strategy` is always a **valid** `Strategy` (passes `validate_strategy`).
/// `unmapped` records anything that was dropped or approximated.
/// `input_bindings` records provenance: `(input_var_name, tunable_path)` for
/// each `input.*` variable that was traced to an optimizer mutation target.
/// Fed to WU3 `input_mutation_targets`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MapOutcome {
    /// The mapped strategy. Always valid.
    pub strategy: Strategy,
    /// Nodes that could not be mapped deterministically. Fed to WU4.
    pub unmapped: Vec<UnmappedNode>,
    /// Provenance bindings: each entry is `(input_var_name, dotted_path)` where
    /// `dotted_path` is the optimizer mutation path the input knob feeds.
    /// - Filter numeric: `"conditions.<i>.rhs.numeric"`
    /// - Mechanistic stop/profit/trail: `"mechanistic.close_policies.<i>.pct"`
    /// - Mechanistic time exit: `"mechanistic.close_policies.<i>.bars"`
    /// - Mechanistic target PnL: `"mechanistic.close_policies.<i>.usd"`
    /// Inputs that do not bind to any tunable path are absent from this list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub input_bindings: Vec<(String, String)>,
}

// ── Internal state ────────────────────────────────────────────────────────────

/// A resolved indicator binding extracted from a `TaAssignment` or inline
/// `TaCall` expression.
#[derive(Debug, Clone)]
struct IndicatorBinding {
    /// The Pine variable name that receives this indicator value.
    var_name: String,
    /// Resolved xvision indicator.
    indicator_ref: IndicatorRef,
    /// Original `ta.<name>` token (e.g. `"sma"`). Reserved for WU4 fidelity
    /// reporting; not consumed by WU2 itself.
    #[allow(dead_code)]
    ta_name: String,
}

// ── Mapping table: Pine ta.* → xvision IndicatorName ─────────────────────────

/// Map a `ta.*` function name to an `IndicatorName`, plus extract the period
/// from the args list.
///
/// Returns `None` when the function name is unknown (should be recorded as
/// unmapped) or the period argument is not a literal integer.
fn map_ta_call(ta_name: &str, args: &[Expr]) -> Option<IndicatorRef> {
    match ta_name {
        // Simple period-based indicators — first arg is the source (close/etc),
        // second arg is the period. Pine convention: ta.sma(source, length).
        "sma" | "ema" | "wma" | "hma" | "vwma" => {
            let name = match ta_name {
                "sma" => IndicatorName::Sma,
                "ema" => IndicatorName::Ema,
                "wma" => IndicatorName::Wma,
                "hma" => IndicatorName::Hma,
                "vwma" => IndicatorName::Vwma,
                _ => unreachable!(),
            };
            let period = extract_period_arg(args, 1)?;
            validate_period(name, period)?;
            Some(IndicatorRef::periodic(name, period))
        }

        // RSI: ta.rsi(source, length)
        "rsi" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Rsi, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Rsi, period))
        }

        // ATR: ta.atr(length)
        "atr" => {
            let period = extract_period_arg(args, 0)?;
            validate_period(IndicatorName::Atr, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Atr, period))
        }

        // Bollinger Bands: ta.bb(source, length, mult) — we extract all three
        // components but represent them as BbUpper/BbMiddle/BbLower with the
        // length as the period. Only BbMiddle is mapped (the mean); upper/lower
        // require the multiplier which we drop here.
        "bb" | "bbands" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::BbMiddle, period)?;
            Some(IndicatorRef::periodic(IndicatorName::BbMiddle, period))
        }

        // MACD: ta.macd(source, fast, slow, signal) — standard defaults 12/26/9.
        // The xvision MACD indicators are periodless (fixed 12/26/9).
        "macd" => Some(IndicatorRef {
            name: IndicatorName::MacdLine,
            period: None,
            bar_offset: None,
        }),

        // ADX and DI: ta.dmi(length, smoothing)
        "dmi" | "adx" => {
            let period = extract_period_arg(args, 0)?;
            validate_period(IndicatorName::Adx, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Adx, period))
        }

        // Stochastic: ta.stoch(close, high, low, length)
        "stoch" => {
            let period = extract_period_arg(args, 3)?;
            validate_period(IndicatorName::StochK, period)?;
            Some(IndicatorRef::periodic(IndicatorName::StochK, period))
        }

        // CCI: ta.cci(source, length)
        "cci" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Cci, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Cci, period))
        }

        // MFI: ta.mfi(source, length)
        "mfi" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Mfi, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Mfi, period))
        }

        // ROC: ta.roc(source, length)
        "roc" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Roc, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Roc, period))
        }

        // Highest/Lowest: ta.highest(source, length) / ta.lowest(source, length)
        "highest" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Highest, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Highest, period))
        }
        "lowest" => {
            let period = extract_period_arg(args, 1)?;
            validate_period(IndicatorName::Lowest, period)?;
            Some(IndicatorRef::periodic(IndicatorName::Lowest, period))
        }

        // SuperTrend: ta.supertrend(factor, atr_period)
        // Packed: atr_period * 1000 + factor_times_10
        "supertrend" => {
            let atr_period = extract_period_arg(args, 1)?;
            let factor_times_10 = extract_float_arg(args, 0)
                .map(|f| (f * 10.0).round() as u32)
                .unwrap_or(30);
            let packed = atr_period * 1000 + factor_times_10;
            if !(2001..=200_200).contains(&packed) {
                return None;
            }
            Some(IndicatorRef::periodic(IndicatorName::SuperTrend, packed))
        }

        // Pivot high/low: pivothigh(source, left, right) / pivotlow(source, left, right)
        "pivothigh" | "ta.pivothigh" => {
            let left = extract_period_arg(args, 1)?;
            let right = extract_period_arg(args, 2)?;
            let packed = left * 1000 + right;
            if !(1001..=100_100).contains(&packed) {
                return None;
            }
            Some(IndicatorRef::periodic(IndicatorName::PivotHigh, packed))
        }
        "pivotlow" | "ta.pivotlow" => {
            let left = extract_period_arg(args, 1)?;
            let right = extract_period_arg(args, 2)?;
            let packed = left * 1000 + right;
            if !(1001..=100_100).contains(&packed) {
                return None;
            }
            Some(IndicatorRef::periodic(IndicatorName::PivotLow, packed))
        }

        // crossover / crossunder: these are relational, not indicator values.
        // We handle them specially in the condition mapping, not here.
        "crossover" | "crossunder" => None,

        // Unknown ta.* → caller records as unmapped
        _ => None,
    }
}

/// Validate that a period is within the indicator's allowed bounds.
/// Returns `None` (reject) if out of range.
fn validate_period(name: IndicatorName, period: u32) -> Option<u32> {
    if let Some((lo, hi)) = name.period_bounds() {
        if (lo..=hi).contains(&period) {
            Some(period)
        } else {
            None
        }
    } else {
        // periodless indicator — period was incorrectly supplied; reject
        None
    }
}

/// Extract an integer period argument from a position in the args list.
/// Returns `None` if the argument is missing, not a literal, or not in a
/// valid range for a period (2..=500 sanity guard).
fn extract_period_arg(args: &[Expr], pos: usize) -> Option<u32> {
    let expr = args.get(pos)?;
    match expr {
        Expr::IntLit { value } if *value >= 2 && *value <= 500 => Some(*value as u32),
        Expr::FloatLit { value } if *value >= 2.0 && *value <= 500.0 => Some(*value as u32),
        // Variable reference — period is dynamic (input knob); we cannot resolve it at import time.
        // Return None so the caller can decide to harvest it as a briefing indicator instead.
        _ => None,
    }
}

/// Extract a float argument from a position in the args list.
fn extract_float_arg(args: &[Expr], pos: usize) -> Option<f64> {
    let expr = args.get(pos)?;
    match expr {
        Expr::FloatLit { value } => Some(*value),
        Expr::IntLit { value } => Some(*value as f64),
        _ => None,
    }
}

// ── Strategy string arg extraction ───────────────────────────────────────────

/// Extract a string literal from a named or positional arg list.
fn extract_str_arg(args: &[(Option<String>, Expr)], key: &str, pos: usize) -> Option<String> {
    // Try named first.
    for (name, expr) in args {
        if name.as_deref() == Some(key) {
            if let Expr::StrLit { value } = expr {
                return Some(value.clone());
            }
        }
    }
    // Fall back to positional.
    args.get(pos).and_then(|(_, expr)| {
        if let Expr::StrLit { value } = expr {
            Some(value.clone())
        } else {
            None
        }
    })
}

/// Extract a numeric (float or int) from a named or positional arg list.
fn extract_num_arg(args: &[(Option<String>, Expr)], key: &str, pos: usize) -> Option<f64> {
    for (name, expr) in args {
        if name.as_deref() == Some(key) {
            return match expr {
                Expr::FloatLit { value } => Some(*value),
                Expr::IntLit { value } => Some(*value as f64),
                _ => None,
            };
        }
    }
    args.get(pos).and_then(|(_, expr)| match expr {
        Expr::FloatLit { value } => Some(*value),
        Expr::IntLit { value } => Some(*value as f64),
        _ => None,
    })
}

/// Determine whether a `strategy.entry` / `strategy.long` arg indicates a Long
/// or Short direction. Returns `None` when the direction cannot be determined.
fn extract_entry_direction(args: &[(Option<String>, Expr)]) -> Option<EntryDirection> {
    // Positional arg 1 (0-indexed) is `strategy.long` or `strategy.short` or a
    // string "long"/"short". The parser represents `strategy.long` / `strategy.short`
    // as `Ident { name: "strategy.long" }` or as a `StrategyCall`.
    // We also handle `Ident { name: "strategy.long" }` from TaCall / Ident nodes.
    let dir_expr = args.get(1).map(|(_, e)| e);
    match dir_expr {
        Some(Expr::Ident { name }) => match name.as_str() {
            "strategy.long" | "long" => Some(EntryDirection::Long),
            "strategy.short" | "short" => Some(EntryDirection::Short),
            _ => None,
        },
        Some(Expr::StrLit { value }) => match value.to_lowercase().as_str() {
            "long" => Some(EntryDirection::Long),
            "short" => Some(EntryDirection::Short),
            _ => None,
        },
        _ => {
            // Try named arg `direction=`
            for (name, expr) in args {
                if name.as_deref() == Some("direction") {
                    return match expr {
                        Expr::Ident { name } => match name.as_str() {
                            "strategy.long" | "long" => Some(EntryDirection::Long),
                            "strategy.short" | "short" => Some(EntryDirection::Short),
                            _ => None,
                        },
                        Expr::StrLit { value } => match value.to_lowercase().as_str() {
                            "long" => Some(EntryDirection::Long),
                            "short" => Some(EntryDirection::Short),
                            _ => None,
                        },
                        _ => None,
                    };
                }
            }
            // Heuristic: entry id often contains "Long" or "Short"
            if let Some(id) = extract_str_arg(args, "id", 0) {
                let id_lower = id.to_lowercase();
                if id_lower.contains("long") {
                    return Some(EntryDirection::Long);
                }
                if id_lower.contains("short") {
                    return Some(EntryDirection::Short);
                }
            }
            None
        }
    }
}

// ── Condition mapping ─────────────────────────────────────────────────────────

/// Attempt to map a Pine expression (typically a BinOp comparison) to an
/// xvision `Condition`.
///
/// Returns `Some(Condition)` on success. `None` means the expression is too
/// complex to reduce to a single condition (goes to fuzzy / unmapped path).
fn map_expr_to_condition(expr: &Expr, indicator_table: &[IndicatorBinding]) -> Option<Condition> {
    match expr {
        Expr::BinOp { op, left, right } => {
            // Comparison operators that map to Operator variants.
            let xvision_op = match op.as_str() {
                ">" => Some(Operator::Gt),
                "<" => Some(Operator::Lt),
                ">=" => Some(Operator::Gte),
                "<=" => Some(Operator::Lte),
                "==" => Some(Operator::Eq),
                _ => None,
            };
            if let Some(op) = xvision_op {
                let lhs = map_expr_to_operand(left, indicator_table)?;
                // lhs must be an indicator for the filter validator
                if !matches!(lhs, Operand::Indicator(_)) {
                    return None;
                }
                let rhs = map_expr_to_operand(right, indicator_table)?;
                // rhs must be indicator or numeric (not range) for these ops
                if matches!(rhs, Operand::Range(_, _)) {
                    return None;
                }
                let cond = Condition { lhs, op, rhs };
                return Some(cond);
            }
            None
        }
        // ta.crossover(a, b) → CrossesAbove
        Expr::TaCall { name, args } if name == "crossover" => {
            let lhs_expr = args.first()?;
            let rhs_expr = args.get(1)?;
            let lhs = map_expr_to_operand(lhs_expr, indicator_table)?;
            if !matches!(lhs, Operand::Indicator(_)) {
                return None;
            }
            let rhs = map_expr_to_operand(rhs_expr, indicator_table)?;
            if matches!(rhs, Operand::Range(_, _)) {
                return None;
            }
            Some(Condition {
                lhs,
                op: Operator::CrossesAbove,
                rhs,
            })
        }
        // ta.crossunder(a, b) → CrossesBelow
        Expr::TaCall { name, args } if name == "crossunder" => {
            let lhs_expr = args.first()?;
            let rhs_expr = args.get(1)?;
            let lhs = map_expr_to_operand(lhs_expr, indicator_table)?;
            if !matches!(lhs, Operand::Indicator(_)) {
                return None;
            }
            let rhs = map_expr_to_operand(rhs_expr, indicator_table)?;
            if matches!(rhs, Operand::Range(_, _)) {
                return None;
            }
            Some(Condition {
                lhs,
                op: Operator::CrossesBelow,
                rhs,
            })
        }
        _ => None,
    }
}

/// Map a Pine expression to an `Operand`.
///
/// - Ident that refers to a known indicator variable → `Operand::Indicator`
/// - Numeric literals → `Operand::Numeric`
/// - Inline `ta.*` calls → `Operand::Indicator` if they map cleanly
/// - Ident `close` / `open` / `high` / `low` / `volume` → `Operand::Indicator`
/// - Anything else → `None` (complex expression, cannot reduce)
fn map_expr_to_operand(expr: &Expr, indicator_table: &[IndicatorBinding]) -> Option<Operand> {
    match expr {
        Expr::FloatLit { value } => Some(Operand::Numeric(*value)),
        Expr::IntLit { value } => Some(Operand::Numeric(*value as f64)),

        Expr::Ident { name } => {
            // Known indicator variable from table?
            if let Some(binding) = indicator_table.iter().find(|b| &b.var_name == name) {
                return Some(Operand::Indicator(binding.indicator_ref.clone()));
            }
            // Pine built-ins that map directly to periodless indicators
            match name.as_str() {
                "close" => Some(Operand::Indicator(IndicatorRef::close())),
                "open" => Some(Operand::Indicator(IndicatorRef {
                    name: IndicatorName::Open,
                    period: None,
                    bar_offset: None,
                })),
                "high" => Some(Operand::Indicator(IndicatorRef {
                    name: IndicatorName::High,
                    period: None,
                    bar_offset: None,
                })),
                "low" => Some(Operand::Indicator(IndicatorRef {
                    name: IndicatorName::Low,
                    period: None,
                    bar_offset: None,
                })),
                "volume" => Some(Operand::Indicator(IndicatorRef {
                    name: IndicatorName::Volume,
                    period: None,
                    bar_offset: None,
                })),
                _ => None,
            }
        }

        Expr::Paren { inner } => map_expr_to_operand(inner, indicator_table),

        // Inline ta.* call — try to map it directly
        Expr::TaCall { name, args } => map_ta_call(name, args).map(Operand::Indicator),

        _ => None,
    }
}

// ── Close policy mapping ──────────────────────────────────────────────────────

/// Map `strategy.exit` args to one or more `ClosePolicy` entries.
/// `loss=` / `stop=` → StopLoss; `profit=` / `limit=` → TakeProfit; `trail_*` → TrailingStop.
fn map_exit_to_close_policies(args: &[(Option<String>, Expr)]) -> Vec<ClosePolicy> {
    let mut policies = Vec::new();

    // StopLoss: loss= (a percentage) or stop= (a fixed price — we only handle
    // the percentage form since xvision's StopLoss.pct is a percentage).
    let loss_pct = extract_num_arg(args, "loss", usize::MAX);
    if let Some(pct) = loss_pct {
        if pct > 0.0 && pct <= 100.0 {
            policies.push(ClosePolicy::StopLoss { pct });
        }
    }

    // TakeProfit: profit= (percentage) or limit= (fixed price, percentage form).
    let profit_pct = extract_num_arg(args, "profit", usize::MAX);
    if let Some(pct) = profit_pct {
        if pct > 0.0 && pct <= 1000.0 {
            policies.push(ClosePolicy::TakeProfit { pct });
        }
    }

    // TrailingStop: trail_points= or trail_price= — we approximate as a
    // percentage by noting the field type. Pine's `trail_offset` (in price
    // points) cannot be losslessly converted; we record it as TrailingStop
    // only when `trail_percent` is directly available.
    let trail_pct = extract_num_arg(args, "trail_percent", usize::MAX);
    if let Some(pct) = trail_pct {
        if pct > 0.0 && pct <= 100.0 {
            policies.push(ClosePolicy::TrailingStop { pct });
        }
    }

    policies
}

// ── Default strategy scaffold ─────────────────────────────────────────────────

/// Build the fixed parts of the scaffold strategy manifest.
/// The asset_universe defaults to `["BTC/USD"]` (a commonly-targeted asset in
/// TradingView scripts) so `validate_strategy` does not reject an empty universe.
fn make_scaffold_manifest(title: Option<&str>) -> PublicManifest {
    PublicManifest {
        id: Ulid::new().to_string(),
        display_name: title.unwrap_or("Imported Pine Strategy").to_string(),
        plain_summary: "Imported from Pine Script v5".to_string(),
        creator: "@pine-import".to_string(),
        template: "pine_import".to_string(),
        regime_fit: vec![],
        asset_universe: vec!["BTC/USD".to_string()],
        decision_cadence_minutes: 60,
        timeframe_requirements: Default::default(),
        attested_with: vec![],
        required_tools: vec![],
        risk_preset_or_config: "balanced".to_string(),
        published_at: None,
        min_warmup_bars: None,
        color: None,
        execution_mode: Default::default(),
        capital_mode: Default::default(),
    }
}

// ── Pass-2 statement mapper ───────────────────────────────────────────────────

/// Map a single statement in pass-2, populating filter conditions, entry rules,
/// and close policies.
///
/// `guard_condition` is `Some(cond)` when this statement is inside an `if` block
/// whose guard has already been successfully mapped to a filter condition. In that
/// case, the guard condition is applied to any `strategy.entry` or `strategy.exit`
/// call found inside the body (as the filter condition for that entry rule).
#[allow(clippy::too_many_arguments)]
fn map_stmt(
    stmt: &Statement,
    indicator_table: &[IndicatorBinding],
    input_var_names: &std::collections::HashSet<String>,
    filter_conditions: &mut Vec<Condition>,
    entry_rules: &mut Vec<EntryRule>,
    close_policies: &mut Vec<ClosePolicy>,
    fuzzy_indicators: &mut Vec<BriefingIndicator>,
    unmapped: &mut Vec<UnmappedNode>,
    condition_input_refs: &mut Vec<(String, usize)>,
    exit_input_refs: &mut Vec<(String, &'static str)>,
    guard_condition: Option<Condition>,
) {
    match stmt {
        Statement::Assignment { name, value, is_var } => {
            if let Some(cond) = map_expr_to_condition(value, indicator_table) {
                if validate_condition_ok(&cond) {
                    let cond_idx = filter_conditions.len();
                    if let Expr::BinOp { right, .. } = value {
                        if let Expr::Ident { name: rhs_name } = right.as_ref() {
                            if input_var_names.contains(rhs_name) {
                                condition_input_refs.push((rhs_name.clone(), cond_idx));
                            }
                        }
                    }
                    filter_conditions.push(cond);
                } else {
                    harvest_condition_as_briefing(value, indicator_table, fuzzy_indicators, unmapped);
                }
            } else if *is_var {
                harvest_expr_as_briefing(value, indicator_table, fuzzy_indicators);
                unmapped.push(UnmappedNode {
                    reason: "var declaration with complex expression; mapped as briefing indicator"
                        .to_string(),
                    raw: format!("var {name} = ..."),
                });
            } else {
                harvest_expr_as_briefing(value, indicator_table, fuzzy_indicators);
                if expr_references_indicator(value, indicator_table) {
                    unmapped.push(UnmappedNode {
                        reason: "Assignment expression too complex to reduce to a filter condition"
                            .to_string(),
                        raw: format!("{name} = ..."),
                    });
                }
            }
        }

        Statement::StrategyEntry { args } => {
            let signal_name = extract_str_arg(args, "id", 0).unwrap_or_else(|| "entry".to_string());
            let direction = extract_entry_direction(args).unwrap_or(EntryDirection::Long);
            // If a guard condition was passed in, add it to filter_conditions now.
            if let Some(guard) = guard_condition {
                let cond_idx = filter_conditions.len();
                // WU3: check if guard RHS was an input-variable reference.
                // We don't have the original expr here, so we can't detect it.
                // The guard binding is handled upstream in the If branch.
                let _ = cond_idx;
                filter_conditions.push(guard);
            }
            entry_rules.push(EntryRule {
                signal_name,
                direction,
            });
        }

        Statement::StrategyExit { args } => {
            let policies = map_exit_to_close_policies(args);
            if policies.is_empty() {
                collect_exit_input_refs(args, input_var_names, exit_input_refs, close_policies);
                if close_policies.is_empty() {
                    unmapped.push(UnmappedNode {
                        reason: "strategy.exit with no mappable loss/profit/trail args".to_string(),
                        raw: "strategy.exit(...)".to_string(),
                    });
                }
            } else {
                close_policies.extend(policies);
                scan_exit_input_refs_non_empty(args, input_var_names, exit_input_refs);
            }
        }

        Statement::If { condition, body } => {
            // Try to map the guard expression to a filter condition.
            let guard = map_expr_to_condition(condition, indicator_table).filter(validate_condition_ok);

            // WU3: if guard RHS references an input variable, record the binding.
            // We check the condition expression for an input-var Ident on the RHS.
            if let Some(ref g) = guard {
                // If there's already a valid condition we'll push it per-entry below.
                // Pre-check for input binding on RHS of the guard expression.
                if let Expr::BinOp { right, .. } = condition {
                    if let Expr::Ident { name: rhs_name } = right.as_ref() {
                        if input_var_names.contains(rhs_name) {
                            // The guard will be pushed to filter_conditions when the
                            // entry is mapped; record the binding at the prospective index.
                            // (Approximate: index = current filter_conditions.len())
                            let prospective_idx = filter_conditions.len();
                            condition_input_refs.push((rhs_name.clone(), prospective_idx));
                        }
                    }
                }
                let _ = g;
            }

            if guard.is_none() {
                // Fuzzy guard — harvest any indicators from the condition for briefing.
                harvest_expr_as_briefing(condition, indicator_table, fuzzy_indicators);
            }

            // Process body statements.
            for body_stmt in body {
                map_stmt(
                    body_stmt,
                    indicator_table,
                    input_var_names,
                    filter_conditions,
                    entry_rules,
                    close_policies,
                    fuzzy_indicators,
                    unmapped,
                    condition_input_refs,
                    exit_input_refs,
                    guard.clone(), // pass guard to each body statement
                );
            }
        }

        _ => {}
    }
}

// ── Statement walking helpers ─────────────────────────────────────────────────

/// Collect indicator bindings from a single statement, recursing into
/// `Statement::If` bodies so that `ta.*` assignments inside if blocks are
/// also added to the indicator table.
fn collect_indicator_bindings_from_stmt(
    stmt: &Statement,
    indicator_table: &mut Vec<IndicatorBinding>,
    fuzzy_indicators: &mut Vec<BriefingIndicator>,
    unmapped: &mut Vec<UnmappedNode>,
) {
    match stmt {
        Statement::TaAssignment { name, ta_name, args } => {
            if let Some(ind_ref) = map_ta_call(ta_name, args) {
                indicator_table.push(IndicatorBinding {
                    var_name: name.clone(),
                    indicator_ref: ind_ref.clone(),
                    ta_name: ta_name.clone(),
                });
            } else {
                match ta_name.as_str() {
                    "crossover" | "crossunder" => {
                        // Relational helpers — handled in predicate walk.
                    }
                    _ => {
                        if let Some(bi_name) = ta_name_to_indicator_name_lossy(ta_name) {
                            let period = extract_period_arg_lossy(args);
                            fuzzy_indicators.push(BriefingIndicator {
                                name: bi_name,
                                params: period.map(|p| vec![p as f64]).unwrap_or_default(),
                                source_token: name.clone(),
                            });
                        } else {
                            unmapped.push(UnmappedNode {
                                reason: format!("Unknown ta.* function: ta.{ta_name}"),
                                raw: format!("{name} = ta.{ta_name}(...)"),
                            });
                        }
                    }
                }
            }
        }
        Statement::If { body, .. } => {
            // Recurse into the body so ta.* assignments inside `if` blocks
            // are also added to the indicator table.
            for body_stmt in body {
                collect_indicator_bindings_from_stmt(body_stmt, indicator_table, fuzzy_indicators, unmapped);
            }
        }
        Statement::Unsupported { raw, .. } => {
            unmapped.push(UnmappedNode {
                reason: "Unsupported Pine construct".to_string(),
                raw: raw.clone(),
            });
        }
        _ => {}
    }
}

// ── Main mapper ───────────────────────────────────────────────────────────────

/// Map a parsed `PineScript` AST to a valid xvision `Strategy`.
///
/// See module-level docs for the full mapping strategy. The returned
/// `MapOutcome::strategy` always passes `validate_strategy`.
pub fn map_script(script: &PineScript) -> MapOutcome {
    let mut unmapped: Vec<UnmappedNode> = Vec::new();
    let mut input_bindings: Vec<(String, String)> = Vec::new();

    // Title from the header
    let title = script.header.as_ref().and_then(|h| h.title.as_deref());

    // Build a set of all declared input variable names for provenance tracking.
    let input_var_names: std::collections::HashSet<String> = script
        .statements
        .iter()
        .filter_map(|stmt| {
            if let Statement::Input { name, .. } = stmt {
                Some(name.clone())
            } else {
                None
            }
        })
        .collect();

    // ── Pass 1: collect indicator bindings from TaAssignment statements ────
    let mut indicator_table: Vec<IndicatorBinding> = Vec::new();
    let mut fuzzy_indicators: Vec<BriefingIndicator> = Vec::new();

    for stmt in &script.statements {
        collect_indicator_bindings_from_stmt(
            stmt,
            &mut indicator_table,
            &mut fuzzy_indicators,
            &mut unmapped,
        );
    }

    // ── Pass 2: collect conditions from assignments and strategy calls ─────
    let mut filter_conditions: Vec<Condition> = Vec::new();
    let mut entry_rules: Vec<EntryRule> = Vec::new();
    let mut close_policies: Vec<ClosePolicy> = Vec::new();

    // WU3 provenance: (input_var_name, leaf_token) collected during exit processing.
    // leaf_token is one of "pct", "bars", "usd" identifying the close-policy leaf.
    // After dedup we resolve these to final path indices.
    let mut exit_input_refs: Vec<(String, &'static str)> = Vec::new();

    // WU3 provenance: (input_var_name, condition_index) for filter conditions
    // where an input variable appears as the RHS numeric operand.
    // Condition index here is the index into `filter_conditions` at time of push.
    let mut condition_input_refs: Vec<(String, usize)> = Vec::new();

    for stmt in &script.statements {
        map_stmt(
            stmt,
            &indicator_table,
            &input_var_names,
            &mut filter_conditions,
            &mut entry_rules,
            &mut close_policies,
            &mut fuzzy_indicators,
            &mut unmapped,
            &mut condition_input_refs,
            &mut exit_input_refs,
            None, // no guard condition at top level
        );
    }

    // Deduplicate close_policies by kind.
    close_policies.dedup_by(|a, b| std::mem::discriminant(a) == std::mem::discriminant(b));

    // ── WU3: resolve input bindings to optimizer paths ─────────────────────────

    // Filter condition bindings: map (input_var, condition_idx) → "conditions.<i>.rhs.numeric"
    for (var_name, cond_idx) in &condition_input_refs {
        let path = format!("conditions.{cond_idx}.rhs.numeric");
        if !input_bindings.iter().any(|(v, p)| v == var_name && p == &path) {
            input_bindings.push((var_name.clone(), path));
        }
    }

    // Mechanistic close-policy bindings: (input_var, leaf) → "mechanistic.close_policies.<i>.<leaf>"
    // Match each (input_var, leaf) to the close_policy at the first index with that leaf type.
    for (var_name, leaf) in &exit_input_refs {
        let policy_idx = close_policies.iter().enumerate().find_map(|(i, p)| {
            let p_leaf: &'static str = match p {
                ClosePolicy::StopLoss { .. }
                | ClosePolicy::TakeProfit { .. }
                | ClosePolicy::TrailingStop { .. } => "pct",
                ClosePolicy::TimeExit { .. } => "bars",
                ClosePolicy::TargetPnl { .. } => "usd",
            };
            if p_leaf == *leaf {
                Some(i)
            } else {
                None
            }
        });
        if let Some(idx) = policy_idx {
            let path = format!("mechanistic.close_policies.{idx}.{leaf}");
            if !input_bindings.iter().any(|(v, _)| v == var_name) {
                input_bindings.push((var_name.clone(), path));
            }
        }
    }

    // ── Decide: Mechanistic vs Agentic ──────────────────────────────────────

    // Build a filter from any conditions we harvested.
    let filter = if !filter_conditions.is_empty() {
        let strategy_id = "pine-import";
        let f = Filter {
            id: FilterId::new(Ulid::new().to_string()),
            strategy_id: xvision_filters::StrategyId::new(strategy_id),
            display_name: "Imported Pine conditions".to_string(),
            description: None,
            status: FilterStatus::Draft,
            asset_scope: vec![Symbol::new("BTC/USD")],
            timeframe: Timeframe::new("1h"),
            scan_cadence: ScanCadence::BarClose,
            conditions: ConditionTree::All(filter_conditions.into_iter().map(ConditionItem::Leaf).collect()),
            fire: None,
            cooldown_bars: 0,
            max_wakeups_per_day: None,
            wake_when_in_position: xvision_filters::WakeInPosition::OnInvalidationOrTargetOnly,
            agent_context_template: xvision_filters::AgentContextTemplateId::new(
                xvision_filters::DEFAULT_AGENT_CONTEXT_TEMPLATE,
            ),
        };
        // Validate the filter — if it fails, discard it and harvest conditions as briefing.
        match validate_filter(&f) {
            Ok(()) => Some(f),
            Err(e) => {
                unmapped.push(UnmappedNode {
                    reason: format!("Filter validation failed: {e}; falling back to Agentic"),
                    raw: "filter conditions (all)".to_string(),
                });
                None
            }
        }
    } else {
        None
    };

    // Determine if we can go Mechanistic.
    let can_mechanistic = !entry_rules.is_empty();

    // Build the full briefing_indicators list from the indicator_table.
    // These are always useful for the LLM trader, whether the final mode is
    // Mechanistic or Agentic — in Mechanistic mode they serve as context the
    // agent-side system won't need, so we only carry them for Agentic.
    let all_briefing: Vec<BriefingIndicator> = {
        let mut binders: Vec<BriefingIndicator> = indicator_table
            .iter()
            .map(|b| BriefingIndicator {
                name: b.indicator_ref.name,
                params: b.indicator_ref.period.map(|p| vec![p as f64]).unwrap_or_default(),
                source_token: b.var_name.clone(),
            })
            .collect();
        binders.extend(fuzzy_indicators.iter().cloned());
        binders.dedup_by(|a, b| a.source_token == b.source_token);
        binders
    };

    // For Mechanistic to be valid we need mechanistic_config to be non-empty.
    // The close_policies may be empty (OK per validate_strategy — only
    // entry_rules non-empty is required).
    let (decision_mode, mechanistic_config, briefing_indicators, final_filter) = if can_mechanistic {
        // Mechanistic: record any fuzzy indicators in unmapped (not briefing).
        for bi in &fuzzy_indicators {
            unmapped.push(UnmappedNode {
                reason: format!(
                    "Indicator '{}' in fuzzy expression; not wired to filter condition",
                    bi.source_token
                ),
                raw: bi.source_token.clone(),
            });
        }
        let cfg = MechanisticConfig {
            entry_rules,
            close_policies,
        };
        (
            DecisionMode::Mechanistic,
            Some(cfg),
            Vec::new(), // Mechanistic strategies don't use briefing_indicators
            filter,
        )
    } else {
        // Agentic: populate briefing_indicators with all extracted indicators.
        // Agentic does not use a filter for decision gating (entry rules absent).
        (DecisionMode::Agentic, None, all_briefing, None)
    };

    // ── Build the Strategy ─────────────────────────────────────────────────
    let manifest = make_scaffold_manifest(title);
    let risk = RiskPreset::Balanced.expand();

    // Agentic strategies require at least one agent to pass `validate_strategy`.
    // For pine-import Agentic strategies we add a placeholder trader agent that
    // the operator replaces via the import wizard (WU6/WU8). The placeholder id
    // uses a well-known prefix so the frontend can detect and prompt for binding.
    let (agents, pipeline) = match decision_mode {
        DecisionMode::Agentic => {
            let placeholder = AgentRef {
                agent_id: format!("pine-import-placeholder-{}", Ulid::new()),
                role: "trader".to_string(),
                activates: None,
                prompt: String::new(),
                model_override: None,
                checkpoint: None,
                veto: None,
            };
            (vec![placeholder], PipelineDef::default())
        }
        DecisionMode::Mechanistic => (Vec::new(), PipelineDef::default()),
    };

    let strategy = Strategy {
        manifest,
        hypothesis: None,
        agents,
        pipeline,
        regime_slot: None,
        trader_slot: None,
        risk,
        activation_mode: ActivationMode::EveryBar,
        filter: final_filter,
        acknowledge_no_filter: true, // pine-import strategies don't require a filter gate
        decision_mode,
        mechanistic_config,
        briefing_indicators,
        tunable_bounds: Vec::new(),
    };

    // ── Validation safety net ──────────────────────────────────────────────
    // If the strategy is invalid (should not happen by construction), demote
    // all mechanistic config and return a minimal Agentic strategy.
    match validate_strategy(&strategy) {
        Ok(()) => MapOutcome {
            strategy,
            unmapped,
            input_bindings,
        },
        Err(e) => {
            unmapped.push(UnmappedNode {
                reason: format!("Strategy validation failed: {e:?}; demoted to minimal Agentic"),
                raw: "strategy (all)".to_string(),
            });
            let fallback_agent = AgentRef {
                agent_id: format!("pine-import-placeholder-{}", Ulid::new()),
                role: "trader".to_string(),
                activates: None,
                prompt: String::new(),
                model_override: None,
                checkpoint: None,
                veto: None,
            };
            let fallback = Strategy {
                manifest: make_scaffold_manifest(title),
                hypothesis: None,
                agents: vec![fallback_agent],
                pipeline: PipelineDef::default(),
                regime_slot: None,
                trader_slot: None,
                risk: RiskPreset::Balanced.expand(),
                activation_mode: ActivationMode::EveryBar,
                filter: None,
                acknowledge_no_filter: true,
                decision_mode: DecisionMode::Agentic,
                mechanistic_config: None,
                briefing_indicators: Vec::new(),
                tunable_bounds: Vec::new(),
            };
            // On fallback demote, input_bindings become meaningless (no mechanistic config).
            MapOutcome {
                strategy: fallback,
                unmapped,
                input_bindings: Vec::new(),
            }
        }
    }
}

// ── WU3 helpers: input provenance binding ─────────────────────────────────────

/// Scan `strategy.exit(...)` args for input-variable references. When an arg
/// that controls a close-policy scalar (loss, profit, trail_percent) is an
/// `Ident` that names an input variable, we:
///   1. Synthesize the `ClosePolicy` using the input's default value (to give the
///      optimizer a valid starting point — the default from the `input.*` call).
///   2. Record `(input_var_name, leaf)` in `exit_input_refs` for path resolution.
///
/// `input_var_names` is the set of all declared input variable names.
/// This is called ONLY when `map_exit_to_close_policies` returned no policies.
fn collect_exit_input_refs(
    args: &[(Option<String>, Expr)],
    input_var_names: &std::collections::HashSet<String>,
    exit_input_refs: &mut Vec<(String, &'static str)>,
    close_policies: &mut Vec<ClosePolicy>,
) {
    // We need the script to look up defaults; here we use a placeholder default
    // of 1.0 pct (always valid). The real default is resolved in inputs.rs via
    // the script's Input statement args. This call only establishes binding,
    // not the exact default value on the ClosePolicy.
    //
    // loss=<var> → StopLoss(1.0), leaf="pct"
    // profit=<var> → TakeProfit(1.0), leaf="pct"
    // trail_percent=<var> → TrailingStop(1.0), leaf="pct"
    for (name, expr) in args {
        let key = name.as_deref().unwrap_or("");
        if let Expr::Ident { name: var_name } = expr {
            if input_var_names.contains(var_name) {
                match key {
                    "loss" => {
                        if !close_policies
                            .iter()
                            .any(|p| matches!(p, ClosePolicy::StopLoss { .. }))
                        {
                            close_policies.push(ClosePolicy::StopLoss { pct: 1.0 });
                        }
                        if !exit_input_refs.iter().any(|(v, _)| v == var_name) {
                            exit_input_refs.push((var_name.clone(), "pct"));
                        }
                    }
                    "profit" => {
                        if !close_policies
                            .iter()
                            .any(|p| matches!(p, ClosePolicy::TakeProfit { .. }))
                        {
                            close_policies.push(ClosePolicy::TakeProfit { pct: 1.0 });
                        }
                        if !exit_input_refs.iter().any(|(v, _)| v == var_name) {
                            exit_input_refs.push((var_name.clone(), "pct"));
                        }
                    }
                    "trail_percent" => {
                        if !close_policies
                            .iter()
                            .any(|p| matches!(p, ClosePolicy::TrailingStop { .. }))
                        {
                            close_policies.push(ClosePolicy::TrailingStop { pct: 1.0 });
                        }
                        if !exit_input_refs.iter().any(|(v, _)| v == var_name) {
                            exit_input_refs.push((var_name.clone(), "pct"));
                        }
                    }
                    _ => {}
                }
            }
        }
    }
}

/// Scan `strategy.exit(...)` args for input-variable references when some policies
/// were ALREADY mapped from literal values. Records `(input_var, leaf)` for the
/// variable-valued args so they can be tracked for WU3 binding.
/// Unlike `collect_exit_input_refs`, this does NOT synthesize `ClosePolicy` entries.
fn scan_exit_input_refs_non_empty(
    args: &[(Option<String>, Expr)],
    input_var_names: &std::collections::HashSet<String>,
    exit_input_refs: &mut Vec<(String, &'static str)>,
) {
    for (name, expr) in args {
        let key = name.as_deref().unwrap_or("");
        if let Expr::Ident { name: var_name } = expr {
            if input_var_names.contains(var_name) {
                let leaf: Option<&'static str> = match key {
                    "loss" | "profit" | "trail_percent" => Some("pct"),
                    _ => None,
                };
                if let Some(leaf) = leaf {
                    if !exit_input_refs.iter().any(|(v, _)| v == var_name) {
                        exit_input_refs.push((var_name.clone(), leaf));
                    }
                }
            }
        }
    }
}

// ── Helpers for briefing indicator harvesting ─────────────────────────────────

/// Lossy `ta.*` name → `IndicatorName` for briefing harvest (fuzzy path).
/// Returns `None` for structural helpers (crossover, etc.) that aren't values.
fn ta_name_to_indicator_name_lossy(ta_name: &str) -> Option<IndicatorName> {
    match ta_name {
        "sma" => Some(IndicatorName::Sma),
        "ema" => Some(IndicatorName::Ema),
        "wma" => Some(IndicatorName::Wma),
        "hma" => Some(IndicatorName::Hma),
        "vwma" => Some(IndicatorName::Vwma),
        "rsi" => Some(IndicatorName::Rsi),
        "atr" => Some(IndicatorName::Atr),
        "adx" | "dmi" => Some(IndicatorName::Adx),
        "macd" => Some(IndicatorName::MacdLine),
        "cci" => Some(IndicatorName::Cci),
        "mfi" => Some(IndicatorName::Mfi),
        "roc" => Some(IndicatorName::Roc),
        "bb" | "bbands" => Some(IndicatorName::BbMiddle),
        "stoch" => Some(IndicatorName::StochK),
        "highest" => Some(IndicatorName::Highest),
        "lowest" => Some(IndicatorName::Lowest),
        "supertrend" => Some(IndicatorName::SuperTrend),
        "pivothigh" => Some(IndicatorName::PivotHigh),
        "pivotlow" => Some(IndicatorName::PivotLow),
        _ => None,
    }
}

/// Lossy period extraction — tries position 0 then 1, falls back to None.
fn extract_period_arg_lossy(args: &[Expr]) -> Option<u32> {
    extract_period_arg(args, 1).or_else(|| extract_period_arg(args, 0))
}

/// Check whether a `Condition` is valid for the filter validator without
/// actually constructing the full Filter (saves allocations).
fn validate_condition_ok(cond: &Condition) -> bool {
    // lhs must be Indicator
    if !matches!(cond.lhs, Operand::Indicator(_)) {
        return false;
    }
    // rhs must not be Range for non-Between operators
    if matches!(cond.rhs, Operand::Range(_, _)) && !matches!(cond.op, Operator::Between) {
        return false;
    }
    // RSI lhs: rhs numeric must be in [0, 100]
    if let (Operand::Indicator(ind), Operand::Numeric(v)) = (&cond.lhs, &cond.rhs) {
        match ind.name {
            IndicatorName::Rsi | IndicatorName::Adx | IndicatorName::StochK | IndicatorName::StochD => {
                if !(*v >= 0.0 && *v <= 100.0) {
                    return false;
                }
            }
            _ => {}
        }
    }
    // Period bounds on the lhs indicator.
    if let Operand::Indicator(ind) = &cond.lhs {
        if let Some((lo, hi)) = ind.name.period_bounds() {
            if let Some(p) = ind.period {
                if !(lo..=hi).contains(&p) {
                    return false;
                }
            } else if ind.name.has_period() {
                return false; // period required but absent
            }
        }
    }
    true
}

/// Harvest any indicator references from a complex expression (ternary,
/// compound arithmetic, etc.) into the briefing_indicators list.
fn harvest_expr_as_briefing(
    expr: &Expr,
    indicator_table: &[IndicatorBinding],
    briefing: &mut Vec<BriefingIndicator>,
) {
    match expr {
        Expr::Ident { name } => {
            if let Some(b) = indicator_table.iter().find(|b| &b.var_name == name) {
                let already = briefing.iter().any(|bi| bi.source_token == b.var_name);
                if !already {
                    briefing.push(BriefingIndicator {
                        name: b.indicator_ref.name,
                        params: b.indicator_ref.period.map(|p| vec![p as f64]).unwrap_or_default(),
                        source_token: b.var_name.clone(),
                    });
                }
            }
        }
        Expr::TaCall { name, args } => {
            if let Some(ind_ref) = map_ta_call(name, args) {
                let token = format!("ta.{name}");
                let already = briefing.iter().any(|bi| bi.source_token == token);
                if !already {
                    briefing.push(BriefingIndicator {
                        name: ind_ref.name,
                        params: ind_ref.period.map(|p| vec![p as f64]).unwrap_or_default(),
                        source_token: token,
                    });
                }
            } else if let Some(ind_name) = ta_name_to_indicator_name_lossy(name) {
                let period = extract_period_arg_lossy(args);
                let token = format!("ta.{name}");
                let already = briefing.iter().any(|bi| bi.source_token == token);
                if !already {
                    briefing.push(BriefingIndicator {
                        name: ind_name,
                        params: period.map(|p| vec![p as f64]).unwrap_or_default(),
                        source_token: token,
                    });
                }
            }
        }
        Expr::BinOp { left, right, .. } => {
            harvest_expr_as_briefing(left, indicator_table, briefing);
            harvest_expr_as_briefing(right, indicator_table, briefing);
        }
        Expr::Ternary { cond, then_, else_ } => {
            harvest_expr_as_briefing(cond, indicator_table, briefing);
            harvest_expr_as_briefing(then_, indicator_table, briefing);
            harvest_expr_as_briefing(else_, indicator_table, briefing);
        }
        Expr::Paren { inner } => harvest_expr_as_briefing(inner, indicator_table, briefing),
        Expr::Not { expr } => harvest_expr_as_briefing(expr, indicator_table, briefing),
        _ => {}
    }
}

/// Harvest any indicator references from a failed condition into briefing_indicators.
fn harvest_condition_as_briefing(
    expr: &Expr,
    indicator_table: &[IndicatorBinding],
    briefing: &mut Vec<BriefingIndicator>,
    unmapped: &mut Vec<UnmappedNode>,
) {
    harvest_expr_as_briefing(expr, indicator_table, briefing);
    unmapped.push(UnmappedNode {
        reason: "Condition expression failed filter validation; harvested as briefing indicator".to_string(),
        raw: "condition expr".to_string(),
    });
}

/// Returns `true` if the expression references any known indicator binding.
fn expr_references_indicator(expr: &Expr, indicator_table: &[IndicatorBinding]) -> bool {
    match expr {
        Expr::Ident { name } => indicator_table.iter().any(|b| &b.var_name == name),
        Expr::TaCall { .. } => true,
        Expr::BinOp { left, right, .. } => {
            expr_references_indicator(left, indicator_table)
                || expr_references_indicator(right, indicator_table)
        }
        Expr::Ternary { cond, then_, else_ } => {
            expr_references_indicator(cond, indicator_table)
                || expr_references_indicator(then_, indicator_table)
                || expr_references_indicator(else_, indicator_table)
        }
        Expr::Paren { inner } => expr_references_indicator(inner, indicator_table),
        Expr::Not { expr } => expr_references_indicator(expr, indicator_table),
        _ => false,
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use xvision_filters::IndicatorName;

    #[test]
    fn map_ta_call_sma_with_literal_period() {
        let args = vec![
            Expr::Ident {
                name: "close".to_string(),
            },
            Expr::IntLit { value: 20 },
        ];
        let result = map_ta_call("sma", &args);
        assert!(result.is_some());
        let r = result.unwrap();
        assert_eq!(r.name, IndicatorName::Sma);
        assert_eq!(r.period, Some(20));
    }

    #[test]
    fn map_ta_call_rsi_with_literal_period() {
        let args = vec![
            Expr::Ident {
                name: "close".to_string(),
            },
            Expr::IntLit { value: 14 },
        ];
        let result = map_ta_call("rsi", &args);
        assert_eq!(result.unwrap().name, IndicatorName::Rsi);
    }

    #[test]
    fn map_ta_call_unknown_returns_none() {
        let result = map_ta_call("some_custom_func", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn map_ta_call_crossover_returns_none() {
        // crossover is handled as a condition operator, not an indicator value.
        let result = map_ta_call("crossover", &[]);
        assert!(result.is_none());
    }

    #[test]
    fn extract_entry_direction_long_from_ident() {
        let args = vec![
            (
                Some("id".to_string()),
                Expr::StrLit {
                    value: "Long".to_string(),
                },
            ),
            (
                None,
                Expr::Ident {
                    name: "strategy.long".to_string(),
                },
            ),
        ];
        assert_eq!(extract_entry_direction(&args), Some(EntryDirection::Long));
    }

    #[test]
    fn extract_entry_direction_short_from_ident() {
        let args = vec![
            (
                Some("id".to_string()),
                Expr::StrLit {
                    value: "Short".to_string(),
                },
            ),
            (
                None,
                Expr::Ident {
                    name: "strategy.short".to_string(),
                },
            ),
        ];
        assert_eq!(extract_entry_direction(&args), Some(EntryDirection::Short));
    }

    #[test]
    fn close_policy_stop_loss_from_loss_arg() {
        let args = vec![
            (
                Some("id".to_string()),
                Expr::StrLit {
                    value: "Exit".to_string(),
                },
            ),
            (Some("loss".to_string()), Expr::FloatLit { value: 2.0 }),
        ];
        let policies = map_exit_to_close_policies(&args);
        assert!(policies
            .iter()
            .any(|p| matches!(p, ClosePolicy::StopLoss { pct } if *pct == 2.0)));
    }

    #[test]
    fn close_policy_take_profit_from_profit_arg() {
        let args = vec![(Some("profit".to_string()), Expr::FloatLit { value: 4.0 })];
        let policies = map_exit_to_close_policies(&args);
        assert!(policies
            .iter()
            .any(|p| matches!(p, ClosePolicy::TakeProfit { pct } if *pct == 4.0)));
    }
}
