use std::sync::Arc;

use anyhow::Context;
use serde::{Deserialize, Serialize};
use xvision_filters::{Filter, Operand, Operator};

use crate::agent::llm::{LlmDispatch, LlmRequest, Message};
use crate::autooptimizer::config::AutoOptimizerConfig;
use crate::autooptimizer::content_hash::ContentHash;
use crate::autooptimizer::program_view;
use crate::autooptimizer::validator::{validate_mutation_diff, ValidationError};
use crate::strategies::Strategy;

const PROMPT_TEMPLATE: &str = include_str!("../../prompts/autooptimizer/mutator-v1.md");

/// Cortex namespace the mutator records gated candidate outcomes to and recalls
/// prior outcomes from. This is the **generalized cross-run/cross-framework**
/// memory layer: it surfaces what kinds of changes helped or hurt on similar
/// strategies in earlier runs so the experiment writer can build on wins and
/// avoid repeating failures.
///
/// It is **advisory only** and coexists with the F32 exact-repeat guarantee.
/// The hard avoid-set / `already_tried` lineage content hashes remain the
/// authoritative defence against proposing a byte-identical repeat; memory does
/// not replace or weaken that — it just generalizes the lesson across runs and
/// frameworks. Both layers stay: memory advises, the avoid-set governs exact
/// repeats. Subsurface (developer-facing) name per the autooptimizer
/// terminology lock; never collapses to bare `optimizer`.
pub const MUTATIONS_NS: &str = "autooptimizer:mutations";

/// Gate evidence recorded alongside an optimizer outcome memory line.
///
/// This is intentionally compact text input for the next experiment writer, not
/// an alternate gate. The deterministic gate remains authoritative; this just
/// keeps recalled memories from losing the real rejection dimension (holdout,
/// drawdown, trades, realized return) behind a generic `ΔSharpe` summary.
#[derive(Debug, Clone, Copy, Default)]
pub struct MutationGateContext<'a> {
    pub objective_label: &'a str,
    pub delta_day: Option<f64>,
    pub delta_holdout: Option<f64>,
    pub drawdown_ratio: Option<f64>,
    pub parent_n_trades: Option<u32>,
    pub child_n_trades: Option<u32>,
    pub min_trade_retention_ratio: Option<f64>,
    pub parent_realized_return_ratio: Option<f64>,
    pub child_realized_return_ratio: Option<f64>,
    pub min_realized_return_ratio: Option<f64>,
    pub reason: Option<&'a str>,
}

/// A compact, human-readable one-line summary of a gated candidate's outcome,
/// suitable for recording as an Observation in [`MUTATIONS_NS`] (and later
/// recall once distilled into a Pattern).
///
/// For a single-param change it reads e.g.
/// `param risk.stop_loss_atr_multiple 2.0→3.5 ⇒ ΔSharpe -0.40 (rejected)`; when
/// gate evidence is available, it appends the objective deltas, guard ratios, and
/// rejection reason so future proposal prompts learn the real failure mode.
/// Always a single line.
pub fn describe_mutation_outcome(
    diff: &MutationDiff,
    delta_sharpe: f64,
    status_label: &str,
    gate: Option<MutationGateContext<'_>>,
) -> String {
    let lever = match diff.kind {
        MutationKind::Param => {
            if let Some(p) = diff.params.first() {
                let extra = if diff.params.len() > 1 {
                    format!(" (+{} more)", diff.params.len() - 1)
                } else {
                    String::new()
                };
                format!(
                    "param {} {}→{}{}",
                    p.key,
                    compact_json(&p.before),
                    compact_json(&p.after),
                    extra
                )
            } else {
                "param (none)".to_string()
            }
        }
        MutationKind::Tool => {
            let added = diff.tools.added.join("+");
            let removed = diff.tools.removed.join("-");
            let mut parts = Vec::new();
            if !added.is_empty() {
                parts.push(format!("+{added}"));
            }
            if !removed.is_empty() {
                parts.push(format!("-{removed}"));
            }
            format!(
                "tool {}",
                if parts.is_empty() {
                    "(none)".to_string()
                } else {
                    parts.join(" ")
                }
            )
        }
        MutationKind::Prose => {
            let role = diff.prose.first().map(|p| p.agent_role.as_str()).unwrap_or("?");
            format!("prose {role}")
        }
        MutationKind::Filter => {
            if let Some(fe) = diff.filter.first() {
                let extra = if diff.filter.len() > 1 {
                    format!(" (+{} more)", diff.filter.len() - 1)
                } else {
                    String::new()
                };
                format!(
                    "filter {} {}→{}{}",
                    fe.path,
                    compact_json(&fe.before),
                    compact_json(&fe.after),
                    extra
                )
            } else {
                "filter (none)".to_string()
            }
        }
    };
    let mut summary = format!("{lever} ⇒ ΔSharpe {delta_sharpe:+.2} ({status_label})");
    if let Some(gate) = gate {
        summary.push_str(&format_gate_context(gate));
    }
    // Strip any embedded newlines defensively so the result is always one line.
    summary.replace('\n', " ")
}

fn format_gate_context(gate: MutationGateContext<'_>) -> String {
    let mut parts = Vec::new();
    if let Some(delta_day) = gate.delta_day {
        parts.push(format!("{} Δday {delta_day:+.4}", gate.objective_label));
    }
    if let Some(delta_holdout) = gate.delta_holdout {
        parts.push(format!("Δholdout {delta_holdout:+.4}"));
    }
    if let Some(drawdown_ratio) = gate.drawdown_ratio {
        parts.push(format!("drawdown {drawdown_ratio:.2}×"));
    }
    if let (Some(parent), Some(child), Some(ratio)) = (
        gate.parent_n_trades,
        gate.child_n_trades,
        gate.min_trade_retention_ratio,
    ) {
        parts.push(format!("trades {child}/{parent} (min {:.0}%)", ratio * 100.0));
    }
    if let (Some(child_rr), Some(min_rr)) = (gate.child_realized_return_ratio, gate.min_realized_return_ratio)
    {
        match gate.parent_realized_return_ratio {
            Some(parent_rr) => parts.push(format!(
                "realized child {child_rr:.2} vs parent {parent_rr:.2} (min {min_rr:.2})"
            )),
            None => parts.push(format!("realized child {child_rr:.2} (min {min_rr:.2})")),
        }
    }
    if let Some(reason) = gate.reason.map(compact_reason).filter(|s| !s.is_empty()) {
        parts.push(format!("reason: {reason}"));
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("; gate {}", parts.join(", "))
    }
}

fn compact_reason(reason: &str) -> String {
    const MAX_REASON_CHARS: usize = 240;
    let one_line = reason.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut out: String = one_line.chars().take(MAX_REASON_CHARS).collect();
    if one_line.chars().count() > MAX_REASON_CHARS {
        out.push('…');
    }
    out
}

/// Render a JSON scalar compactly for the outcome descriptor (drops quotes on
/// strings, prints numbers/bools/null plainly). Non-scalar values fall back to
/// their compact JSON form.
fn compact_json(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Null => "null".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MutationKind {
    Prose,
    Param,
    Tool,
    Filter,
}

impl MutationKind {
    /// Stable string label matching the serde serialization.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Prose => "prose",
            Self::Param => "param",
            Self::Tool => "tool",
            Self::Filter => "filter",
        }
    }
}

