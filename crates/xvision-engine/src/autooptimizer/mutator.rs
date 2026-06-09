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

/// A compact, human-readable one-line summary of a gated candidate's outcome,
/// suitable for recording as an Observation in [`MUTATIONS_NS`] (and later
/// recall once distilled into a Pattern).
///
/// For a single-param change it reads e.g.
/// `param risk.stop_loss_atr_multiple 2.0→3.5 ⇒ ΔSharpe -0.40 (rejected)`; for
/// prose/tool diffs it falls back to a short kind-based summary plus the same
/// `⇒ ΔSharpe {:+.2} ({status_label})` suffix. Always a single line.
pub fn describe_mutation_outcome(diff: &MutationDiff, delta_sharpe: f64, status_label: &str) -> String {
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
    // Strip any embedded newlines defensively so the result is always one line.
    let lever = lever.replace('\n', " ");
    format!("{lever} ⇒ ΔSharpe {delta_sharpe:+.2} ({status_label})")
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

/// One incremental change to a numeric threshold inside the strategy's typed
/// `Filter` AST, addressed by a stable dotted path (see `filter_tunable_paths`).
/// Examples:
///   `path = "conditions.0.rhs.numeric"`, `before = 25.0`, `after = 28.0`
///   `path = "conditions.0.op.within_pct"`, `before = 1.5`, `after = 2.0`
///   `path = "cooldown_bars"`, `before = 3`, `after = 6`
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FilterEdit {
    pub path: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProseEdit {
    pub agent_role: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamChange {
    pub key: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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
        rationale: String::new(),
    }
}

/// Numeric `RiskConfig` fields the mutator may tune via `risk.<field>` param
/// keys. F14/F20 (QA 2026-06-04): the real strategies on the node all have an
/// empty `mechanical_params`; their only tunable knobs live in `risk`, so
/// without this the optimizer could never produce a valid param experiment for
/// any real strategy. Keep in sync with `RiskConfig` (xvision-risk).
pub const RISK_PARAM_FIELDS: &[&str] = &[
    "risk_pct_per_trade",
    "max_concurrent_positions",
    "max_leverage",
    "stop_loss_atr_multiple",
    "daily_loss_kill_pct",
    "max_position_pct_nav",
];

/// If `key` addresses a tunable `risk` field — either `risk.<field>` or a bare
/// `<field>` that isn't shadowed by a `mechanical_params` key — return the field
/// name; otherwise `None` (the key targets `mechanical_params`).
pub fn risk_field_for_key(base: &Strategy, key: &str) -> Option<String> {
    if let Some(field) = key.strip_prefix("risk.") {
        return RISK_PARAM_FIELDS.contains(&field).then(|| field.to_string());
    }
    let shadowed_by_mechanical = base
        .mechanical_params
        .as_object()
        .map(|m| m.contains_key(key))
        .unwrap_or(false);
    if !shadowed_by_mechanical && RISK_PARAM_FIELDS.contains(&key) {
        return Some(key.to_string());
    }
    None
}

/// The param keys an experiment may target on `base`: every `mechanical_params`
/// top-level key plus `risk.<field>` for each tunable risk knob. Used to tell
/// the experiment writer which keys exist (F21) and to render a helpful
/// `unknown_param` error.
pub fn tunable_param_keys(base: &Strategy) -> Vec<String> {
    let mut keys = Vec::new();
    if let Some(mp) = base.mechanical_params.as_object() {
        for (k, v) in mp {
            // Only scalar leaves are directly tunable.
            if !v.is_object() && !v.is_array() {
                keys.push(k.clone());
            }
        }
    }
    for f in RISK_PARAM_FIELDS {
        keys.push(format!("risk.{f}"));
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

/// The mutation kinds that are *structurally applicable* to `base`, intersected
/// with the operator-allowed kinds (F21). `param` is applicable whenever the
/// strategy exposes a tunable key (always, since every strategy has a `risk`
/// config). `tool` stays as allowed. `prose` is applicable when the strategy
/// has at least one `AgentRef` to carry a `prompt_override` (Phase 0 substrate):
/// a prose edit sets `AgentRef.prompt_override` and changes the strategy content
/// hash, so it is a real change — not a no-op — on any agent strategy. For
/// agentless/pre-refactor strategies there is still no home, so prose is
/// excluded there. `filter` is applicable when the strategy has a `filter`
/// (`base.filter.is_some()`); the AST-walk tunable-path enumeration and
/// apply/validate support (Phase 2) back this arm.
pub fn applicable_mutation_kinds(base: &Strategy, allowed: &[String]) -> Vec<String> {
    let has_params = !tunable_param_keys(base).is_empty();
    // Prose is applicable iff the strategy has at least one agent to carry a
    // `prompt_override` (Phase 0). For agentless/pre-refactor strategies there
    // is still no home, so prose stays excluded there.
    let has_prompt_home = !base.agents.is_empty();
    // Filter is applicable when the strategy has a typed Filter to walk.
    let has_filter = base.filter.is_some();
    allowed
        .iter()
        .filter(|k| match k.as_str() {
            "param" => has_params,
            "tool" => true,
            "prose" => has_prompt_home,
            "filter" => has_filter,
            _ => false,
        })
        .cloned()
        .collect()
}

impl MutationDiff {
    pub fn is_empty(&self) -> bool {
        self.prose.is_empty()
            && self.params.is_empty()
            && self.tools.added.is_empty()
            && self.tools.removed.is_empty()
            && self.filter.is_empty()
    }

    /// Apply this diff to `base`, returning the candidate strategy.
    ///
    /// This is the **canonical** apply used by the cycle orchestrator, the
    /// inversion-pair check, the `mutate-once` CLI verb, and the mutator's own
    /// identity check, so all of them agree on what a diff actually changes. It
    /// applies:
    ///   - `params` targeting `risk.<field>` (or a bare risk-field name): routed
    ///     into the typed `risk` config via a JSON round-trip (F14/F20 — this is
    ///     the only tunable surface real strategies have).
    ///   - `params` otherwise: dot-path keys into `mechanical_params` (nested
    ///     objects are created as needed).
    ///   - `tools`: add/remove against `manifest.required_tools`.
    ///   - `prose`: each `ProseEdit` sets the matching `AgentRef.prompt_override`
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
            if let Some(field) = risk_field_for_key(base, &change.key) {
                if let Some(obj) = risk_json.as_object_mut() {
                    obj.insert(field, change.after.clone());
                    risk_touched = true;
                }
            } else {
                set_param_value(&mut s.mechanical_params, &change.key, change.after.clone());
            }
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
                a.prompt_override = Some(edit.after.clone());
            }
        }
        // Filter edits resolve path → AST node and write `after`. An unresolved
        // path or a wrong-type value is a silent no-op (validator rejects those
        // upstream; apply stays total). The filter field is cloned before mutation
        // so a partial-failure edit doesn't leave the filter half-changed.
        if let Some(ref mut f) = s.filter {
            for edit in &self.filter {
                // Ignore the return value; validator already ensured the path
                // resolves and the value has the right type.
                set_filter_value(f, &edit.path, &edit.after);
            }
        }
        s
    }
}

/// Set `params[key] = value`, where `key` is a dot path (`a.b.c`). Missing
/// intermediate objects are created. A path that traverses a non-object value
/// is left unchanged rather than clobbering it.
fn set_param_value(params: &mut serde_json::Value, key: &str, value: serde_json::Value) {
    if key.is_empty() {
        return;
    }
    let parts: Vec<&str> = key.splitn(16, '.').collect();
    let (last, prefix) = parts.split_last().expect("splitn yields at least one part");
    if !params.is_object() {
        *params = serde_json::Value::Object(serde_json::Map::new());
    }
    let mut cur = params;
    for &part in prefix {
        let map = match cur.as_object_mut() {
            Some(m) => m,
            None => return,
        };
        cur = map
            .entry(part.to_string())
            .or_insert_with(|| serde_json::Value::Object(serde_json::Map::new()));
    }
    if let Some(map) = cur.as_object_mut() {
        map.insert(last.to_string(), value);
    }
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
        memory_context: Option<&str>,
        avoid: &std::collections::HashSet<ContentHash>,
    ) -> anyhow::Result<MutationDiff> {
        let program_md = program_view::to_markdown(base);
        let mut last_errors: Option<Vec<ValidationError>> = None;
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
                memory_context,
                avoid.len(),
                &prose_roles,
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
                response_schema: None,
                cache_control: None,
                force_json: true,
            };

            let resp = self
                .dispatch
                .complete(req)
                .await
                .with_context(|| format!("mutator dispatch failed on attempt {attempt}"))?;
            let raw_text = resp.text();

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

        anyhow::bail!("mutator failed after {} attempt(s): {}", max_attempts, error_text)
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

fn build_user_payload(
    program_md: &str,
    allowed_kinds: &[String],
    param_keys: &[String],
    filter_paths: &[(String, serde_json::Value)],
    previous_errors: Option<&[ValidationError]>,
    exploration_seed: u64,
    memory_context: Option<&str>,
    avoid_count: usize,
    // Issues 1/2 (QA 2026-06-08): the agent roles that can carry a
    // `prompt_override`, so the exploration focus can rotate onto the PROSE lever
    // (rewrite an agent's prompt) and not only numeric levers. Empty when prose is
    // not an applicable kind for this strategy.
    prose_roles: &[String],
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
    // Filter paths section: only included when "filter" is in allowed kinds and
    // the strategy has a filter (non-empty paths list). Guides the experiment
    // writer to propose valid dotted paths from the live AST.
    let filter_section = if allowed_kinds.iter().any(|k| k == "filter") && !filter_paths.is_empty() {
        format!(
            "\n\nTunable filter paths (a `filter` experiment's `path` MUST be exactly one of these; \
             `before` must match the current value shown):\n{}",
            filter_paths
                .iter()
                .map(|(p, v)| format!("  - {p}: {v}"))
                .collect::<Vec<_>>()
                .join("\n")
        )
    } else {
        String::new()
    };
    let errors_section = match previous_errors {
        None => String::new(),
        Some(errs) => {
            format!(
                "\n\nPrevious attempt errors — you MUST fix all of these:\n\n{}",
                format_validation_errors(errs)
            )
        }
    };

    // F32: a SUBSTANTIVE per-cycle exploration directive. The previous version
    // only passed a cosmetic "variant N" nonce + a non-zero temperature, which a
    // real model (e.g. gemini-flash-lite) ignores — it collapses the constrained
    // experiment space to the single most obvious tweak every cycle, so repeat
    // cycles re-derived the byte-identical candidate and never explored. Instead,
    // use the exploration seed to NAME a concrete focus the writer must experiment
    // on. Different cycles ⇒ different seed ⇒ different focus ⇒ a materially
    // different prompt ⇒ a different candidate, even from a fully deterministic
    // model. (Pairs with the hard `already_tried` reject in `propose`, which
    // guarantees a previously-seen candidate is never re-emitted.)
    //
    // Issues 1/2 (QA 2026-06-08): the focus must rotate across the applicable
    // mutation KINDS, not just param keys. The previous version only ever named a
    // `risk.*` param whenever `param` was allowed (the default), so a weak
    // experiment-writer was steered onto the numeric risk lever every single
    // cycle — prose (prompt) and filter levers were never focused and so never
    // exercised (QA: gemma mutated only risk.* across all 7 cycles). Build one
    // focus group per applicable, focusable kind in a fixed priority order, pick
    // the KIND by `seed % n_kinds` (balanced kind-level rotation, independent of
    // how many param keys exist), then pick a concrete target within that kind by
    // `seed / n_kinds`. This guarantees prose and filter each get focused on a
    // fixed cadence rather than losing every cycle to the 6 risk.* keys.
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
    let exploration_section = if focus_groups.is_empty() {
        format!(
            "\n\nExploration directive (variant {exploration_seed}): pick a different change than \
             the single most obvious one, so repeated runs explore rather than re-propose one tweak."
        )
    } else {
        let n_kinds = focus_groups.len();
        let (kind, targets) = &focus_groups[(exploration_seed as usize) % n_kinds];
        let target = &targets[((exploration_seed as usize) / n_kinds) % targets.len()];
        match *kind {
            "prose" => format!(
                "\n\nExploration directive (variant {exploration_seed}): FOCUS this experiment on the \
                 `{target}` agent's system prompt — propose a `prose` experiment that rewrites its \
                 trading logic, reasoning steps, or entry/exit criteria (NOT merely a number). This \
                 focus rotates the optimizer across its levers so successive runs explore the prompt, \
                 the filter, and the numeric parameters rather than re-proposing one fixed tweak. If \
                 the prompt genuinely cannot be improved, you may target another listed lever instead."
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
    };
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

    // P3: advisory cross-run/cross-framework memory. When recall surfaced prior
    // optimizer outcomes on similar strategies, prepend them before the final
    // instruction so the writer can build on wins and avoid repeating failures.
    // This is advisory ONLY — it does not relax the F32 exploration directive
    // above or the hard avoid-set (exact-repeat dedup) the orchestrator enforces.
    let memory_section = match memory_context {
        Some(ctx) if !ctx.trim().is_empty() => format!(
            "\n\nPrior optimizer outcomes on similar strategies (advisory — avoid repeating failures, build on wins):\n{ctx}"
        ),
        _ => String::new(),
    };

    format!(
        "Strategy program view:\n\n{program_md}\n\nAllowed experiment kinds: {kinds_text}{keys_section}{filter_section}{errors_section}{exploration_section}{no_repeat_section}{memory_section}\n\nPropose ONE experiment as a JSON object."
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
            },
            "mechanical_params": { "ema_fast": 12, "ema_slow": 26 }
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
            rationale: "test".into(),
        }
    }

    #[test]
    fn apply_to_sets_top_level_param() {
        let base = fixture_strategy();
        let diff = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(20),
            }],
            vec![],
            vec![],
        );
        let child = diff.apply_to(&base);
        assert_eq!(child.mechanical_params["ema_fast"], serde_json::json!(20));
        assert_eq!(child.mechanical_params["ema_slow"], serde_json::json!(26));
    }

    #[test]
    fn apply_to_creates_nested_param_path() {
        let base = fixture_strategy();
        let diff = diff_with(
            vec![ParamChange {
                key: "signals.rsi.period".into(),
                before: serde_json::Value::Null,
                after: serde_json::json!(14),
            }],
            vec![],
            vec![],
        );
        let child = diff.apply_to(&base);
        assert_eq!(
            child.mechanical_params["signals"]["rsi"]["period"],
            serde_json::json!(14)
        );
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
            assert!(
                child.mechanical_params.get("stop_loss_atr_multiple").is_none(),
                "risk param must not leak into mechanical_params for key {key}"
            );
            // And it's a real change, not an identity no-op.
            assert!(
                !is_identity_diff(&diff, &base),
                "risk change must not be identity for {key}"
            );
        }
    }

    #[test]
    fn tunable_keys_include_risk_fields() {
        let base = fixture_strategy();
        let keys = tunable_param_keys(&base);
        assert!(keys.contains(&"risk.stop_loss_atr_multiple".to_string()));
        assert!(keys.contains(&"risk.risk_pct_per_trade".to_string()));
        // mechanical_params scalar keys are included too.
        assert!(keys.contains(&"ema_fast".to_string()));
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
    fn identity_diff_detected_for_noop_change() {
        let base = fixture_strategy();
        // Setting a param to its current value is a no-op at the hash level.
        let noop = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(12),
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

        // A real change is not identity.
        let real = diff_with(
            vec![ParamChange {
                key: "ema_fast".into(),
                before: serde_json::json!(12),
                after: serde_json::json!(99),
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
        let with = build_user_payload("prog", &kinds, &keys, &filter_paths, None, 7, Some(ctx), 0, &[]);
        assert!(
            with.contains("Prior optimizer outcomes on similar strategies"),
            "memory section header missing: {with}"
        );
        assert!(with.contains(ctx), "memory context text missing: {with}");
        // F32 exploration directive must still be present alongside memory.
        assert!(
            with.contains("Exploration directive"),
            "F32 exploration section must remain: {with}"
        );

        // None / empty → no memory section, but F32 exploration still present.
        let without = build_user_payload("prog", &kinds, &keys, &filter_paths, None, 7, None, 0, &[]);
        assert!(
            !without.contains("Prior optimizer outcomes on similar strategies"),
            "memory section must be absent when None: {without}"
        );
        assert!(
            without.contains("Exploration directive"),
            "F32 exploration section must remain when no memory: {without}"
        );

        let empty = build_user_payload("prog", &kinds, &keys, &filter_paths, None, 7, Some("   "), 0, &[]);
        assert!(
            !empty.contains("Prior optimizer outcomes on similar strategies"),
            "blank memory context must be treated as absent: {empty}"
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
        let payload = build_user_payload("prog", &kinds, &keys, &filter_paths, None, 5, None, 0, &[]);
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
            None,
            0,
            &[],
        );
        assert!(
            !no_filter_payload.contains("Tunable filter paths"),
            "filter section must be absent when filter not in allowed kinds: {no_filter_payload}"
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
        let payload = build_user_payload("prog", &kinds, &keys, &filter_paths, None, 7, None, 0, &[]);

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
                None,
                0,
                &prose_roles,
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
            let p = build_user_payload("prog", &kinds, &keys, &filter_paths, None, seed, None, 0, &[]);
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
            rationale: "test".into(),
        };
        let child = diff.apply_to(&base);
        let trader = child
            .agents
            .iter()
            .find(|a| a.canonical_role() == "trader")
            .unwrap();
        assert_eq!(
            trader.prompt_override.as_deref(),
            Some("Trade only with-trend; size down in chop.")
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
            "mechanical_params": {},
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
        let desc = describe_mutation_outcome(&diff, -0.40, "rejected");
        assert!(desc.contains("risk.stop_loss_atr_multiple"), "{desc}");
        assert!(desc.contains("2.0→3.5"), "{desc}");
        assert!(desc.contains("ΔSharpe -0.40"), "{desc}");
        assert!(desc.contains("(rejected)"), "{desc}");
        assert_eq!(desc.lines().count(), 1, "must be one line: {desc}");
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
            None,
            0,
            &[],
        );
        let p1 = build_user_payload(
            "prog",
            &["param".to_string()],
            &keys,
            &filter_paths,
            None,
            base_seed.wrapping_add(1),
            None,
            0,
            &[],
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
}