impl std::fmt::Display for MutationKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// One incremental change to a numeric threshold inside the strategy's typed
/// `Filter` AST, addressed by a stable dotted path (see `filter_tunable_paths`).
/// Examples:
///   `path = "conditions.0.rhs.numeric"`, `before = 25.0`, `after = 28.0`
///   `path = "conditions.0.op.within_pct"`, `before = 1.5`, `after = 2.0`
///   `path = "cooldown_bars"`, `before = 3`, `after = 6`
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FilterEdit {
    pub path: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProseEdit {
    pub agent_role: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ParamChange {
    pub key: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolDiff {
    pub added: Vec<String>,
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MutationDiff {
    pub kind: MutationKind,
    pub prose: Vec<ProseEdit>,
    pub params: Vec<ParamChange>,
    pub tools: ToolDiff,
    #[serde(default)]
    pub filter: Vec<FilterEdit>,
    /// vxn (structural filter creation): the full Filter JSON the experiment
    /// writer authored, INSTALLED when the parent strategy has no filter
    /// (`base.filter` is `None`). Distinct from `filter` (path-level edits that
    /// TUNE an existing filter). Materialized and validated through the same
    /// authoring parse/validate path operators use (`xvision_filters::validate`),
    /// so an invalid filter is rejected on a clean retry rather than reaching
    /// backtest. `None` (the common case) means "no structural filter change".
    /// Always serialized (as `null` when absent) so the wire shape matches the
    /// `create_filter`-required `mutation_diff` response schema.
    #[serde(default)]
    pub create_filter: Option<serde_json::Value>,
    pub rationale: String,
}

pub fn empty_mutation() -> MutationDiff {
    MutationDiff {
        kind: MutationKind::Prose,
        prose: Vec::new(),
        params: Vec::new(),
        tools: ToolDiff {
            added: Vec::new(),
            removed: Vec::new(),
        },
        filter: Vec::new(),
        create_filter: None,
        rationale: String::new(),
    }
}

/// Structural diff between two strategies computed by field comparison (not
/// LLM). Used by the Strategy Inspector UI's "diff from originating strategy"
/// panel to show what changed between a parent and its lineage descendant.
///
/// Mirrors [`MutationDiff`]'s shape but carries no `kind` or `rationale` —
/// those are LLM-proposal concepts. `StrategyDiff` is a pure data product.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct StrategyDiff {
    pub prose: Vec<ProseEdit>,
    pub params: Vec<ParamChange>,
    pub tools: ToolDiff,
    pub filter: Vec<FilterEdit>,
}

/// Compute a [`StrategyDiff`] by comparing strategy `a` (the originating /
/// parent) with strategy `b` (the descendant).
///
/// - **Prose**: walks `b.agents`; for each role present in both strategies,
///   emits a [`ProseEdit`] when `prompt_override` differs (treating `None` as
///   `""`).
/// - **Params**: always empty — scalar tuning flows through `risk.*` /
///   `mechanistic.*` keys proposed directly by the experiment writer, not via
///   a free-form params blob.
/// - **Tools**: always returns an empty [`ToolDiff`] — tools are managed at
///   the agent level, not the strategy level.
/// - **Filter**: recursively diffs numeric leaf values in the serialised filter
///   JSON; non-numeric leaves and structural differences are ignored.
pub fn strategy_diff(a: &Strategy, b: &Strategy) -> StrategyDiff {
    // ── Prose ──────────────────────────────────────────────────────────────
    let mut prose: Vec<ProseEdit> = Vec::new();
    for agent_b in &b.agents {
        let role_b = agent_b.canonical_role();
        let before_str = a
            .agents
            .iter()
            .find(|ag| ag.canonical_role() == role_b)
            .map(|ag| ag.prompt.as_str())
            .unwrap_or("")
            .to_string();
        let after_str = agent_b.prompt.clone();
        if before_str != after_str {
            prose.push(ProseEdit {
                agent_role: role_b,
                before: before_str,
                after: after_str,
            });
        }
    }

    // ── Params ─────────────────────────────────────────────────────────────
    // No free-form mechanical-params blob to diff any more. Scalar tuning now
    // flows exclusively through the surfaces the executor reads (risk.* and
    // mechanistic.* keys), which the LLM experiment writer proposes directly as
    // ParamChange entries; strategy_diff produces no scalar param edits.
    let params: Vec<ParamChange> = Vec::new();

    // ── Tools ──────────────────────────────────────────────────────────────
    // Tools are agent-level, not strategy-level; always empty here.
    let tools = ToolDiff {
        added: Vec::new(),
        removed: Vec::new(),
    };

    // ── Filter ─────────────────────────────────────────────────────────────
    let mut filter: Vec<FilterEdit> = Vec::new();
    let val_a = match &a.filter {
        Some(f) => serde_json::to_value(f).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    };
    let val_b = match &b.filter {
        Some(f) => serde_json::to_value(f).unwrap_or(serde_json::Value::Null),
        None => serde_json::Value::Null,
    };
    diff_filter_values(&val_a, &val_b, "", &mut filter);

    StrategyDiff {
        prose,
        params,
        tools,
        filter,
    }
}

/// Recursively walk two JSON values and emit a [`FilterEdit`] for every
/// differing `Number` leaf, addressed by a dotted `path`. Objects are
/// walked key-by-key; arrays are walked index-by-index. Non-numeric
/// leaves and structural differences (one side is an object, the other
/// is a scalar) are silently skipped — the function is a best-effort
/// numeric diff, not a full structural JSON differ.
fn diff_filter_values(a: &serde_json::Value, b: &serde_json::Value, path: &str, out: &mut Vec<FilterEdit>) {
    match (a, b) {
        (serde_json::Value::Object(map_a), serde_json::Value::Object(map_b)) => {
            for (key, val_b) in map_b {
                let child_path = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                let val_a = map_a.get(key).cloned().unwrap_or(serde_json::Value::Null);
                diff_filter_values(&val_a, val_b, &child_path, out);
            }
        }
        (serde_json::Value::Array(arr_a), serde_json::Value::Array(arr_b)) => {
            for (i, val_b) in arr_b.iter().enumerate() {
                let child_path = if path.is_empty() {
                    i.to_string()
                } else {
                    format!("{path}.{i}")
                };
                let val_a = arr_a.get(i).cloned().unwrap_or(serde_json::Value::Null);
                diff_filter_values(&val_a, val_b, &child_path, out);
            }
        }
        (val_a, val_b) if val_b.is_number() => {
            if val_a != val_b {
                out.push(FilterEdit {
                    path: path.to_string(),
                    before: val_a.clone(),
                    after: val_b.clone(),
                });
            }
        }
        // Non-numeric leaf or mismatched types — skip.
        _ => {}
    }
}

/// Numeric `RiskConfig` fields the mutator may tune via `risk.<field>` param
/// keys. F14/F20 (QA 2026-06-04): the real strategies on the node have no
/// mechanistic config, so their only tunable knobs live in `risk` — without
/// this the optimizer could never produce a valid param experiment for any
/// real strategy. Keep in sync with `RiskConfig` (xvision-engine strategies::risk).
///
/// Engine-level safety limits (`max_leverage`, `stop_loss_atr_multiple`,
/// `daily_loss_kill_pct`) are listed in [`PROTECTED_ENGINE_PARAMS`] and are
/// excluded from the tunable set — the mutator must NOT propose changes to
/// them. See `tunable_param_keys()` for the filtered output.
pub const RISK_PARAM_FIELDS: &[&str] = &[
    "risk_pct_per_trade",
    "max_concurrent_positions",
    "max_leverage",
    "stop_loss_atr_multiple",
    "daily_loss_kill_pct",
    "max_position_pct_nav",
];

/// Engine-level risk parameters that the optimizer MUST NOT mutate. These are
/// safety limits, not strategy logic — changing them shifts the risk profile
/// without improving decision quality. The mutator is prevented from proposing
/// changes to these via `tunable_param_keys()` filtering; the DSPy flywheel's
/// `lock_protected_tokens()` provides a second layer of defense.
///
/// These values are NEVER tunable — they are engine-level safety limits.
/// The optimizer should only tune decision logic, conviction thresholds,
/// signal interpretation, and action selection heuristics.
pub const PROTECTED_ENGINE_PARAMS: &[&str] =
    &["max_leverage", "stop_loss_atr_multiple", "daily_loss_kill_pct"];

/// If `key` addresses a tunable `risk` field — either `risk.<field>` or a bare
/// `<field>` naming a known risk knob — return the field name; otherwise `None`.
pub fn risk_field_for_key(_base: &Strategy, key: &str) -> Option<String> {
    if let Some(field) = key.strip_prefix("risk.") {
        return RISK_PARAM_FIELDS.contains(&field).then(|| field.to_string());
    }
    RISK_PARAM_FIELDS.contains(&key).then(|| key.to_string())
}

/// The param keys an experiment may target on `base`: `risk.<field>` for each
/// tunable risk knob (excluding engine-level safety limits in
/// [`PROTECTED_ENGINE_PARAMS`]), plus `mechanistic.close_policies.<i>.<leaf>`
/// for each tunable scalar in `mechanistic_config` (WU3a). These are the
/// surfaces the executor actually reads at decision time. Used to tell the
/// experiment writer which keys exist (F21) and to render a helpful
/// `unknown_param` error.
pub fn tunable_param_keys(base: &Strategy) -> Vec<String> {
    let mut keys = Vec::new();
    for f in RISK_PARAM_FIELDS {
        if PROTECTED_ENGINE_PARAMS.contains(f) {
            continue;
        }
        keys.push(format!("risk.{f}"));
    }
    // WU3a: mechanistic close-policy scalars.
    if let Some(mc) = &base.mechanistic_config {
        for (path, _) in mechanistic_tunable_paths(mc) {
            keys.push(path);
        }
    }
    keys
}

/// Walk the typed `Filter` AST and return a stable dotted path + current JSON
/// value for every mutatable numeric node:
///   - Each `Condition`'s `lhs`/`rhs` `Numeric(f64)` operands
///   - Each `Condition`'s `lhs`/`rhs` `Range(lo, hi)` — as `.range.lo` / `.range.hi`
///   - Each parameterized `Operator` argument (e.g. `op.above_for`, `op.within_pct`)
///   - `cooldown_bars` (u32)
///   - `max_wakeups_per_day` (Option<u32>; null when None)
///
/// Path scheme (symmetric with `set_filter_value`):
///   `conditions.<i>.lhs.numeric` — lhs Numeric at condition index i
///   `conditions.<i>.rhs.numeric` — rhs Numeric at condition index i
///   `conditions.<i>.lhs.range.lo` / `.range.hi` — lhs Range operand components
///   `conditions.<i>.rhs.range.lo` / `.range.hi` — rhs Range operand components
///   `conditions.<i>.op.above_for` — AboveFor(n) parameter
///   `conditions.<i>.op.below_for` — BelowFor(n) parameter
///   `conditions.<i>.op.crossed_above` — CrossedAbove(n) parameter
///   `conditions.<i>.op.crossed_below` — CrossedBelow(n) parameter
///   `conditions.<i>.op.slope_gt` — SlopeGt(n) parameter
///   `conditions.<i>.op.slope_lt` — SlopeLt(n) parameter
///   `conditions.<i>.op.zscore_gt` — ZscoreGt(n) parameter
///   `conditions.<i>.op.zscore_lt` — ZscoreLt(n) parameter
///   `conditions.<i>.op.within_pct` — WithinPct(pct) parameter
///   `cooldown_bars` — Filter::cooldown_bars
///   `max_wakeups_per_day` — Filter::max_wakeups_per_day (null when None)
pub fn filter_tunable_paths(filter: &Filter) -> Vec<(String, serde_json::Value)> {
    let mut paths: Vec<(String, serde_json::Value)> = Vec::new();

    for (i, item) in filter.conditions.items().iter().enumerate() {
        let cond = match item {
            xvision_filters::ConditionItem::Leaf(c) => c,
            xvision_filters::ConditionItem::Group(_) => continue, // groups are not individually tunable
        };
        let prefix = format!("conditions.{i}");
        // LHS
        match &cond.lhs {
            Operand::Numeric(v) => {
                paths.push((format!("{prefix}.lhs.numeric"), serde_json::json!(v)));
            }
            Operand::Range(lo, hi) => {
                paths.push((format!("{prefix}.lhs.range.lo"), serde_json::json!(lo)));
                paths.push((format!("{prefix}.lhs.range.hi"), serde_json::json!(hi)));
            }
            Operand::Indicator(_) => {}
        }
        // RHS
        match &cond.rhs {
            Operand::Numeric(v) => {
                paths.push((format!("{prefix}.rhs.numeric"), serde_json::json!(v)));
            }
            Operand::Range(lo, hi) => {
                paths.push((format!("{prefix}.rhs.range.lo"), serde_json::json!(lo)));
                paths.push((format!("{prefix}.rhs.range.hi"), serde_json::json!(hi)));
            }
            Operand::Indicator(_) => {}
        }
        // Parameterized operators
        match cond.op {
            Operator::AboveFor(n) => {
                paths.push((format!("{prefix}.op.above_for"), serde_json::json!(n)));
            }
            Operator::BelowFor(n) => {
                paths.push((format!("{prefix}.op.below_for"), serde_json::json!(n)));
            }
            Operator::CrossedAbove(n) => {
                paths.push((format!("{prefix}.op.crossed_above"), serde_json::json!(n)));
            }
            Operator::CrossedBelow(n) => {
                paths.push((format!("{prefix}.op.crossed_below"), serde_json::json!(n)));
            }
            Operator::SlopeGt(n) => {
                paths.push((format!("{prefix}.op.slope_gt"), serde_json::json!(n)));
            }
            Operator::SlopeLt(n) => {
                paths.push((format!("{prefix}.op.slope_lt"), serde_json::json!(n)));
            }
            Operator::ZscoreGt(n) => {
                paths.push((format!("{prefix}.op.zscore_gt"), serde_json::json!(n)));
            }
            Operator::ZscoreLt(n) => {
                paths.push((format!("{prefix}.op.zscore_lt"), serde_json::json!(n)));
            }
            Operator::WithinPct(pct) => {
                paths.push((format!("{prefix}.op.within_pct"), serde_json::json!(pct)));
            }
            // Non-parameterized operators — no tunable value
            Operator::Gt
            | Operator::Lt
            | Operator::Gte
            | Operator::Lte
            | Operator::Eq
            | Operator::CrossesAbove
            | Operator::CrossesBelow
            | Operator::Between => {}
        }
    }

    // Scalar filter-level fields
    paths.push((
        "cooldown_bars".to_string(),
        serde_json::json!(filter.cooldown_bars),
    ));
    paths.push((
        "max_wakeups_per_day".to_string(),
        match filter.max_wakeups_per_day {
            Some(n) => serde_json::json!(n),
            None => serde_json::Value::Null,
        },
    ));

    paths
}

/// Coerce a JSON number to u32, accepting both integer and float representations
/// (e.g. `4` or `4.0`). Returns None for non-numeric or negative/overflow values.
fn value_as_u32(value: &serde_json::Value) -> Option<u32> {
    if let Some(n) = value.as_u64() {
        return u32::try_from(n).ok();
    }
    if let Some(f) = value.as_f64() {
        if f >= 0.0 && f.fract() == 0.0 && f <= u32::MAX as f64 {
            return Some(f as u32);
        }
    }
    None
}

/// Set the value at `path` (a dotted path produced by `filter_tunable_paths`)
/// in `filter`. Returns `true` if the path resolved and the value was written,
/// `false` if the path is unknown (no panic, no partial write).
///
/// The path scheme mirrors `filter_tunable_paths` exactly — every path that
/// function emits must resolve here, and nothing else must.
pub fn set_filter_value(filter: &mut Filter, path: &str, value: &serde_json::Value) -> bool {
    if path == "cooldown_bars" {
        // Accept both integer and float JSON numbers (LLM may emit 4.0).
        if let Some(n) = value_as_u32(value) {
            filter.cooldown_bars = n;
            return true;
        }
        return false;
    }
    if path == "max_wakeups_per_day" {
        if value.is_null() {
            filter.max_wakeups_per_day = None;
            return true;
        }
        if let Some(n) = value_as_u32(value) {
            filter.max_wakeups_per_day = Some(n);
            return true;
        }
        return false;
    }

    // Parse condition paths: "conditions.<i>.<rest>"
    let parts: Vec<&str> = path.splitn(3, '.').collect();
    if parts.len() == 3 && parts[0] == "conditions" {
        let Ok(idx) = parts[1].parse::<usize>() else {
            return false;
        };
        let rest = parts[2];
        let Some(item) = filter.conditions.items_mut().get_mut(idx) else {
            return false;
        };
        let cond = match item {
            xvision_filters::ConditionItem::Leaf(c) => c,
            xvision_filters::ConditionItem::Group(_) => return false,
        };

        match rest {
            "lhs.numeric" => {
                if let Some(v) = value.as_f64() {
                    cond.lhs = Operand::Numeric(v);
                    return true;
                }
                return false;
            }
            "rhs.numeric" => {
                if let Some(v) = value.as_f64() {
                    cond.rhs = Operand::Numeric(v);
                    return true;
                }
                return false;
            }
            "lhs.range.lo" => {
                if let Some(new_lo) = value.as_f64() {
                    if let Operand::Range(lo, _) = &mut cond.lhs {
                        *lo = new_lo;
                        return true;
                    }
                }
                return false;
            }
            "lhs.range.hi" => {
                if let Some(new_hi) = value.as_f64() {
                    if let Operand::Range(_, hi) = &mut cond.lhs {
                        *hi = new_hi;
                        return true;
                    }
                }
                return false;
            }
            "rhs.range.lo" => {
                if let Some(new_lo) = value.as_f64() {
                    if let Operand::Range(lo, _) = &mut cond.rhs {
                        *lo = new_lo;
                        return true;
                    }
                }
                return false;
            }
            "rhs.range.hi" => {
                if let Some(new_hi) = value.as_f64() {
                    if let Operand::Range(_, hi) = &mut cond.rhs {
                        *hi = new_hi;
                        return true;
                    }
                }
                return false;
            }
            "op.above_for" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::AboveFor(n);
                    return true;
                }
                return false;
            }
            "op.below_for" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::BelowFor(n);
                    return true;
                }
                return false;
            }
            "op.crossed_above" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::CrossedAbove(n);
                    return true;
                }
                return false;
            }
            "op.crossed_below" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::CrossedBelow(n);
                    return true;
                }
                return false;
            }
            "op.slope_gt" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::SlopeGt(n);
                    return true;
                }
                return false;
            }
            "op.slope_lt" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::SlopeLt(n);
                    return true;
                }
                return false;
            }
            "op.zscore_gt" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::ZscoreGt(n);
                    return true;
                }
                return false;
            }
            "op.zscore_lt" => {
                if let Some(n) = value_as_u32(value) {
                    cond.op = Operator::ZscoreLt(n);
                    return true;
                }
                return false;
            }
            "op.within_pct" => {
                if let Some(pct) = value.as_f64() {
                    cond.op = Operator::WithinPct(pct);
                    return true;
                }
                return false;
            }
            _ => return false,
        }
    }

    false
}

// ── WU3a: mechanistic.* tunable paths ─────────────────────────────────────────

/// Structured error returned by `set_mechanistic_value` when a mutation is
/// rejected. The caller (apply path and tests) must inspect this rather than
/// silently ignoring the failure.
///
/// Variants:
/// - `UnknownPath` — the dotted path does not match any tunable mechanistic node.
/// - `VariantMismatch` — the leaf token (`.pct`/`.bars`/`.usd`) does not match
///   the `ClosePolicy` variant at that index (e.g. setting `.pct` on a
///   `TimeExit` entry).
/// - `InvalidValue` — the JSON value cannot be coerced to the expected Rust
///   type (e.g. a non-numeric JSON value where f64/u32 is required).
#[derive(Debug, Clone, PartialEq)]
pub enum MutatePathError {
    UnknownPath(String),
    VariantMismatch {
        path: String,
        expected_leaf: &'static str,
    },
    InvalidValue(String),
}

impl std::fmt::Display for MutatePathError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutatePathError::UnknownPath(p) => write!(f, "unknown mechanistic path: {p}"),
            MutatePathError::VariantMismatch { path, expected_leaf } => {
                write!(
                    f,
                    "variant mismatch at {path}: variant only supports .{expected_leaf}"
                )
            }
            MutatePathError::InvalidValue(msg) => write!(f, "invalid value: {msg}"),
        }
    }
}

/// Walk `MechanisticConfig.close_policies` and return a stable dotted path +
/// current JSON value for every tunable numeric scalar:
///   - `StopLoss{pct}`     → `mechanistic.close_policies.<i>.pct`
///   - `TakeProfit{pct}`   → `mechanistic.close_policies.<i>.pct`
///   - `TrailingStop{pct}` → `mechanistic.close_policies.<i>.pct`
///   - `TimeExit{bars}`    → `mechanistic.close_policies.<i>.bars`
///   - `TargetPnl{usd}`    → `mechanistic.close_policies.<i>.usd`
///
/// Path scheme is symmetric with `set_mechanistic_value`: every path emitted
/// here must resolve in the setter, and the leaf token encodes the variant so
/// cross-variant writes are caught at the setter level.
pub fn mechanistic_tunable_paths(
    cfg: &crate::strategies::MechanisticConfig,
) -> Vec<(String, serde_json::Value)> {
    use crate::strategies::ClosePolicy;
    let mut paths = Vec::new();
    for (i, policy) in cfg.close_policies.iter().enumerate() {
        match policy {
            ClosePolicy::StopLoss { pct }
            | ClosePolicy::TakeProfit { pct }
            | ClosePolicy::TrailingStop { pct } => {
                paths.push((
                    format!("mechanistic.close_policies.{i}.pct"),
                    serde_json::json!(pct),
                ));
            }
            ClosePolicy::TimeExit { bars } => {
                paths.push((
                    format!("mechanistic.close_policies.{i}.bars"),
                    serde_json::json!(bars),
                ));
            }
            ClosePolicy::TargetPnl { usd } => {
                paths.push((
                    format!("mechanistic.close_policies.{i}.usd"),
                    serde_json::json!(usd),
                ));
            }
        }
    }
    paths
}

/// Set the value at `path` (a dotted path produced by `mechanistic_tunable_paths`)
/// in `cfg`. Returns `Ok(())` on success. Returns a [`MutatePathError`] when:
///   - the path is unknown / out of bounds → `UnknownPath`
///   - the leaf token does not match the `ClosePolicy` variant at that index →
///     `VariantMismatch` (config is NOT mutated)
///   - the JSON value cannot be coerced to the required type → `InvalidValue`
///
/// Path scheme: `mechanistic.close_policies.<i>.<leaf>` where `<leaf>` is one of
/// `pct`, `bars`, or `usd`. The variant at index `<i>` must support that leaf.
pub fn set_mechanistic_value(
    cfg: &mut crate::strategies::MechanisticConfig,
    path: &str,
    value: &serde_json::Value,
) -> Result<(), MutatePathError> {
    use crate::strategies::ClosePolicy;

    // Parse: "mechanistic.close_policies.<i>.<leaf>"
    let parts: Vec<&str> = path.splitn(5, '.').collect();
    // Expected: ["mechanistic", "close_policies", "<i>", "<leaf>"]
    if parts.len() != 4 || parts[0] != "mechanistic" || parts[1] != "close_policies" {
        return Err(MutatePathError::UnknownPath(path.to_string()));
    }
    let idx: usize = parts[2]
        .parse()
        .map_err(|_| MutatePathError::UnknownPath(path.to_string()))?;
    let leaf = parts[3];

    let policy = cfg
        .close_policies
        .get_mut(idx)
        .ok_or_else(|| MutatePathError::UnknownPath(path.to_string()))?;

    // Variant-aware: check that the leaf matches the variant BEFORE mutating.
    let variant_leaf: &'static str = match policy {
        ClosePolicy::StopLoss { .. } | ClosePolicy::TakeProfit { .. } | ClosePolicy::TrailingStop { .. } => {
            "pct"
        }
        ClosePolicy::TimeExit { .. } => "bars",
        ClosePolicy::TargetPnl { .. } => "usd",
    };

    if leaf != variant_leaf {
        return Err(MutatePathError::VariantMismatch {
            path: path.to_string(),
            expected_leaf: variant_leaf,
        });
    }

    // Now apply the value — variant and leaf agree.
    match policy {
        ClosePolicy::StopLoss { pct }
        | ClosePolicy::TakeProfit { pct }
        | ClosePolicy::TrailingStop { pct } => {
            let v = value.as_f64().ok_or_else(|| {
                MutatePathError::InvalidValue(format!("expected f64 for .pct, got {value}"))
            })?;
            *pct = v;
        }
        ClosePolicy::TimeExit { bars } => {
            let v = value_as_u32(value).ok_or_else(|| {
                MutatePathError::InvalidValue(format!("expected u32 for .bars, got {value}"))
            })?;
            *bars = v;
        }
        ClosePolicy::TargetPnl { usd } => {
            let v = value.as_f64().ok_or_else(|| {
                MutatePathError::InvalidValue(format!("expected f64 for .usd, got {value}"))
            })?;
            *usd = v;
        }
    }

    Ok(())
}

/// The mutation kinds that are *structurally applicable* to `base`, intersected
/// with the operator-allowed kinds (F21). `param` is applicable whenever the
/// strategy exposes a tunable key (always, since every strategy has a `risk`
/// config). `tool` stays as allowed. `prose` is applicable when the strategy
/// has at least one `AgentRef` to carry a `prompt_override` (Phase 0 substrate):
/// a prose edit sets `AgentRef.prompt` and changes the strategy content
/// hash, so it is a real change — not a no-op — on any agent strategy. For
/// agentless/pre-refactor strategies there is still no home, so prose is
/// excluded there. `filter` is always applicable: with an existing filter the
/// AST-walk tunable-path enumeration drives TUNING; with none, the writer may
/// CREATE one (xvision-vxn) via `MutationDiff::create_filter`.
pub fn applicable_mutation_kinds(base: &Strategy, allowed: &[String]) -> Vec<String> {
    let has_params = !tunable_param_keys(base).is_empty();
    // Prose is applicable iff the strategy has at least one agent to carry a
    // `prompt_override` (Phase 0). For agentless/pre-refactor strategies there
    // is still no home, so prose stays excluded there.
    let has_prompt_home = !base.agents.is_empty();
    allowed
        .iter()
        .filter(|k| match k.as_str() {
            "param" => has_params,
            "tool" => true,
            "prose" => has_prompt_home,
            // Filter is always an applicable lever: TUNE the typed Filter AST
            // when one exists, or CREATE one (xvision-vxn) when `base.filter` is
            // None so a filterless strategy can still be gated/throttled.
            "filter" => true,
            _ => false,
        })
        .cloned()
        .collect()
}

impl MutationDiff {
    /// Return `self.kind.as_str()` for convenience.
    pub fn kind_label(&self) -> &'static str {
        self.kind.as_str()
    }

    pub fn is_empty(&self) -> bool {
        self.prose.is_empty()
            && self.params.is_empty()
            && self.tools.added.is_empty()
            && self.tools.removed.is_empty()
            && self.filter.is_empty()
            // xvision-vxn: a create-only diff (authored filter, no other edits)
            // is a real structural change, not an empty mutation.
            && self.create_filter.is_none()
    }

    /// Apply this diff to `base`, returning the candidate strategy.
    ///
    /// This is the **canonical** apply used by the cycle orchestrator, the
    /// inversion-pair check, the `mutate-once` CLI verb, and the mutator's own
    /// identity check, so all of them agree on what a diff actually changes. It
    /// applies:
    ///   - `params` targeting `risk.<field>` (or a bare risk-field name): routed
    ///     into the typed `risk` config via a JSON round-trip (F14/F20 — this is
    ///     the primary tunable surface real strategies have).
    ///   - `params` targeting `mechanistic.*`: routed into the typed
    ///     `mechanistic_config` via the variant-aware setter.
    ///   - `params` otherwise: a no-op (the validator rejects unknown keys
    ///     upstream, so apply stays total).
    ///   - `tools`: add/remove against `manifest.required_tools`.
    ///   - `prose`: each `ProseEdit` sets the matching `AgentRef.prompt`
    ///     on the strategy (matched by `canonical_role`). This lands in the
    ///     `Strategy` content hash — so the override is part of proper lineage —
    ///     without touching the shared `Agent` library record. An edit naming a
    ///     role no agent plays is a no-op (the validator rejects those upstream;
    ///     apply stays total).
    pub fn apply_to(&self, base: &Strategy) -> Strategy {
        let mut s = base.clone();
        // Route risk-targeted params through a single JSON round-trip of the
        // typed risk config so an invalid value can't half-apply.
        let mut risk_json = serde_json::to_value(&s.risk).unwrap_or(serde_json::Value::Null);
        let mut risk_touched = false;
        for change in &self.params {
            if change.key.starts_with("mechanistic.") {
                // WU3a: route mechanistic.* keys through the variant-aware setter.
                // A mismatch or invalid value is a silent no-op here (the validator
                // rejects those upstream; apply stays total).
                // WU-B: clamp to TunableBound before writing, if a bound exists.
                if let Some(ref mut mc) = s.mechanistic_config {
                    let value_to_write = if let Some(bound) = find_bound(&base.tunable_bounds, &change.key) {
                        clamp_to_bound(&change.after, bound)
                    } else {
                        change.after.clone()
                    };
                    let _ = set_mechanistic_value(mc, &change.key, &value_to_write);
                }
            } else if let Some(field) = risk_field_for_key(base, &change.key) {
                // WU-B: risk params — clamp to TunableBound if present.
                let value_to_write = if let Some(bound) = find_bound(&base.tunable_bounds, &change.key) {
                    clamp_to_bound(&change.after, bound)
                } else {
                    change.after.clone()
                };
                if let Some(obj) = risk_json.as_object_mut() {
                    obj.insert(field, value_to_write);
                    risk_touched = true;
                }
            }
            // A key that is neither `mechanistic.*` nor a known risk field is a
            // silent no-op — the validator rejects such keys upstream (they are
            // not in `tunable_param_keys`), so apply stays total.
        }
        if risk_touched {
            if let Ok(new_risk) = serde_json::from_value(risk_json) {
                s.risk = new_risk;
            }
        }
        for added in &self.tools.added {
            if !s.manifest.required_tools.contains(added) {
                s.manifest.required_tools.push(added.clone());
            }
        }
        for removed in &self.tools.removed {
            s.manifest.required_tools.retain(|t| t != removed);
        }
        // Prose edits land on the trader AgentRef's prompt_override (Phase 0
        // substrate). Matching by canonical role keeps lineage stable: the
        // override changes the Strategy content hash WITHOUT touching the shared
        // Agent library record. An edit naming a role no agent plays is a no-op
        // (the validator rejects those upstream; apply stays total).
        for edit in &self.prose {
            let target = crate::strategies::agent_ref::canonical_role(&edit.agent_role);
            if let Some(a) = s.agents.iter_mut().find(|a| a.canonical_role() == target) {
                a.prompt = edit.after.clone();
            }
        }
        // xvision-vxn: structural filter CREATION. When the parent has no filter
        // and the writer authored one, install it through the same authoring
        // parse/validate path operators use (stamps id + strategy_id, runs
        // `xvision_filters::validate`) and gate activation. An existing filter is
        // never clobbered — that case is a TUNE via filter edits below. An invalid
        // payload is a silent no-op here (the validator rejects it upstream; apply
        // stays total).
        if s.filter.is_none() {
            if let Some(create) = &self.create_filter {
                if let Ok(filter) = crate::authoring::parse_filter_value(create.clone(), &s.manifest.id) {
                    s.filter = Some(filter);
                    s.activation_mode = xvision_filters::ActivationMode::FilterGated;
                }
            }
        }
        // Filter edits resolve path → AST node and write `after`. An unresolved
        // path or a wrong-type value is a silent no-op (validator rejects those
        // upstream; apply stays total). The filter field is cloned before mutation
        // so a partial-failure edit doesn't leave the filter half-changed.
        // WU-B: clamp each filter edit's value to its TunableBound before writing.
        if let Some(ref mut f) = s.filter {
            for edit in &self.filter {
                // Ignore the return value; validator already ensured the path
                // resolves and the value has the right type.
                let value_to_write = if let Some(bound) = find_bound(&base.tunable_bounds, &edit.path) {
                    clamp_to_bound(&edit.after, bound)
                } else {
                    edit.after.clone()
                };
                set_filter_value(f, &edit.path, &value_to_write);
            }
        }
        s
    }
}

/// Clamp `value` to the `[min, max]` range declared by a `TunableBound`, then
/// apply step alignment for `Int` kind.
///
/// Behaviour per kind:
/// - **Int**: clamp to `[min, max]` (if present), then round to nearest
///   integer (so step=1 is honoured; finer steps are rounded to integer
///   because Pine `input.int` is always integer-valued).
/// - **Float**: clamp to `[min, max]` (if present). Non-numeric values are
///   returned unchanged (the write path below will handle or ignore them).
/// - **Bool**: coerce the value to a JSON bool.  Any truthy non-zero number
///   → `true`; zero / JSON `false` / `null` → `false`.  Non-numeric,
///   non-bool values are returned unchanged.
///
/// Paths with no matching `TunableBound` call this function's caller with
/// the original value — this function is never called for unbound paths.
pub fn clamp_to_bound(value: &serde_json::Value, b: &crate::strategies::TunableBound) -> serde_json::Value {
    use crate::strategies::pine_import::inputs::InputKind;

    match b.kind {
        InputKind::Bool => {
            // Coerce to bool: numeric 0 / JSON false / null → false; anything
            // else truthy → true.
            let result = match value {
                serde_json::Value::Bool(v) => *v,
                serde_json::Value::Number(n) => n.as_f64().map(|f| f != 0.0).unwrap_or(false),
                serde_json::Value::Null => false,
                _ => return value.clone(), // non-coercible; caller handles
            };
            serde_json::Value::Bool(result)
        }
        InputKind::Float => {
            let Some(mut v) = value.as_f64() else {
                return value.clone();
            };
            if let Some(min) = b.min {
                if v < min {
                    v = min;
                }
            }
            if let Some(max) = b.max {
                if v > max {
                    v = max;
                }
            }
            serde_json::json!(v)
        }
        InputKind::Int => {
            let Some(mut v) = value.as_f64() else {
                return value.clone();
            };
            if let Some(min) = b.min {
                if v < min {
                    v = min;
                }
            }
            if let Some(max) = b.max {
                if v > max {
                    v = max;
                }
            }
            // Round to nearest integer (Pine input.int is always integer-valued).
            v = v.round();
            serde_json::json!(v)
        }
    }
}

/// Look up the `TunableBound` for `path` in `bounds`, if any.
fn find_bound<'a>(
    bounds: &'a [crate::strategies::TunableBound],
    path: &str,
) -> Option<&'a crate::strategies::TunableBound> {
    bounds.iter().find(|b| b.path == path)
}

pub struct Mutator {
    pub provider: String,
    pub model: String,
    pub dispatch: Arc<dyn LlmDispatch + Send + Sync>,
    pub max_retries: u32,
}

impl Mutator {
    pub async fn propose(
        &self,
        base: &Strategy,
        config: &AutoOptimizerConfig,
        dsr_prefix: Option<&str>,
        exploration_seed: u64,
        // Position of this writer within the cycle's mutations_per_parent loop
        // (0-based). Used to rotate the focus KIND across writers so every kind
        // is explored within a single cycle rather than leaving kind selection to
        // hash chance. The exploration_seed still varies the specific target within
        // the selected kind.
        mutation_idx: usize,
        memory_context: Option<&str>,
        avoid: &std::collections::HashSet<ContentHash>,
        resolved_agent_prompts: Option<&std::collections::HashMap<String, String>>,
    ) -> anyhow::Result<MutationDiff> {
        let empty_map = std::collections::HashMap::new();
        let resolved = resolved_agent_prompts.unwrap_or(&empty_map);
        let program_md = program_view::to_markdown_with_resolved_prompts(base, resolved);
        let mut last_errors: Option<Vec<ValidationError>> = None;
        // R4: retain the most recent raw model output so a no_candidate failure
        // is debuggable (it was previously discarded after parsing).
        let mut last_raw: Option<String> = None;
        let max_attempts = self.max_retries.saturating_add(1);

        assert!(max_attempts >= 1, "max_attempts must be at least 1");

        // F21: only offer the experiment writer the kinds that can actually
        // change this strategy, and tell it exactly which param keys exist
        // (mechanical + risk.*), so it stops proposing non-existent params or a
        // prose edit that can't be applied.
        let kinds = applicable_mutation_kinds(base, &config.allowed_mutation_kinds);
        let kinds = if kinds.is_empty() {
            // Defensive: never send an empty kind list. `param` is universally
            // applicable because every strategy carries a risk config.
            vec!["param".to_string()]
        } else {
            kinds
        };
        let param_keys = tunable_param_keys(base);
        // Filter paths: enumerate once per propose call (same filter for all attempts).
        // Only computed when "filter" is in the applicable kinds to avoid walking
        // a filter that won't be offered.
        let filter_paths: Vec<(String, serde_json::Value)> = if kinds.iter().any(|k| k == "filter") {
            base.filter.as_ref().map(filter_tunable_paths).unwrap_or_default()
        } else {
            Vec::new()
        };
        // Issues 1/2 (QA 2026-06-08): the prose-capable agent roles, so the
        // exploration focus can rotate onto the PROSE lever. Only enumerate when
        // "prose" is actually applicable (the strategy has agents AND prose is in
        // the allowed kinds), matching the validator's known-role set.
        let prose_roles: Vec<String> = if kinds.iter().any(|k| k == "prose") {
            base.agents.iter().map(|a| a.role.clone()).collect()
        } else {
            Vec::new()
        };

        for attempt in 0..max_attempts {
            // F32 (run-7): rotate the exploration seed per attempt so the focus
            // parameter — `param_keys[seed % len]` — actually changes across
            // retries. Without this, every retry re-derives the SAME focus and
            // `already_tried` fires until the budget is exhausted with no escape.
            let attempt_seed = exploration_seed.wrapping_add(attempt as u64);
            let user_text = build_user_payload(
                &program_md,
                &kinds,
                &param_keys,
                &filter_paths,
                last_errors.as_deref(),
                attempt_seed,
                mutation_idx,
                memory_context,
                avoid.len(),
                &prose_roles,
                attempt as usize,
                base.filter.is_some(),
            );
            let req = LlmRequest {
                model: self.model.clone(),
                system_prompt: build_system_prompt(dsr_prefix),
                messages: vec![Message::user_text(user_text)],
                max_tokens: None,
                tools: vec![],
                // F32: the experiment writer was deterministic (temperature None +
                // a fixed prompt), so the same parent produced the IDENTICAL
                // candidate every cycle — the optimizer could never explore or
                // converge. Sample with a non-zero, per-cycle-jittered temperature
                // and a per-cycle exploration nonce in the prompt so successive
                // cycles propose diverse candidates. Also jitter per attempt.
                temperature: Some(exploration_temperature(attempt_seed)),
                // B3: supply a constrained `mutation_diff` schema so OpenAI-compat
                // dispatchers (Ollama) grammar-constrain the JSON. Without it the
                // unconstrained json_object mode parse-fails ~40% on Ollama.
                response_schema: Some(crate::agent::llm::ResponseSchema::mutation_diff()),
                cache_control: None,
                force_json: true,
            };

            // R3: a dispatch error (transient reset / 5xx / timeout) is treated
            // like a parse/validation error — counted against the attempt budget
            // and retried — instead of propagating immediately and aborting the
            // experiment. On exhaustion the loop bails and the cycle absorbs it
            // as `no_candidate`; cycle-level isolation (R3) then seals the cycle.
            // F35 (two-tier dispatch): try structured output first, then fall
            // back to plain JSON if the provider doesn't support response_format.
            let mut resp = None;
            let mut fallback_err: Option<anyhow::Error> = None;
            match self.dispatch.complete(req.clone()).await {
                Ok(r) => resp = Some(r),
                Err(e) => {
                    // Check if this is a response_format unsupported error so we
                    // can retry without schema before consuming an attempt.
                    let is_response_format_unsupported = e
                        .downcast_ref::<crate::agent::llm::OpenAiCompatError>()
                        .map_or(false, |rfe| {
                            matches!(
                                rfe,
                                crate::agent::llm::OpenAiCompatError::ResponseFormatUnsupported { .. }
                            )
                        });
                    if is_response_format_unsupported {
                        tracing::info!(
                            target: "xvision::autooptimizer",
                            attempt,
                            "response_format unsupported by provider; retrying as plain JSON (F35)"
                        );
                        let fallback_req = LlmRequest {
                            response_schema: None,
                            system_prompt: build_system_prompt(dsr_prefix)
                                + "\n\nIMPORTANT: Respond with valid JSON only. No markdown fences, no commentary. Your entire response must be parseable by JSON.parse().",
                            ..req
                        };
                        match self.dispatch.complete(fallback_req).await {
                            Ok(r) => resp = Some(r),
                            Err(fb_err) => {
                                tracing::warn!(
                                    target: "xvision::autooptimizer",
                                    attempt,
                                    error = %fb_err,
                                    "mutator plain-JSON fallback also failed (F35)"
                                );
                                fallback_err = Some(fb_err);
                            }
                        }
                    } else {
                        // Not a response_format issue — treat as transient error (R3).
                        tracing::warn!(
                            target: "xvision::autooptimizer",
                            attempt,
                            error = %e,
                            "mutator dispatch errored; retrying within the attempt budget (R3)"
                        );
                        fallback_err = Some(e);
                    }
                }
            };
            let resp = match (resp, fallback_err) {
                (Some(r), _) => r,
                (None, Some(err)) => {
                    last_errors = Some(vec![ValidationError {
                        code: "dispatch_error".into(),
                        message: format!("experiment-writer dispatch failed: {err:#}"),
                        path: None,
                    }]);
                    continue;
                }
                (None, None) => unreachable!("dispatch must return success or error"),
            };
            let raw_text = resp.text();
            last_raw = Some(raw_text.clone());

            let parse_result = extract_and_parse(&raw_text);
            match parse_result {
                Err(parse_err) => {
                    let synthetic = vec![ValidationError {
                        code: "parse_error".into(),
                        message: parse_err.to_string(),
                        path: None,
                    }];
                    last_errors = Some(synthetic);
                }
                Ok(diff) => match validate_mutation_diff(&diff, base) {
                    Ok(()) if is_identity_diff(&diff, base) => {
                        // F14: a diff that leaves the strategy byte-identical is
                        // a guaranteed 0.0-delta no-op and hashes to the parent
                        // (corrupting lineage — F12). Feed it back as an error so
                        // the next attempt proposes a real change rather than the
                        // mutator "succeeding" with nothing to gate.
                        last_errors = Some(vec![ValidationError {
                            code: "identity_diff".into(),
                            message: "the proposed change does not alter the strategy (no-op); \
                                      propose a concrete parameter or tool change"
                                .into(),
                            path: None,
                        }]);
                    }
                    Ok(()) if candidate_already_tried(&diff, base, avoid) => {
                        // F32: the proposed candidate is byte-identical to one this
                        // parent ALREADY produced in an earlier experiment/cycle.
                        // Re-emitting it re-spends a backtest on a known result and
                        // is exactly the fixed point that made repeat cycles never
                        // explore (the real model collapses to the single "most
                        // obvious" tweak regardless of temperature). Reject and
                        // retry, steering the writer to genuinely new territory —
                        // a hard, model-independent guarantee that the optimizer
                        // never re-evaluates a candidate it has already seen.
                        last_errors = Some(vec![ValidationError {
                            code: "already_tried".into(),
                            message: "this exact candidate was already evaluated on this parent in a \
                                      prior experiment; propose a DIFFERENT change — a different \
                                      parameter key, or a clearly different direction/magnitude for \
                                      the same key"
                                .into(),
                            path: None,
                        }]);
                    }
                    Ok(()) => return Ok(diff),
                    Err(errors) => {
                        last_errors = Some(errors);
                    }
                },
            }
        }

        let error_text = last_errors
            .as_deref()
            .map(format_validation_errors)
            .unwrap_or_else(|| "unknown error".into());

        // R4: log the full-ish raw output (the model's actual response) so an
        // operator can diagnose WHY every attempt failed, and append a truncated
        // snippet to the error so it flows into the cycle's `no_candidate`
        // reason. Previously the raw text was discarded entirely.
        if let Some(raw) = last_raw.as_deref() {
            tracing::warn!(
                target: "xvision::autooptimizer",
                mutation_idx,
                attempts = max_attempts,
                raw = %raw.chars().take(2000).collect::<String>(),
                "experiment writer produced no usable candidate; last raw output captured"
            );
        }
        let raw_tail = last_raw
            .as_deref()
            .map(|r| format!("; last raw output: {}", r.chars().take(800).collect::<String>()))
            .unwrap_or_default();

        anyhow::bail!(
            "mutator failed after {} attempt(s): {}{}",
            max_attempts,
            error_text,
            raw_tail
        )
    }
}

/// True when applying `diff` to `base` yields a candidate whose content hash is
/// already in `avoid` — i.e. this parent already produced this exact candidate in
/// an earlier experiment/cycle. F32: re-emitting it would re-spend a backtest on a
/// known result; rejecting it is the hard, model-independent guarantee that
/// successive cycles can't re-derive the same losing candidate forever.
fn candidate_already_tried(
    diff: &MutationDiff,
    base: &Strategy,
    avoid: &std::collections::HashSet<ContentHash>,
) -> bool {
    if avoid.is_empty() {
        return false;
    }
    match serde_json::to_value(diff.apply_to(base)) {
        Ok(c) => avoid.contains(&ContentHash::of_json(&c)),
        Err(_) => false,
    }
}

/// True when applying `diff` to `base` produces a strategy with the same
/// content hash — i.e. the diff is a no-op at the strategy-artifact level.
fn is_identity_diff(diff: &MutationDiff, base: &Strategy) -> bool {
    let candidate = diff.apply_to(base);
    match (serde_json::to_value(base), serde_json::to_value(&candidate)) {
        (Ok(b), Ok(c)) => ContentHash::of_json(&b) == ContentHash::of_json(&c),
        // If either fails to serialize we can't prove it's identity; let the
        // downstream gate handle it rather than spuriously rejecting.
        _ => false,
    }
}

fn system_prompt_text() -> String {
    let marker = "# USER";
    if let Some(idx) = PROMPT_TEMPLATE.find(marker) {
        PROMPT_TEMPLATE[..idx].trim().to_string()
    } else {
        PROMPT_TEMPLATE.to_string()
    }
}

fn build_system_prompt(dsr_prefix: Option<&str>) -> String {
    let base = system_prompt_text();
    match dsr_prefix {
        None | Some("") => base,
        Some(prefix) => format!("{prefix}\n\n---\n\n{base}"),
    }
}

/// F32: per-cycle sampling temperature for the experiment writer. Jittered by the
/// exploration seed within an exploratory band (0.7–1.1) so different cycles
/// sample differently — the deterministic `temperature: None` produced the same
/// candidate every cycle. The band stays below fully-random so proposals remain
/// coherent JSON edits.
fn exploration_temperature(exploration_seed: u64) -> f64 {
    0.7 + (exploration_seed % 5) as f64 * 0.1
}

/// B17: per-path domain-constraint hint for the enumerated filter list, so the
/// experiment writer learns the operator's value domain up front instead of
/// repeatedly proposing out-of-domain values (e.g. `zscore_lt=0`) that the
/// validator then rejects, wasting an attempt.
///
/// The validator (`validate_filter_edits`) stays the authoritative safety net;
/// this only mirrors its constraints into the prompt. To avoid drift, the
/// window-op set is the shared `FILTER_U32_WINDOW_OPS` from the validator.
/// Returns `None` for paths with no special integer/positivity constraint (e.g.
/// `conditions.<i>.rhs.numeric`), which must not get the integer marker.
pub(crate) fn filter_path_constraint_hint(path: &str) -> Option<&'static str> {
    // Parameterized operator paths look like `conditions.<i>.op.<suffix>`.
    let suffix = path
        .strip_prefix("conditions.")
        .and_then(|rest| rest.split_once('.').map(|(_, t)| t))
        .and_then(|tail| tail.strip_prefix("op."));
    match suffix {
        Some("within_pct") => Some("(positive number > 0)"),
        Some(s) if crate::autooptimizer::validator::FILTER_U32_WINDOW_OPS.contains(&s) => {
            Some("(positive integer >= 1)")
        }
        _ => None,
    }
}

/// B17 (xvision-ds0): the "Tunable filter paths" prompt section, each path
/// annotated with its domain-constraint hint via [`filter_path_constraint_hint`].
/// Shared by both proposal paths — the single-shot `build_user_payload` and the
/// tournament `build_proposal_user` — so the filter-domain guidance can never
/// drift between them. Empty unless `filter` is an allowed kind and the strategy
/// actually has tunable filter paths.
pub(crate) fn annotated_filter_paths_section(
    allowed_kinds: &[String],
    filter_paths: &[(String, serde_json::Value)],
) -> String {
    if allowed_kinds.iter().any(|k| k == "filter") && !filter_paths.is_empty() {
        format!(
            "\n\nTunable filter paths (a `filter` experiment's `path` MUST be exactly one of these; \
             `before` must match the current value shown):\n{}",
            filter_paths
                .iter()
                .map(|(p, v)| match filter_path_constraint_hint(p) {
                    Some(hint) => format!("  - {p}: {v}  {hint}"),
                    None => format!("  - {p}: {v}"),
                })
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        String::new()
    }
}

/// B6 (xvision-ds0): the per-attempt exploration/kind-rotation directive. The
/// focused mutation KIND rotates by `(slot + attempt) % n_kinds` so successive
/// retries (and successive writer slots / tournament candidates) focus DIFFERENT
/// levers instead of re-hammering one failing kind until the attempt budget is
/// gone; the specific TARGET within the kind is chosen by `exploration_seed`.
/// Shared by both proposal paths to keep the rotation identical.
pub(crate) fn kind_focus_directive(
    allowed_kinds: &[String],
    param_keys: &[String],
    filter_paths: &[(String, serde_json::Value)],
    prose_roles: &[String],
    slot: usize,
    attempt: usize,
    exploration_seed: u64,
) -> String {
    let param_allowed = allowed_kinds.iter().any(|k| k == "param");
    let mut focus_groups: Vec<(&str, Vec<String>)> = Vec::new();
    if allowed_kinds.iter().any(|k| k == "prose") && !prose_roles.is_empty() {
        focus_groups.push(("prose", prose_roles.to_vec()));
    }
    if allowed_kinds.iter().any(|k| k == "filter") && !filter_paths.is_empty() {
        focus_groups.push(("filter", filter_paths.iter().map(|(p, _)| p.clone()).collect()));
    }
    if param_allowed && !param_keys.is_empty() {
        focus_groups.push(("param", param_keys.to_vec()));
    }
    if focus_groups.is_empty() {
        format!(
            "\n\nExploration directive (variant {exploration_seed}): pick a different change than \
             the single most obvious one, so repeated runs explore rather than re-propose one tweak."
        )
    } else {
        let n_kinds = focus_groups.len();
        // B6: offset the kind index by the retry `attempt` so successive retries
        // focus DIFFERENT kinds. When n_kinds == 1 the offset is a no-op. The
        // target WITHIN a kind is still chosen by `exploration_seed`.
        let (kind, targets) = &focus_groups[(slot + attempt) % n_kinds];
        let target = &targets[(exploration_seed as usize) % targets.len()];
        match *kind {
            "prose" => format!(
                "\n\nExploration directive (variant {exploration_seed}): FOCUS this experiment on the \
                 `{target}` agent's system prompt — propose a `prose` experiment that rewrites its \
                 trading logic, reasoning steps, or entry/exit criteria. The `after` field MUST \
                 contain the COMPLETE replacement prompt (copy the full current prompt from the \
                 'Current system prompt:' section above and modify it). Do NOT leave `after` empty or \
                 abbreviated — the full prompt text is required. A `param` or `tool` experiment will \
                 be REJECTED — you MUST submit a `prose` experiment this round. This focus rotates \
                 the optimizer across its levers so successive runs explore the prompt, the filter, \
                 and the numeric parameters rather than re-proposing one fixed tweak."
            ),
            "filter" => format!(
                "\n\nExploration directive (variant {exploration_seed}): FOCUS this experiment on filter \
                 path `{target}` — propose a `filter` experiment changing its value with a clear \
                 direction and magnitude (e.g. loosen a threshold to admit more trades, or tighten it \
                 to be more selective). This focus rotates the optimizer across its levers so \
                 successive runs explore the prompt, the filter, and the numeric parameters rather \
                 than re-proposing one fixed tweak. If `{target}` genuinely cannot be improved, you \
                 may target another listed lever instead."
            ),
            _ => format!(
                "\n\nExploration directive (variant {exploration_seed}): FOCUS this experiment on \
                 parameter `{target}` — propose a meaningful change to its value (a clear direction \
                 and magnitude). This focus rotates the optimizer across its levers so successive \
                 runs explore the prompt, the filter, and the numeric parameters rather than \
                 re-proposing one fixed tweak. If `{target}` genuinely cannot be improved, you may \
                 target another listed lever instead."
            ),
        }
    }
}

/// xvision-vxn: the structural filter-CREATION directive. When `filter` is an
/// allowed kind and the strategy has NO filter (`filter_exists == false`), invite
/// the writer to AUTHOR a complete filter under the `create_filter` field (rather
/// than the path-level `filter` edits, which only TUNE an existing filter). Empty
/// when a filter already exists or `filter` is not an allowed kind. Shared by both
/// proposal paths so the guidance can never drift.
pub(crate) fn filter_create_directive(allowed_kinds: &[String], filter_exists: bool) -> String {
    if filter_exists || !allowed_kinds.iter().any(|k| k == "filter") {
        return String::new();
    }
    "\n\nThis strategy has NO filter yet. To use the filter lever you must CREATE one: set \
     `create_filter` to a COMPLETE filter object (with `display_name`, `asset_scope`, \
     `timeframe`, a non-empty `conditions` tree, and optionally `cooldown_bars` / \
     `max_wakeups_per_day` to throttle trade frequency). Do NOT use the `filter` edit list \
     for this (that only TUNES an existing filter). Leave `create_filter` null if you are \
     proposing a different kind of experiment."
        .to_string()
}

fn build_user_payload(
    program_md: &str,
    allowed_kinds: &[String],
    param_keys: &[String],
    filter_paths: &[(String, serde_json::Value)],
    previous_errors: Option<&[ValidationError]>,
    exploration_seed: u64,
    // Position of this writer in the outer mutations_per_parent loop (0-based).
    // Drives balanced kind rotation across the cycle's N concurrent writers so
    // every applicable kind gets a dedicated slot rather than competing via hash
    // modulo (which can cluster). `exploration_seed` still diversifies the
    // specific target chosen within the selected kind.
    mutation_idx: usize,
    memory_context: Option<&str>,
    avoid_count: usize,
    // Issues 1/2 (QA 2026-06-08): the agent roles that can carry a
    // `prompt_override`, so the exploration focus can rotate onto the PROSE lever
    // (rewrite an agent's prompt) and not only numeric levers. Empty when prose is
    // not an applicable kind for this strategy.
    prose_roles: &[String],
    // B6: the propose retry attempt index (0-based). `mutation_idx` is fixed for
    // the whole propose call, so on its own it re-focuses the SAME kind on every
    // retry — a failing kind then consumes the entire attempt budget. Offsetting
    // the kind index by `attempt` rotates successive retries onto DIFFERENT kinds.
    // Target rotation within a kind still comes from `exploration_seed`.
    attempt: usize,
    // xvision-vxn: whether the parent already has a filter. Drives the
    // create-vs-tune guidance (`filter_create_directive`).
    filter_exists: bool,
) -> String {
    let kinds_text = allowed_kinds.join(", ");
    let param_allowed = allowed_kinds.iter().any(|k| k == "param");
    // Codex P2: only show param key list when `param` is actually allowed. A
    // filter-only config that still shows risk.* keys steers the model to propose
    // a param mutation that the cycle loop accepts (validate_mutation_diff does not
    // enforce the allowed-kinds list). Gate the section and the F32 focus directive
    // together so the prompt never references a disallowed mutation axis.
    let keys_section = if !param_allowed {
        String::new()
    } else if param_keys.is_empty() {
        "\n\nThis strategy exposes no tunable parameter keys; do not propose a `param` experiment."
            .to_string()
    } else {
        format!(
            "\n\nTunable parameter keys (a `param` experiment's `key` MUST be exactly one of these):\n{}",
            param_keys
                .iter()
                .map(|k| format!("  - {k}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    };
    // Filter paths section (B17): shared with the tournament path so the
    // filter-domain hints never drift between proposers.
    let filter_section = annotated_filter_paths_section(allowed_kinds, filter_paths);
    // xvision-vxn: when the strategy has no filter, invite a structural create.
    let create_section = filter_create_directive(allowed_kinds, filter_exists);
    let errors_section = match previous_errors {
        None => String::new(),
        Some(errs) => {
            format!(
                "\n\nPrevious attempt errors — you MUST fix all of these:\n\n{}",
                format_validation_errors(errs)
            )
        }
    };

    // F32/B6: the per-attempt exploration + kind-rotation directive, shared
    // with the tournament path via `kind_focus_directive`. `mutation_idx` is
    // this writer's slot in the outer mutations_per_parent loop; `attempt`
    // rotates the focused kind across retries; `exploration_seed` picks the
    // target within the kind.
    let exploration_section = kind_focus_directive(
        allowed_kinds,
        param_keys,
        filter_paths,
        prose_roles,
        mutation_idx,
        attempt,
        exploration_seed,
    );
    // F32: when this parent has already produced candidates in prior experiments,
    // tell the writer so it aims for genuinely new territory (the `already_tried`
    // gate will reject any duplicate and force a retry regardless).
    let no_repeat_section = if avoid_count == 0 {
        String::new()
    } else {
        format!(
            "\n\nThis parent has already been experimented on {avoid_count} time(s); your proposal \
             MUST differ from every prior candidate. An exact repeat will be rejected."
        )
    };

    // P3 + low-trade cycle context: advisory context may include recalled prior
    // outcomes and current-window sample-size warnings. It is advisory ONLY — it
    // does not relax the F32 exploration directive above or the hard avoid-set
    // (exact-repeat dedup) the orchestrator enforces.
    let memory_section = match memory_context {
        Some(ctx) if !ctx.trim().is_empty() => format!(
            "\n\nAdvisory optimizer context (do not override allowed experiment kinds or JSON schema):\n{ctx}"
        ),
        _ => String::new(),
    };

    format!(
        "Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: {kinds_text}{keys_section}{filter_section}{create_section}{errors_section}{exploration_section}{no_repeat_section}{memory_section}\n\nPropose ONE experiment as a JSON object."
    )
}

fn extract_and_parse(text: &str) -> anyhow::Result<MutationDiff> {
    let json_str = extract_json_from_response(text);
    serde_json::from_str::<MutationDiff>(json_str).context("failed to parse MutationDiff from LLM response")
}

fn extract_json_from_response(text: &str) -> &str {
    let trimmed = text.trim();
    let stripped = trimmed
        .strip_prefix("```json")
        .or_else(|| trimmed.strip_prefix("```"))
        .map(|s| s.trim_start())
        .and_then(|s| s.strip_suffix("```"))
        .map(|s| s.trim_end());
    stripped.unwrap_or(trimmed)
}

fn format_validation_errors(errors: &[ValidationError]) -> String {
    assert!(
        !errors.is_empty(),
        "format_validation_errors called with empty slice"
    );
    errors
        .iter()
        .map(|e| {
            if let Some(path) = &e.path {
                format!("- [{}] {} (at {})", e.code, e.message, path)
            } else {
                format!("- [{}] {}", e.code, e.message)
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_strategy() -> Strategy {
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000A",
                "display_name": "Apply Test Strategy",
                "plain_summary": "Minimal strategy for apply/identity tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000A", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            }
        });
        serde_json::from_value(v).expect("fixture strategy must deserialise")
    }

    fn diff_with(params: Vec<ParamChange>, added: Vec<String>, removed: Vec<String>) -> MutationDiff {
        MutationDiff {
            kind: MutationKind::Param,
            prose: Vec::new(),
            params,
            tools: ToolDiff { added, removed },
            filter: Vec::new(),
            create_filter: None,
            rationale: "test".into(),
        }
    }

    #[test]
    fn apply_to_adds_and_removes_tools() {
        let base = fixture_strategy();
        let diff = diff_with(vec![], vec!["macd".into()], vec!["rsi".into()]);
        let child = diff.apply_to(&base);
        assert!(child.manifest.required_tools.contains(&"macd".to_string()));
        assert!(!child.manifest.required_tools.contains(&"rsi".to_string()));
    }

    #[test]
    fn apply_to_routes_risk_param_into_risk_config() {
        // F14/F20: the real tunable surface. `risk.<field>` (and the bare field)
        // must land on the typed risk config, not be dumped into mechanical_params.
        let base = fixture_strategy();
        let before = base.risk.stop_loss_atr_multiple;
        for key in ["risk.stop_loss_atr_multiple", "stop_loss_atr_multiple"] {
            let diff = diff_with(
                vec![ParamChange {
                    key: key.into(),
                    before: serde_json::json!(before),
                    after: serde_json::json!(3.5),
                }],
                vec![],
                vec![],
            );
            let child = diff.apply_to(&base);
            assert_eq!(
                child.risk.stop_loss_atr_multiple, 3.5,
                "key {key} must update risk"
            );
            // And it's a real change, not an identity no-op.
            assert!(
                !is_identity_diff(&diff, &base),
                "risk change must not be identity for {key}"
            );
        }
    }

    #[test]
    fn tunable_keys_include_unprotected_risk_fields() {
        let base = fixture_strategy();
        let keys = tunable_param_keys(&base);
        // Unprotected risk fields that ARE tunable.
        assert!(keys.contains(&"risk.risk_pct_per_trade".to_string()));
        assert!(keys.contains(&"risk.max_concurrent_positions".to_string()));
        assert!(keys.contains(&"risk.max_position_pct_nav".to_string()));
        // Protected engine params must NOT be in the tunable set.
        for protected in PROTECTED_ENGINE_PARAMS {
            assert!(
                !keys.contains(&format!("risk.{protected}")),
                "protected engine param risk.{protected} must not be tunable"
            );
        }
    }

    #[test]
    fn applicable_kinds_allow_prose_when_strategy_has_an_agent() {
        let base = fixture_strategy(); // has a trader AgentRef
        let allowed = vec!["prose".into(), "param".into(), "tool".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(
            kinds.contains(&"prose".to_string()),
            "prose now has a home (AgentRef override)"
        );
        assert!(
            kinds.contains(&"param".to_string()),
            "param always applicable (risk exists)"
        );
    }

    #[test]
    fn applicable_kinds_drop_prose_when_strategy_has_no_agents() {
        let mut base = fixture_strategy();
        base.agents.clear();
        let allowed = vec!["prose".into(), "param".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(
            !kinds.contains(&"prose".to_string()),
            "no agent => no prompt home => prose excluded"
        );
    }

    #[test]
    fn applicable_kinds_offer_filter_when_strategy_has_no_filter() {
        // xvision-vxn: a filterless strategy can now be helped by the filter
        // lever via structural CREATION, so "filter" must be offered even when
        // base.filter is None (previously excluded).
        let base = fixture_strategy(); // no filter
        assert!(base.filter.is_none(), "fixture must be filterless");
        let allowed = vec!["param".into(), "filter".into()];
        let kinds = applicable_mutation_kinds(&base, &allowed);
        assert!(
            kinds.contains(&"filter".to_string()),
            "filter must be applicable on a filterless strategy (create path); got {kinds:?}"
        );
    }

    #[test]
    fn mutation_diff_schema_exposes_create_filter() {
        // xvision-vxn: the LLM response schema must carry create_filter so a
        // grammar-constrained writer can emit a structural filter creation.
        let fmt = crate::agent::llm::ResponseSchema::mutation_diff().openai_response_format();
        let s = serde_json::to_string(&fmt).expect("schema serializes");
        assert!(
            s.contains("create_filter"),
            "mutation_diff schema must expose create_filter; got:\n{s}"
        );
    }

    #[test]
    fn identity_diff_detected_for_noop_change() {
        let base = fixture_strategy();
        let current_sl = serde_json::json!(base.risk.stop_loss_atr_multiple);
        // Setting a tunable risk param to its current value is a no-op at the
        // hash level.
        let noop = diff_with(
            vec![ParamChange {
                key: "risk.stop_loss_atr_multiple".into(),
                before: current_sl.clone(),
                after: current_sl,
            }],
            vec![],
            vec![],
        );
        assert!(
            is_identity_diff(&noop, &base),
            "no-op param change must be identity"
        );

        // An empty diff is also identity.
        assert!(is_identity_diff(&empty_mutation(), &base));

        // A real change to a kept tunable surface is not identity.
        let real = diff_with(
            vec![ParamChange {
                key: "risk.stop_loss_atr_multiple".into(),
                before: serde_json::json!(base.risk.stop_loss_atr_multiple),
                after: serde_json::json!(base.risk.stop_loss_atr_multiple + 1.5),
            }],
            vec![],
            vec![],
        );
        assert!(!is_identity_diff(&real, &base));
    }

    #[test]
    fn build_user_payload_includes_memory_section_when_present() {
        let kinds = vec!["param".to_string()];
        let keys = vec!["risk.max_leverage".to_string()];
        let filter_paths: Vec<(String, serde_json::Value)> = vec![];
        let ctx = "param risk.max_leverage 1.0→3.0 ⇒ ΔSharpe -0.30 (rejected)";
        let with = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            7,
            0,
            Some(ctx),
            0,
            &[],
            0,
            true,
        );
        assert!(
            with.contains("Advisory optimizer context"),
            "advisory section header missing: {with}"
        );
        assert!(with.contains(ctx), "advisory context text missing: {with}");
        // F32 exploration directive must still be present alongside memory.
        assert!(
            with.contains("Exploration directive"),
            "F32 exploration section must remain: {with}"
        );

        // None / empty → no memory section, but F32 exploration still present.
        let without = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            7,
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        assert!(
            !without.contains("Advisory optimizer context"),
            "advisory section must be absent when None: {without}"
        );
        assert!(
            without.contains("Exploration directive"),
            "F32 exploration section must remain when no memory: {without}"
        );

        let empty = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            7,
            0,
            Some("   "),
            0,
            &[],
            0,
            true,
        );
        assert!(
            !empty.contains("Advisory optimizer context"),
            "blank advisory context must be treated as absent: {empty}"
        );
    }
    #[test]
    fn build_user_payload_includes_filter_paths_when_filter_kind_allowed() {
        let kinds = vec!["filter".to_string(), "param".to_string()];
        let keys = vec!["risk.max_leverage".to_string()];
        let filter_paths = vec![
            ("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0)),
            ("cooldown_bars".to_string(), serde_json::json!(3u32)),
        ];
        let payload = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            5,
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        assert!(
            payload.contains("Tunable filter paths"),
            "filter section header must be present: {payload}"
        );
        assert!(
            payload.contains("conditions.0.rhs.numeric"),
            "filter path must be listed: {payload}"
        );
        assert!(
            payload.contains("cooldown_bars"),
            "cooldown_bars path must be listed: {payload}"
        );

        // When filter is NOT in allowed kinds, section must be absent.
        let kinds_no_filter = vec!["param".to_string()];
        let no_filter_payload = build_user_payload(
            "prog",
            &kinds_no_filter,
            &keys,
            &filter_paths,
            None,
            5,
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        assert!(
            !no_filter_payload.contains("Tunable filter paths"),
            "filter section must be absent when filter not in allowed kinds: {no_filter_payload}"
        );
    }

    #[test]
    fn build_user_payload_annotates_filter_path_domain_constraints() {
        // B17: window-op filter paths (e.g. zscore_lt) require a positive integer
        // >= 1; `within_pct` requires a positive number > 0. The mutator must state
        // these per-path constraints in the enumerated filter list so the model
        // stops proposing zscore_lt=0 (which the validator correctly rejects,
        // wasting an attempt). A plain numeric path (rhs.numeric) must NOT get the
        // integer marker.
        let kinds = vec!["filter".to_string()];
        let keys: Vec<String> = vec![];
        let filter_paths = vec![
            ("conditions.0.op.zscore_lt".to_string(), serde_json::json!(3)),
            ("conditions.1.op.within_pct".to_string(), serde_json::json!(1.5)),
            ("conditions.2.rhs.numeric".to_string(), serde_json::json!(25.0)),
        ];
        let payload = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            5,
            0,
            None,
            0,
            &[],
            0,
            true,
        );

        // The window-op path must be listed AND annotated as a positive integer.
        assert!(
            payload.contains("conditions.0.op.zscore_lt"),
            "zscore_lt path must be listed: {payload}"
        );
        assert!(
            payload.contains("positive integer"),
            "window-op path must be annotated 'positive integer': {payload}"
        );
        // within_pct must be annotated as a positive number.
        assert!(
            payload.contains("positive number"),
            "within_pct path must be annotated 'positive number': {payload}"
        );
        // A plain numeric path must NOT carry the integer marker. Isolate the line
        // for rhs.numeric and confirm it lacks the "positive integer" hint.
        let numeric_line = payload
            .lines()
            .find(|l| l.contains("conditions.2.rhs.numeric"))
            .expect("rhs.numeric line present");
        assert!(
            !numeric_line.contains("positive integer"),
            "plain numeric path must not get the integer marker: {numeric_line}"
        );
    }

    #[test]
    fn prompt_template_states_window_op_positive_integer_constraint() {
        // B17 prompt guard: the system prompt template must tell the model that
        // window operators (e.g. zscore_lt) require a positive integer >= 1.
        assert!(
            PROMPT_TEMPLATE.contains("zscore_lt"),
            "prompt must mention a window op such as zscore_lt"
        );
        assert!(
            PROMPT_TEMPLATE.contains("positive integer"),
            "prompt must state the positive-integer constraint for window ops"
        );
    }

    #[test]
    fn build_user_payload_hides_param_keys_when_param_not_allowed() {
        // codex P2: a filter-only config must NOT show param keys or focus the
        // exploration directive on a risk.* parameter — that steers the model to
        // propose a param mutation it would then return as the diff.
        let kinds = vec!["filter".to_string()];
        let keys = vec![
            "risk.max_leverage".to_string(),
            "risk.stop_loss_atr_multiple".to_string(),
        ];
        let filter_paths = vec![
            ("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0)),
            ("cooldown_bars".to_string(), serde_json::json!(3u32)),
        ];
        let payload = build_user_payload(
            "prog",
            &kinds,
            &keys,
            &filter_paths,
            None,
            7,
            0,
            None,
            0,
            &[],
            0,
            true,
        );

        // Param key list and risk.* references must be absent.
        assert!(
            !payload.contains("risk.max_leverage"),
            "param key must not appear in filter-only payload: {payload}"
        );
        assert!(
            !payload.contains("Tunable parameter keys"),
            "param keys section header must not appear: {payload}"
        );
        // Exploration directive must focus on a FILTER path, not a param key.
        assert!(
            !payload.contains("risk.stop_loss_atr_multiple"),
            "exploration focus must not name a risk.* param in filter-only mode: {payload}"
        );
        // Filter section must still appear.
        assert!(
            payload.contains("Tunable filter paths"),
            "filter section must still be present: {payload}"
        );
    }

    #[test]
    fn exploration_focus_rotates_across_prose_filter_and_param_kinds() {
        // Issues 1/2 (QA 2026-06-08) regression: when `param` is allowed (the
        // default), the focus directive used to ALWAYS name a `risk.*` param, so
        // the prose (prompt) and filter levers were never focused and the
        // experiment-writer mutated only risk config every cycle. The focus must
        // now rotate across the applicable KINDS so prose and filter each get
        // focused on a fixed cadence.
        let kinds = vec!["prose".to_string(), "filter".to_string(), "param".to_string()];
        let keys = vec!["risk.stop_loss_atr_multiple".to_string()];
        let filter_paths = vec![("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0))];
        let prose_roles = vec!["trader".to_string()];

        let mut saw_prose = false;
        let mut saw_filter = false;
        let mut saw_param = false;
        for seed in 0..6u64 {
            let p = build_user_payload(
                "prog",
                &kinds,
                &keys,
                &filter_paths,
                None,
                seed,
                seed as usize,
                None,
                0,
                &prose_roles,
                0,
                true,
            );
            // Exactly one lever is focused per cycle (the three directive
            // signatures are mutually exclusive).
            let prose_hit = p.contains("agent's system prompt");
            let filter_hit = p.contains("filter path `conditions.0.rhs.numeric`");
            let param_hit = p.contains("parameter `risk.stop_loss_atr_multiple`");
            let hits = [prose_hit, filter_hit, param_hit].iter().filter(|b| **b).count();
            assert_eq!(
                hits, 1,
                "exactly one lever must be focused per cycle (seed {seed}): {p}"
            );
            saw_prose |= prose_hit;
            saw_filter |= filter_hit;
            saw_param |= param_hit;
        }
        assert!(saw_prose, "the PROSE lever must be focused on some cycle");
        assert!(saw_filter, "the FILTER lever must be focused on some cycle");
        assert!(saw_param, "the PARAM lever must still be focused on some cycle");
    }

    #[test]
    fn exploration_focus_skips_prose_when_no_agent_roles() {
        // Prose must only be focused when the strategy actually has a prose-capable
        // agent role to carry the override; with no roles the rotation falls back
        // to filter/param and never names a prose directive.
        let kinds = vec!["prose".to_string(), "param".to_string()];
        let keys = vec!["risk.max_leverage".to_string()];
        let filter_paths: Vec<(String, serde_json::Value)> = vec![];
        for seed in 0..4u64 {
            let p = build_user_payload(
                "prog",
                &kinds,
                &keys,
                &filter_paths,
                None,
                seed,
                seed as usize,
                None,
                0,
                &[],
                0,
                true,
            );
            assert!(
                !p.contains("agent's system prompt"),
                "prose must not be focused when no agent roles are available (seed {seed}): {p}"
            );
            assert!(
                p.contains("parameter `risk.max_leverage`"),
                "param lever must be focused when it is the only focusable kind (seed {seed}): {p}"
            );
        }
    }

    #[test]
    fn apply_to_writes_prose_into_trader_prompt_override() {
        let base = fixture_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: "trader".into(),
                before: String::new(),
                after: "Trade only with-trend; size down in chop.".into(),
            }],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: Vec::new(),
            create_filter: None,
            rationale: "test".into(),
        };
        let child = diff.apply_to(&base);
        let trader = child
            .agents
            .iter()
            .find(|a| a.canonical_role() == "trader")
            .unwrap();
        assert_eq!(
            trader.prompt.as_str(),
            "Trade only with-trend; size down in chop."
        );
        // And it is a REAL change (distinct content hash), not an identity no-op.
        assert!(
            !is_identity_diff(&diff, &base),
            "prose change must alter the strategy"
        );
    }

    #[test]
    fn apply_to_prose_for_unknown_role_is_noop() {
        // A prose edit naming a role no strategy agent plays leaves the strategy
        // unchanged (validator rejects it upstream; apply stays total/safe).
        let base = fixture_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Prose,
            prose: vec![ProseEdit {
                agent_role: "nonexistent".into(),
                before: String::new(),
                after: "x".into(),
            }],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: Vec::new(),
            create_filter: None,
            rationale: "t".into(),
        };
        assert!(
            is_identity_diff(&diff, &base),
            "unknown-role prose must be a no-op"
        );
    }

    fn fixture_filter_strategy() -> Strategy {
        // Strategy with a Filter: ADX > 25, cooldown_bars=3
        let v = serde_json::json!({
            "manifest": {
                "id": "01HZTEST00000000000000000F",
                "display_name": "Filter Test Strategy",
                "plain_summary": "Strategy with a filter for mutation tests.",
                "creator": "@test",
                "template": "custom",
                "regime_fit": [],
                "asset_universe": ["BTC/USD"],
                "decision_cadence_minutes": 60,
                "required_tools": ["rsi"],
                "risk_preset_or_config": "balanced"
            },
            "agents": [{"agent_id": "01HZAGENT0000000000000000F", "role": "trader"}],
            "risk": {
                "risk_pct_per_trade": 0.01,
                "max_concurrent_positions": 1,
                "max_leverage": 1.0,
                "stop_loss_atr_multiple": 2.0,
                "daily_loss_kill_pct": 0.05
            },
            "activation_mode": "filter_gated",
            "filter": {
                "id": "01HZFILTER000000000000000A",
                "strategy_id": "01HZTEST00000000000000000F",
                "display_name": "ADX Filter",
                "asset_scope": ["BTC/USD"],
                "timeframe": "1h",
                "conditions": {
                    "all": [
                        { "lhs": "adx_14", "op": ">", "rhs": 25.0 }
                    ]
                },
                "cooldown_bars": 3
            }
        });
        serde_json::from_value(v).expect("fixture filter strategy must deserialise")
    }

    #[test]
    fn filter_tunable_paths_returns_rhs_numeric_and_cooldown_bars() {
        let base = fixture_filter_strategy();
        let filter = base.filter.as_ref().expect("fixture has a filter");
        let paths = filter_tunable_paths(filter);
        let path_map: std::collections::HashMap<String, serde_json::Value> = paths.into_iter().collect();

        // rhs of condition 0 is Numeric(25.0) → should appear as "conditions.0.rhs.numeric"
        assert!(
            path_map.contains_key("conditions.0.rhs.numeric"),
            "expected conditions.0.rhs.numeric in paths: {:?}",
            path_map.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            path_map["conditions.0.rhs.numeric"],
            serde_json::json!(25.0),
            "rhs numeric value should be 25.0"
        );

        // cooldown_bars = 3
        assert!(
            path_map.contains_key("cooldown_bars"),
            "expected cooldown_bars in paths"
        );
        assert_eq!(path_map["cooldown_bars"], serde_json::json!(3u32));

        // max_wakeups_per_day = null (None)
        assert!(
            path_map.contains_key("max_wakeups_per_day"),
            "expected max_wakeups_per_day in paths"
        );
        assert!(
            path_map["max_wakeups_per_day"].is_null(),
            "max_wakeups_per_day should be null when None"
        );
    }

    #[test]
    fn filter_kind_mutation_diff_serde_round_trip() {
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".to_string(),
                before: serde_json::json!(25.0),
                after: serde_json::json!(28.0),
            }],
            create_filter: None,
            rationale: "increase ADX threshold for stronger trend signal".into(),
        };
        let json = serde_json::to_string(&diff).expect("serialize");
        // Verify the kind serializes as "filter"
        assert!(
            json.contains("\"filter\""),
            "kind must serialize as \"filter\": {json}"
        );
        let restored: MutationDiff = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.kind, MutationKind::Filter);
        assert_eq!(restored.filter.len(), 1);
        assert_eq!(restored.filter[0].path, "conditions.0.rhs.numeric");
        assert_eq!(restored.filter[0].before, serde_json::json!(25.0));
        assert_eq!(restored.filter[0].after, serde_json::json!(28.0));
    }

    #[test]
    fn apply_to_filter_edit_changes_rhs_threshold() {
        let base = fixture_filter_strategy();
        // Initial rhs is 25.0 from fixture; change it to 28.0
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".to_string(),
                before: serde_json::json!(25.0),
                after: serde_json::json!(28.0),
            }],
            create_filter: None,
            rationale: "increase ADX threshold".into(),
        };
        let child = diff.apply_to(&base);
        let filter = child.filter.as_ref().expect("child must have a filter");
        let cond = filter
            .conditions
            .leaves_dfs()
            .into_iter()
            .next()
            .expect("one condition");
        match &cond.rhs {
            Operand::Numeric(v) => {
                assert!((v - 28.0).abs() < 1e-9, "rhs should be 28.0, got {v}");
            }
            other => panic!("expected Numeric rhs, got {other:?}"),
        }
        assert!(
            !is_identity_diff(&diff, &base),
            "filter change must not be identity"
        );
    }

    // ── xvision-vxn: structural filter CREATION ──────────────────────────────

    #[test]
    fn apply_to_create_filter_installs_authored_filter_and_gates() {
        // Applying a create_filter diff to a filterless strategy installs the
        // authored filter, stamps its strategy_id to the parent, and flips
        // activation to FilterGated (mirrors the operator authoring path).
        let base = fixture_strategy(); // no filter
        assert!(base.filter.is_none(), "fixture must be filterless");
        let mut diff = empty_mutation();
        diff.kind = MutationKind::Filter;
        diff.create_filter = Some(serde_json::json!({
            "display_name": "RSI oversold gate (optimizer-created)",
            "asset_scope": ["BTC/USD"],
            "timeframe": "1h",
            "conditions": { "all": [ { "lhs": "rsi_14", "op": "<", "rhs": 30.0 } ] },
            "cooldown_bars": 3
        }));
        diff.rationale = "add an oversold/throttle filter to a filterless strategy".into();

        let child = diff.apply_to(&base);
        let filter = child
            .filter
            .as_ref()
            .expect("create_filter must install a filter");
        assert_eq!(
            filter.strategy_id.as_str(),
            base.manifest.id.as_str(),
            "installed filter must be stamped with the parent strategy id"
        );
        assert_eq!(
            child.activation_mode,
            xvision_filters::ActivationMode::FilterGated,
            "installing a filter must gate activation"
        );
        assert!(
            !is_identity_diff(&diff, &base),
            "creating a filter must not be an identity no-op"
        );
    }

    #[test]
    fn apply_to_create_filter_is_noop_when_filter_already_present() {
        // create_filter must never clobber an existing filter — that case is a
        // TUNE (filter edits), not a create.
        let base = fixture_filter_strategy();
        let original = base.filter.clone().expect("fixture has a filter");
        let mut diff = empty_mutation();
        diff.kind = MutationKind::Filter;
        diff.create_filter = Some(serde_json::json!({
            "display_name": "should be ignored",
            "asset_scope": ["BTC/USD"],
            "timeframe": "1h",
            "conditions": { "all": [ { "lhs": "rsi_14", "op": "<", "rhs": 30.0 } ] }
        }));
        let child = diff.apply_to(&base);
        assert_eq!(
            child.filter.as_ref(),
            Some(&original),
            "an existing filter must not be replaced by create_filter"
        );
    }

    #[test]
    fn build_user_payload_offers_filter_creation_when_strategy_has_no_filter() {
        // When `filter` is allowed and the strategy has no filter, the prompt
        // must invite the writer to AUTHOR one via create_filter.
        let kinds = vec!["param".to_string(), "filter".to_string()];
        let keys = vec!["risk.risk_pct_per_trade".to_string()];
        let filter_paths: Vec<(String, serde_json::Value)> = vec![]; // no filter
        let out = build_user_payload(
            "PROGRAM",
            &kinds,
            &keys,
            &filter_paths,
            None,
            3,
            0,
            None,
            0,
            &[],
            0,
            false,
        );
        assert!(
            out.contains("create_filter"),
            "filterless + filter allowed must invite create_filter; got:\n{out}"
        );
    }

    #[test]
    fn build_user_payload_no_create_offer_when_filter_exists() {
        let kinds = vec!["filter".to_string()];
        let keys: Vec<String> = vec![];
        let filter_paths = vec![("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0))];
        let out = build_user_payload(
            "PROGRAM",
            &kinds,
            &keys,
            &filter_paths,
            None,
            3,
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        assert!(
            !out.contains("create_filter"),
            "a strategy that already has a filter must not be offered create_filter; got:\n{out}"
        );
    }

    #[test]
    fn apply_to_filter_edit_unknown_path_is_noop() {
        let base = fixture_filter_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.99.rhs.numeric".to_string(), // invalid index
                before: serde_json::json!(25.0),
                after: serde_json::json!(50.0),
            }],
            create_filter: None,
            rationale: "invalid path".into(),
        };
        // Should be identity since path doesn't resolve
        assert!(
            is_identity_diff(&diff, &base),
            "unknown path filter edit must be a no-op"
        );
    }

    #[test]
    fn filter_tunable_paths_symmetry_property() {
        // PROPERTY TEST: for every (path, cur) from filter_tunable_paths,
        // set_filter_value returns true and a re-walk shows the new value at path.
        let base = fixture_filter_strategy();
        let filter_orig = base.filter.as_ref().expect("fixture has filter");
        let paths = filter_tunable_paths(filter_orig);
        assert!(!paths.is_empty(), "fixture filter should have tunable paths");
        for (path, cur) in &paths {
            let new_val: serde_json::Value = if cur.is_null() {
                serde_json::json!(5u32) // set max_wakeups_per_day to some value
            } else if let Some(n) = cur.as_f64() {
                serde_json::json!(n + 1.0) // increment by 1
            } else {
                continue; // skip non-numeric (shouldn't happen)
            };
            let mut filter_copy = filter_orig.clone();
            let applied = set_filter_value(&mut filter_copy, path, &new_val);
            assert!(applied, "set_filter_value must return true for path '{path}'");
            // Re-walk to verify the new value
            let new_paths: std::collections::HashMap<String, serde_json::Value> =
                filter_tunable_paths(&filter_copy).into_iter().collect();
            assert!(
                new_paths.contains_key(path.as_str()),
                "path '{path}' must still be present after set"
            );
            let found = &new_paths[path.as_str()];
            // For null→u32 case (max_wakeups_per_day), new_val is 5
            if cur.is_null() {
                assert_eq!(
                    found,
                    &serde_json::json!(5u32),
                    "path '{path}' should show new value"
                );
            } else {
                // Numeric comparison allowing f64 rounding
                let expected = new_val.as_f64().unwrap();
                let actual = found.as_f64().unwrap_or(f64::NAN);
                assert!(
                    (actual - expected).abs() < 1e-6,
                    "path '{path}' expected {expected}, got {actual}"
                );
            }
        }
    }

    #[test]
    fn describe_mutation_outcome_param_change_compact_form() {
        let diff = diff_with(
            vec![ParamChange {
                key: "risk.stop_loss_atr_multiple".into(),
                before: serde_json::json!(2.0),
                after: serde_json::json!(3.5),
            }],
            vec![],
            vec![],
        );
        let desc = describe_mutation_outcome(&diff, -0.40, "rejected", None);
        assert!(desc.contains("risk.stop_loss_atr_multiple"), "{desc}");
        assert!(desc.contains("2.0→3.5"), "{desc}");
        assert!(desc.contains("ΔSharpe -0.40"), "{desc}");
        assert!(desc.contains("(rejected)"), "{desc}");
        assert_eq!(desc.lines().count(), 1, "must be one line: {desc}");
    }

    #[test]
    fn describe_mutation_outcome_includes_gate_failure_dimensions() {
        let diff = diff_with(
            vec![ParamChange {
                key: "risk.max_position_pct_nav".into(),
                before: serde_json::json!(0.25),
                after: serde_json::json!(0.50),
            }],
            vec![],
            vec![],
        );
        let desc = describe_mutation_outcome(
            &diff,
            2.83,
            "rejected",
            Some(MutationGateContext {
                objective_label: "sharpe",
                delta_day: Some(2.83),
                delta_holdout: Some(-1.12),
                drawdown_ratio: Some(1.82),
                parent_n_trades: Some(26),
                child_n_trades: Some(18),
                min_trade_retention_ratio: Some(0.5),
                parent_realized_return_ratio: Some(0.70),
                child_realized_return_ratio: Some(0.20),
                min_realized_return_ratio: Some(0.25),
                reason: Some(
                    "baseline-untouched-score (sharpe) improved by -1.120000; max drawdown deteriorated",
                ),
            }),
        );
        assert!(desc.contains("sharpe Δday +2.8300"), "{desc}");
        assert!(desc.contains("Δholdout -1.1200"), "{desc}");
        assert!(desc.contains("drawdown 1.82×"), "{desc}");
        assert!(desc.contains("trades 18/26"), "{desc}");
        assert!(desc.contains("realized child 0.20"), "{desc}");
        assert!(desc.contains("baseline-untouched-score"), "{desc}");
        assert_eq!(desc.lines().count(), 1, "must be one line: {desc}");
    }

    #[test]
    fn strategy_diff_detects_prose_change() {
        let mut a = fixture_strategy();
        a.agents[0].prompt = "buy low".to_string();
        let mut b = a.clone();
        b.agents[0].prompt = "sell high".to_string();
        let diff = strategy_diff(&a, &b);
        assert_eq!(diff.prose.len(), 1);
        assert_eq!(diff.prose[0].before, "buy low");
        assert_eq!(diff.prose[0].after, "sell high");
        assert!(diff.params.is_empty());
    }

    #[test]
    fn strategy_diff_identical_strategies_empty() {
        let s = fixture_strategy();
        let diff = strategy_diff(&s, &s);
        assert!(diff.prose.is_empty());
        assert!(diff.params.is_empty());
        assert!(diff.tools.added.is_empty());
        assert!(diff.tools.removed.is_empty());
        assert!(diff.filter.is_empty());
    }

    #[test]
    fn retry_rotates_focus_param_across_attempts() {
        // F32 (run-7): successive retry attempts must use a different focus param
        // so the exploration directive names a different key each attempt. With
        // the old code, `exploration_seed` was fixed and `param_keys[seed % len]`
        // never changed across retries — `already_tried` became unescapable.
        let keys: Vec<String> = (0..4).map(|i| format!("risk.k{i}")).collect();
        // Use a base seed where wrapping_add(1) selects a different key.
        // With 4 keys: seed % 4 vs (seed+1) % 4 differ unless seed is a multiple
        // of 4 wrapping-around exactly — pick seed=9 for robustness.
        let base_seed = 9u64;
        let filter_paths: Vec<(String, serde_json::Value)> = vec![];
        let p0 = build_user_payload(
            "prog",
            &["param".to_string()],
            &keys,
            &filter_paths,
            None,
            base_seed.wrapping_add(0),
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        let p1 = build_user_payload(
            "prog",
            &["param".to_string()],
            &keys,
            &filter_paths,
            None,
            base_seed.wrapping_add(1),
            0,
            None,
            0,
            &[],
            0,
            true,
        );
        // The focus directive must name a different key for attempt 0 vs attempt 1.
        // Since `build_user_payload` embeds the focus in the exploration_section,
        // the payloads must differ when the seed selects different keys.
        assert_ne!(
            p0, p1,
            "attempt 0 and attempt 1 must produce different exploration directives (different focus params)"
        );
        // Also verify the key selected for seed=9 differs from seed=10.
        let key0 = &keys[(base_seed as usize) % keys.len()];
        let key1 = &keys[(base_seed.wrapping_add(1) as usize) % keys.len()];
        assert_ne!(
            key0, key1,
            "seed 9 and seed 10 must index different keys in a 4-key list"
        );
        assert!(
            p0.contains(key0.as_str()),
            "attempt 0 payload must mention the focus key for seed 9: {p0}"
        );
        assert!(
            p1.contains(key1.as_str()),
            "attempt 1 payload must mention the focus key for seed 10: {p1}"
        );
    }

    #[test]
    fn retry_rotates_focus_kind_across_attempts() {
        // B6: the experiment writer must NOT re-focus the SAME mutation kind on
        // every retry. With `mutation_idx` held fixed for the whole propose call,
        // the kind index used to be `focus_groups[mutation_idx % n_kinds]` — a
        // constant — so every retry kept hammering the same kind and the budget
        // was consumed by one failing kind. The `attempt` arg now offsets the kind
        // index so successive attempts focus DIFFERENT kinds.
        let kinds = vec!["prose".to_string(), "filter".to_string(), "param".to_string()];
        let keys = vec!["risk.stop_loss_atr_multiple".to_string()];
        let filter_paths = vec![("conditions.0.rhs.numeric".to_string(), serde_json::json!(25.0))];
        let prose_roles = vec!["trader".to_string()];

        // Hold mutation_idx FIXED at a value that (with attempt 0) maps to param,
        // then vary the new attempt arg across 0,1,2 and collect the focused kind.
        // focus_groups order is [prose, filter, param]; mutation_idx=2 => param at
        // attempt 0.
        let mutation_idx = 2usize;
        let mut focused_kinds = std::collections::BTreeSet::new();
        for attempt in 0..3usize {
            let p = build_user_payload(
                "prog",
                &kinds,
                &keys,
                &filter_paths,
                None,
                7, // exploration_seed fixed: only the kind index should change
                mutation_idx,
                None,
                0,
                &prose_roles,
                attempt,
                true,
            );
            // Detect kind via the existing directive substrings the tests already use.
            if p.contains("agent system prompt") || p.contains("agent's system prompt") {
                focused_kinds.insert("prose");
            } else if p.contains("filter path") {
                focused_kinds.insert("filter");
            } else if p.contains("parameter `") {
                focused_kinds.insert("param");
            }
        }
        assert!(
            focused_kinds.len() >= 2,
            "across attempts 0,1,2 (mutation_idx fixed) at least 2 distinct kinds must be focused, saw: {focused_kinds:?}"
        );
    }

    // ── WU3a: mechanistic.* tunable paths ─────────────────────────────────────

    fn fixture_mechanistic_config() -> crate::strategies::MechanisticConfig {
        use crate::strategies::{ClosePolicy, MechanisticConfig};
        MechanisticConfig {
            entry_rules: vec![],
            close_policies: vec![
                ClosePolicy::StopLoss { pct: 2.0 },
                ClosePolicy::TimeExit { bars: 10 },
            ],
        }
    }

    #[test]
    fn mechanistic_tunable_paths_enumerates_correct_leaf_paths() {
        let cfg = fixture_mechanistic_config();
        let paths = mechanistic_tunable_paths(&cfg);
        let path_map: std::collections::HashMap<String, serde_json::Value> = paths.into_iter().collect();

        // StopLoss at index 0 → .pct
        assert!(
            path_map.contains_key("mechanistic.close_policies.0.pct"),
            "expected mechanistic.close_policies.0.pct; got {:?}",
            path_map.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            path_map["mechanistic.close_policies.0.pct"],
            serde_json::json!(2.0),
            "StopLoss pct must be 2.0"
        );

        // TimeExit at index 1 → .bars
        assert!(
            path_map.contains_key("mechanistic.close_policies.1.bars"),
            "expected mechanistic.close_policies.1.bars; got {:?}",
            path_map.keys().collect::<Vec<_>>()
        );
        assert_eq!(
            path_map["mechanistic.close_policies.1.bars"],
            serde_json::json!(10u32),
            "TimeExit bars must be 10"
        );

        // Must NOT emit .bars for index 0 (StopLoss) or .pct for index 1 (TimeExit)
        assert!(
            !path_map.contains_key("mechanistic.close_policies.0.bars"),
            "StopLoss must not emit .bars path"
        );
        assert!(
            !path_map.contains_key("mechanistic.close_policies.1.pct"),
            "TimeExit must not emit .pct path"
        );

        // Exactly 2 paths total
        assert_eq!(path_map.len(), 2, "exactly 2 tunable paths expected");
    }

    #[test]
    fn set_mechanistic_value_round_trip_pct() {
        use crate::strategies::{ClosePolicy, MechanisticConfig};
        let mut cfg = fixture_mechanistic_config();

        // Set index 0 (.pct on StopLoss) to 3.5
        let result = set_mechanistic_value(
            &mut cfg,
            "mechanistic.close_policies.0.pct",
            &serde_json::json!(3.5),
        );
        assert!(
            result.is_ok(),
            "setting .pct on StopLoss must succeed: {result:?}"
        );

        // Get-back via tunable paths
        let paths: std::collections::HashMap<String, serde_json::Value> =
            mechanistic_tunable_paths(&cfg).into_iter().collect();
        let val = paths["mechanistic.close_policies.0.pct"].as_f64().unwrap();
        assert!((val - 3.5).abs() < 1e-9, "round-trip: expected 3.5, got {val}");

        // Also verify the underlying variant is still StopLoss
        assert!(
            matches!(cfg.close_policies[0], ClosePolicy::StopLoss { pct } if (pct - 3.5).abs() < 1e-9),
            "underlying variant must remain StopLoss with pct=3.5"
        );
    }

    #[test]
    fn set_mechanistic_value_cross_variant_mismatch_returns_error_and_does_not_mutate() {
        use crate::strategies::MechanisticConfig;
        let mut cfg = fixture_mechanistic_config();
        let cfg_before = cfg.clone();

        // index 1 is TimeExit{bars:10}; trying to set .pct on it is a cross-variant mismatch
        let result = set_mechanistic_value(
            &mut cfg,
            "mechanistic.close_policies.1.pct",
            &serde_json::json!(5.0),
        );
        assert!(
            result.is_err(),
            "cross-variant mismatch (.pct on TimeExit) must return an error"
        );
        assert_eq!(
            cfg, cfg_before,
            "config must not be mutated on cross-variant mismatch"
        );
    }

    #[test]
    fn filter_only_strategy_tunable_path_set_unchanged_regression() {
        // Regression: a strategy with a filter and no mechanistic_config must
        // still expose the same filter paths it did before WU3a.
        let base = fixture_filter_strategy();
        assert!(
            base.mechanistic_config.is_none(),
            "fixture has no mechanistic config"
        );
        let filter = base.filter.as_ref().expect("fixture has a filter");
        let paths = filter_tunable_paths(filter);
        let path_map: std::collections::HashMap<String, serde_json::Value> = paths.into_iter().collect();

        // Must still include conditions.0.rhs.numeric and cooldown_bars
        assert!(
            path_map.contains_key("conditions.0.rhs.numeric"),
            "filter-only strategy must still expose conditions.0.rhs.numeric"
        );
        assert!(
            path_map.contains_key("cooldown_bars"),
            "filter-only strategy must still expose cooldown_bars"
        );
        // Must NOT include any mechanistic.* paths from filter_tunable_paths
        assert!(
            !path_map.keys().any(|k| k.starts_with("mechanistic.")),
            "filter_tunable_paths must not emit mechanistic.* keys"
        );
    }

    // ── WU-B: tunable bounds clamp tests ─────────────────────────────────────

    /// Build a Strategy whose `tunable_bounds` has two entries:
    ///   - `conditions.0.rhs.numeric`  → Int [2, 50, step=1]
    ///   - `mechanistic.close_policies.0.pct` → Float [0.5, 10.0, step=none]
    /// The strategy also carries a filter (so filter edits resolve) and a
    /// mechanistic config (so mechanistic writes apply).
    fn fixture_bounded_strategy() -> Strategy {
        use crate::strategies::pine_import::inputs::InputKind;
        use crate::strategies::{ClosePolicy, MechanisticConfig, TunableBound};

        // Build from the filter fixture (which has conditions.0.rhs.numeric = 25.0)
        // and layer in mechanistic_config + tunable_bounds.
        let mut s = fixture_filter_strategy();
        s.mechanistic_config = Some(MechanisticConfig {
            entry_rules: vec![],
            close_policies: vec![ClosePolicy::StopLoss { pct: 2.0 }],
        });
        s.tunable_bounds = vec![
            TunableBound {
                path: "conditions.0.rhs.numeric".to_string(),
                min: Some(2.0),
                max: Some(50.0),
                step: Some(1.0),
                kind: InputKind::Int,
            },
            TunableBound {
                path: "mechanistic.close_policies.0.pct".to_string(),
                min: Some(0.5),
                max: Some(10.0),
                step: None,
                kind: InputKind::Float,
            },
        ];
        s
    }

    #[test]
    fn clamp_to_bound_int_clamps_above_max() {
        // A filter edit proposing 999 on a bound with max=50 must be clamped to 50.
        let base = fixture_bounded_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".to_string(),
                before: serde_json::json!(25.0),
                after: serde_json::json!(999),
            }],
            create_filter: None,
            rationale: "out-of-range test".into(),
        };
        let child = diff.apply_to(&base);
        let filter = child.filter.as_ref().expect("child must have a filter");
        let cond = filter
            .conditions
            .leaves_dfs()
            .into_iter()
            .next()
            .expect("one condition");
        match &cond.rhs {
            Operand::Numeric(v) => {
                assert!(
                    (v - 50.0).abs() < 1e-9,
                    "Int bound: 999 clamped to max=50, got {v}"
                );
            }
            other => panic!("expected Numeric rhs, got {other:?}"),
        }
    }

    #[test]
    fn clamp_to_bound_int_clamps_below_min() {
        // A filter edit proposing 0 on a bound with min=2 must be clamped to 2.
        let base = fixture_bounded_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".to_string(),
                before: serde_json::json!(25.0),
                after: serde_json::json!(0),
            }],
            create_filter: None,
            rationale: "below-min test".into(),
        };
        let child = diff.apply_to(&base);
        let filter = child.filter.as_ref().expect("child must have a filter");
        let cond = filter
            .conditions
            .leaves_dfs()
            .into_iter()
            .next()
            .expect("one condition");
        match &cond.rhs {
            Operand::Numeric(v) => {
                assert!((v - 2.0).abs() < 1e-9, "Int bound: 0 clamped to min=2, got {v}");
            }
            other => panic!("expected Numeric rhs, got {other:?}"),
        }
    }

    #[test]
    fn clamp_to_bound_int_step_alignment() {
        // In-range value 7.3 with step=1 must be rounded to nearest integer (7).
        let base = fixture_bounded_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Filter,
            prose: vec![],
            params: vec![],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![FilterEdit {
                path: "conditions.0.rhs.numeric".to_string(),
                before: serde_json::json!(25.0),
                after: serde_json::json!(7.3),
            }],
            create_filter: None,
            rationale: "step-alignment test".into(),
        };
        let child = diff.apply_to(&base);
        let filter = child.filter.as_ref().expect("child must have a filter");
        let cond = filter
            .conditions
            .leaves_dfs()
            .into_iter()
            .next()
            .expect("one condition");
        match &cond.rhs {
            Operand::Numeric(v) => {
                assert!(
                    (v - 7.0).abs() < 1e-9,
                    "Int kind: 7.3 should round to 7.0, got {v}"
                );
            }
            other => panic!("expected Numeric rhs, got {other:?}"),
        }
    }

    #[test]
    fn clamp_to_bound_float_mechanistic_clamped() {
        // A mechanistic param write proposing 50.0 on a bound max=10.0 clamps to 10.0.
        let base = fixture_bounded_strategy();
        let diff = MutationDiff {
            kind: MutationKind::Param,
            prose: vec![],
            params: vec![ParamChange {
                key: "mechanistic.close_policies.0.pct".to_string(),
                before: serde_json::json!(2.0),
                after: serde_json::json!(50.0),
            }],
            tools: ToolDiff {
                added: vec![],
                removed: vec![],
            },
            filter: vec![],
            create_filter: None,
            rationale: "mechanistic clamp test".into(),
        };
        let child = diff.apply_to(&base);
        use crate::strategies::ClosePolicy;
        let mc = child
            .mechanistic_config
            .as_ref()
            .expect("must have mechanistic config");
        match &mc.close_policies[0] {
            ClosePolicy::StopLoss { pct } => {
                assert!(
                    (pct - 10.0).abs() < 1e-9,
                    "Float bound: 50.0 clamped to max=10.0, got {pct}"
                );
            }
            other => panic!("expected StopLoss, got {other:?}"),
        }
    }

    #[test]
    fn clamp_to_bound_int_rounds_and_clamps() {
        // Int kind: clamp to [min, max], then round to nearest integer.
        use crate::strategies::pine_import::inputs::InputKind;
        use crate::strategies::TunableBound;
        let bound = TunableBound {
            path: "x".to_string(),
            min: Some(5.0),
            max: Some(20.0),
            step: None,
            kind: InputKind::Int,
        };
        assert_eq!(
            clamp_to_bound(&serde_json::json!(999), &bound),
            serde_json::json!(20.0),
            "999 must clamp to max=20"
        );
        assert_eq!(
            clamp_to_bound(&serde_json::json!(1), &bound),
            serde_json::json!(5.0),
            "1 must clamp up to min=5"
        );
    }

    #[test]
    fn clamp_to_bound_bool_coerces() {
        // Bool kind: 0 / false / null → false; any truthy numeric → true.
        use crate::strategies::pine_import::inputs::InputKind;
        use crate::strategies::TunableBound;
        let bound = TunableBound {
            path: "x".to_string(),
            min: None,
            max: None,
            step: None,
            kind: InputKind::Bool,
        };
        assert_eq!(
            clamp_to_bound(&serde_json::json!(0), &bound),
            serde_json::json!(false),
            "0 must coerce to false"
        );
        assert_eq!(
            clamp_to_bound(&serde_json::json!(1), &bound),
            serde_json::json!(true),
            "truthy numeric must coerce to true"
        );
    }

    #[test]
    fn unbound_risk_path_writes_proposed_value_unchanged() {
        // A risk path NOT in tunable_bounds must be written exactly as proposed
        // (no clamping). risk.* is a kept tunable surface post mechanical_params
        // removal.
        let base = fixture_strategy();
        let diff = diff_with(
            vec![ParamChange {
                key: "risk.stop_loss_atr_multiple".into(),
                before: serde_json::json!(base.risk.stop_loss_atr_multiple),
                after: serde_json::json!(3.5),
            }],
            vec![],
            vec![],
        );
        let child = diff.apply_to(&base);
        assert_eq!(
            child.risk.stop_loss_atr_multiple, 3.5,
            "unbound risk path must write the proposed value unchanged"
        );
    }
}
