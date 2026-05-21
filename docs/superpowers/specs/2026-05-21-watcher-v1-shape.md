# Watcher v1 — Shape Spec

> Status: build-ready narrowing of the 2026-05-19 watchers proposal
> (`docs/superpowers/specs/2026-05-19-eval-evidence-and-agent-watchers.md`).
> This document is the source of truth for the v1 surface area: data model,
> DSL, validation rules, runtime semantics, export/UI hooks, and acceptance
> criteria. Everything outside the v1 cuts below is deliberately deferred.

## One-sentence frame

A watcher is a deterministic, user-authored bar-close predicate that decides
whether the strategy's downstream LLM agent is woken on a given bar. The
watcher watches; the agent decides.

## Why we are doing this now

- The 2026-05-21 QA rerun (intake `2026-05-21-eval-honesty-and-agent-graph.md`)
  found the trader LLM running on every bar with ~10k input tokens per call
  and ~432/24h decisions, most of which were guardrail-blocked or no-ops.
  Watchers let the user opt into "wake only when X happens" without changing
  agent semantics.
- The `agent-graph-composition` track in the same intake formalizes
  `AgentRef.kind ∈ {trader, watcher, critic, intern}`. Watcher v1 is the
  first concrete kind besides `trader` and is the foundation that work
  will compose on.
- The 2026-05-19 watchers spec is comprehensive but spans five phases;
  it is too large to ship as one contract. This document is the v1 slice
  that the implementation plan decomposes.

## v1 cuts (what is in)

1. **One activation mode added to strategies: `watcher_gated`.**
   Existing `every_bar` behavior is preserved as the default. v1 does not
   ship `compiled_rules`.
2. **One watcher per strategy.** Multi-watcher composition and watcher
   marketplace artifacts are out of v1.
3. **Five operators, six indicators.** A constrained DSL evaluated
   deterministically against the engine's existing `IndicatorPanel`.
4. **Backtest-only runtime.** No durable daemon, no live/paper wake-ups,
   no kill-switch integration in v1.
5. **Watcher events on the export.** A new `WatcherEventV1` row stream
   alongside `DecisionTraceV2`, and a `watcher_summary` block on the run
   summary.
6. **Read-only watcher card in the agent editor.** Authoring round-trip
   (NL → card → save) is v1.1. v1 ships a JSON/TOML editor pane plus a
   server-side validator.
7. **Position-management wake-ups limited to `on_invalidation_or_target_only`
   or `every_bar_when_in_position`.** No custom management DSL.

## v1 cuts (what is out, named so we don't drift)

- Live/paper daemon, durable scans, audit log, notifications.
- Natural-language → watcher compilation in strategy chat.
- Chart preview of historical triggers (handled by post-run watcher event
  rendering in v1; live preview is v1.2).
- Marketplace publication of watcher artifacts.
- Multi-asset / multi-timeframe scans inside one watcher.
- User-named signals emitted from watcher → downstream agent briefing.
  That belongs to `agent-graph-composition` proper, not v1 shape.
- Compiled-rule strategies sharing the DSL.

If any of these become load-bearing during implementation, push a
contract-update PR before code.

## Terminology lock

| Concept | Use | Don't use |
|---|---|---|
| Deterministic per-bar predicate evaluator | `Watcher` | `trigger engine`, `predicate graph` |
| One fired evaluation | `wake-up` | `event dispatch`, `trigger event` |
| Activation mode field on `Strategy` | `activation_mode` | `mode`, `gating`, `cadence` |
| Compact context payload to the agent on wake | `WakeBriefing` | `compact context`, `mini brief` |
| Per-bar evaluator output | `WatcherEvent` | `watcher result`, `scan tick` |

These names are normative. Public types use them verbatim; UI copy may
soften to "watcher" / "wake-up" / "wake-up reason".

## Data model

### `Watcher`

```rust
/// One deterministic predicate gating an agent's wake-ups.
/// Persisted as JSON inside `Strategy.watcher` (Option<Watcher>).
pub struct Watcher {
    pub schema_version: u32,              // = 1
    pub display_name: String,
    pub description: Option<String>,
    pub asset: String,                    // single asset in v1; must match scenario asset
    pub timeframe: String,                // "1h", "15m", "1d"; must match scenario timeframe
    pub scan: ScanCadence,                // BarClose only in v1
    pub conditions: WatcherConditions,    // see below
    pub cooldown_bars: u32,               // 0 = no cooldown
    pub max_wakeups_per_day: Option<u32>, // None = uncapped
    pub when_in_position: InPositionPolicy,
}

pub enum ScanCadence { BarClose }

pub enum InPositionPolicy {
    SuppressWakeups,                    // never wake while in position
    EveryBar,                           // wake every bar when in position
    OnInvalidationOrTargetOnly,         // engine-driven; see "management wake-ups"
}

pub struct WatcherConditions {
    pub all: Vec<Condition>,            // AND-joined; v1 has no `any` block
}

pub struct Condition {
    pub lhs: Operand,
    pub op: Op,
    pub rhs: Operand,
}

pub enum Operand {
    Indicator(IndicatorRef),            // ema_20, rsi_14, atr_pct, close, ...
    Value(f64),
    Range { low: f64, high: f64 },      // only valid as rhs of `Between`
}

pub enum Op {
    Gt, Lt, Gte, Lte, Eq,
    CrossesAbove, CrossesBelow,         // requires bar(i-1) and bar(i)
    Between,                            // rhs must be Range
}

pub enum IndicatorRef {
    Close, Open, High, Low, Volume,
    Ema12, Ema26,
    Sma20, Sma50, Sma200,
    Rsi14,
    Atr14,
    AtrPct,                             // atr_14 / close
    BbUpper, BbMiddle, BbLower,
}
```

The set of `IndicatorRef` variants is exactly the set the existing
`crates/xvision-data/src/indicators.rs::compute_panel_from_fixture`
populates today, plus the derived `AtrPct`. **No new indicator math
is in v1.** MACD is excluded because the panel does not yet compute it.

### `Strategy.activation_mode` and `Strategy.watcher`

```rust
pub enum ActivationMode {
    EveryBar,                           // current behavior; default
    WatcherGated,                       // requires Strategy.watcher = Some(_)
}

pub struct Strategy {
    // ...existing fields...
    pub activation_mode: ActivationMode, // default EveryBar
    pub watcher: Option<Watcher>,        // required iff WatcherGated
}
```

Serde-default both fields so existing strategies parse unchanged.

### Touchpoint on `AgentRef`

v1 **does not** add `AgentRef.kind`. The `agent-graph-composition` track
owns that field. v1 implicitly treats the strategy's existing trader-role
agent as the wake target. When `agent-graph-composition` lands,
`Strategy.watcher` becomes the canonical config a kind-watcher agent
reads; the runtime evaluator does not change shape.

### `WatcherEventV1` (export row)

```rust
pub struct WatcherEventV1 {
    pub decision_index: u32,            // matches the bar's would-be decision_index
    pub timestamp: DateTime<Utc>,
    pub bar_ref: BarRef,                // sha256 of (asset, timeframe, ts, ohlcv)
    pub outcome: WatcherOutcome,
    pub conditions: WatcherConditionEval,
}

pub enum WatcherOutcome {
    Fired { wake_reason: String },      // wake_reason is Watcher.display_name in v1
    SuppressedByCooldown { remaining_bars: u32 },
    SuppressedByDailyCap { cap: u32, used: u32 },
    SuppressedByInPosition { policy: InPositionPolicy },
    DidNotMatch { failed_count: u32 },
}

pub struct WatcherConditionEval {
    pub passed: Vec<ConditionMatch>,
    pub failed: Vec<ConditionMatch>,
}

pub struct ConditionMatch {
    pub lhs_label: String,              // e.g. "ema_20"
    pub op_label: String,
    pub rhs_label: String,              // e.g. "ema_50" or "0.6"
    pub lhs_value: Option<f64>,
    pub rhs_value: Option<f64>,
}
```

One row per scanned bar. The export size is bounded by the scenario bar
count, not by wake-up count.

### `watcher_summary` (run summary block)

```json
{
  "watcher_summary": {
    "scanned_bars": 30,
    "fired": 4,
    "suppressed": { "cooldown": 1, "daily_cap": 0, "in_position": 5, "no_match": 20 },
    "llm_calls_saved_estimate": 26,
    "input_tokens_saved_estimate": null,
    "wake_reasons": { "Trend Pullback Wake-up": 4 }
  }
}
```

`input_tokens_saved_estimate` is `null` in v1 because the engine does
not yet track per-call token economics deterministically. The
`eval-token-efficiency` track will fill it in.

## DSL (on-disk authoring form)

Watchers may be authored as JSON or TOML. The TOML form is the canonical
human edit surface:

```toml
[watcher]
schema_version = 1
display_name = "Trend Pullback Wake-up"
asset = "BTC/USD"
timeframe = "1h"
scan = "bar_close"
cooldown_bars = 3
max_wakeups_per_day = 2
when_in_position = "on_invalidation_or_target_only"

[[conditions.all]]
lhs = "ema_20"
op  = ">"
rhs = "ema_50"

[[conditions.all]]
lhs = "close"
op  = "crosses_above"
rhs = "ema_20"

[[conditions.all]]
lhs = "rsi_14"
op  = ">"
value = 50

[[conditions.all]]
lhs = "atr_pct"
op  = "between"
range = [0.6, 2.5]
```

JSON has the same field names with snake_case `op` values (`gt`, `lt`,
`crosses_above`, `between`, etc.). The Rust parser accepts both via serde
with `#[serde(rename_all = "snake_case")]` and a small symbol→variant
table for TOML ergonomics.

## Validation (server-side, refuses save on any failure)

1. `schema_version == 1`. Forward-compatible only; older schemas reject.
2. `asset` and `timeframe` parse and match the strategy's bound scenario(s)
   when known; otherwise warn but allow save.
3. Every `IndicatorRef` is in the v1 enum.
4. `Op::Between` requires `rhs = Range { low <= high }`. All other ops
   reject `Range`.
5. `Op::CrossesAbove`/`CrossesBelow` require both operands to be
   indicators (not literal values). The engine needs `t-1` and `t` for
   both sides.
6. `cooldown_bars <= 1000` and `max_wakeups_per_day <= 1440`. Soft caps;
   exceeded means save rejected.
7. No reference to future bars. (Trivially enforced because the DSL has
   no offset operator in v1.)
8. `conditions.all.len() >= 1` and `<= 8`. A watcher with zero conditions
   fires every bar — that is what `EveryBar` mode is for, so reject.
9. `activation_mode == WatcherGated` ⇒ `Strategy.watcher.is_some()`.
   `EveryBar` ⇒ `Strategy.watcher` may be `Some` but is ignored at runtime.
10. `display_name` is 1–80 chars; control characters rejected.

Validation lives in `crates/xvision-engine/src/strategies/watcher_validate.rs`
and is called from both the strategy save API and the eval pre-flight
(`eval-provider-preflight` already exists; watcher validation runs in the
same preflight pass).

## Runtime semantics — backtest

The per-bar loop lives at
`crates/xvision-engine/src/eval/executor/backtest.rs:496`
(`for (i, bar) in bars.iter().enumerate()`). v1 hooks immediately before
the trader LLM call:

```text
for (i, bar) in bars.iter().enumerate():
    indicators_at_i  = panel_at(bars, i)               # already computed today
    indicators_at_im1 = panel_at(bars, i-1)            # new: needed for crosses_*
    if activation_mode == EveryBar:
        call_trader_agent(...)
        continue

    outcome = watcher.evaluate(indicators_at_i, indicators_at_im1,
                               state.cooldown, state.day_wake_count,
                               position_state)
    record WatcherEventV1 { decision_index=i, outcome, conditions }
    match outcome:
        Fired { wake_reason } => {
            wake_briefing = build_wake_briefing(...)
            call_trader_agent(wake_briefing)
            state.cooldown = watcher.cooldown_bars
            state.day_wake_count += 1
        }
        _ => skip trader call (no decision row, no LLM cost)
```

Key invariants:

- **Determinism.** Given identical bars + identical watcher JSON, the
  sequence of `WatcherEventV1` rows is byte-identical across runs. No
  wall-clock, no RNG, no provider variability inputs.
- **Causality.** Watcher evaluation at bar `i` sees only data through
  bar `i`'s close. The validator forbids future references; the
  evaluator API never receives `bars[i+1..]`.
- **Cooldown semantics.** A `Fired` outcome sets cooldown to
  `cooldown_bars`. The next `cooldown_bars` bars that would otherwise
  fire are recorded as `SuppressedByCooldown`. Cooldown decrements one
  per bar regardless of match.
- **Daily cap semantics.** "Day" means UTC calendar day of the bar
  timestamp. Cap resets at 00:00 UTC. If a bar matches but the cap is
  exhausted, the outcome is `SuppressedByDailyCap`, not `Fired`.
- **In-position semantics.** Determined from the engine's existing
  position-state snapshot at the start of bar `i`. Policy applies before
  cooldown and daily-cap checks; the recorded outcome carries the policy.
  v1's `OnInvalidationOrTargetOnly` defers to the engine's existing
  invalidation/target detection; if the engine cannot resolve "near
  invalidation/target" deterministically for the current bar, the policy
  collapses to `SuppressWakeups` and the export records that with a
  note in `ConditionMatch.lhs_label = "in_position_resolver_unavailable"`.

### `WakeBriefing` (v1)

When the watcher fires, the briefing the trader agent receives is the
**existing** trader briefing minus warmup bars and indicator pre-stuffing,
plus a `wake_reason` field naming the watcher and the `passed` condition
labels. v1 does not change the briefing schema beyond that addition. The
"compact context" reductions from the 2026-05-19 spec are deferred to
`eval-token-efficiency`.

```jsonc
{
  // ...existing trader briefing fields...
  "wake": {
    "reason": "Trend Pullback Wake-up",
    "watcher_id": null,                  // null in v1 (no separate watcher_id surface)
    "passed_conditions": [
      "ema_20 > ema_50",
      "close crosses_above ema_20",
      "rsi_14 > 50",
      "atr_pct between [0.6, 2.5]"
    ]
  }
}
```

## Acceptance criteria

A v1 implementation is done when **all** of the following hold:

1. `Strategy` round-trips through serde with default
   `activation_mode = EveryBar` and `watcher = None`; existing fixtures
   parse unchanged.
2. A strategy saved with `activation_mode = WatcherGated` and a valid
   `Watcher` survives a save/load round-trip via the dashboard strategy
   API.
3. The save API rejects all ten validation failure modes above with
   structured error codes (`watcher.schema_version`, `watcher.asset`,
   `watcher.op_range_required`, ... — one code per rule).
4. A backtest run on an existing BTC/USD 1h scenario with
   `activation_mode = EveryBar` produces the same `eval_decisions` rows
   it produced on `main` at the branch point. (Regression guard.)
5. The same scenario run with `activation_mode = WatcherGated` and the
   fixture watcher in `docs/superpowers/specs/fixtures/watcher-v1-pullback.toml`
   produces:
   - `watcher_summary.scanned_bars` = total bar count after warmup.
   - `watcher_summary.fired` + `watcher_summary.suppressed.*` +
     `no_match` = `scanned_bars`.
   - `eval_decisions` row count = `watcher_summary.fired` plus any
     in-position-driven wakes.
6. The export includes one `WatcherEventV1` row per scanned bar.
7. Two runs of step 5 produce byte-identical `watcher_summary` and
   byte-identical `WatcherEventV1` row sequences.
8. `cargo test -p xvision-engine` and `cargo test --workspace` pass.
9. `pnpm --dir frontend/web typecheck` passes after `ts-rs` re-export.
10. `bash scripts/board-lint.sh` passes on the contract.

## Touchpoints (where code lands)

| Layer | File(s) | Purpose |
|---|---|---|
| Domain | `crates/xvision-engine/src/strategies/watcher.rs` (new) | `Watcher`, `WatcherConditions`, `Op`, `IndicatorRef`, `InPositionPolicy`, `ActivationMode` |
| Domain | `crates/xvision-engine/src/strategies/mod.rs` | Add `activation_mode`, `watcher` fields to `Strategy`; serde defaults |
| Domain | `crates/xvision-engine/src/strategies/watcher_validate.rs` (new) | Validation rules 1–10, structured error codes |
| Runtime | `crates/xvision-engine/src/eval/watcher_eval.rs` (new) | Pure per-bar evaluator: `(panel_t, panel_t_minus_1, state) → WatcherOutcome` |
| Runtime | `crates/xvision-engine/src/eval/executor/backtest.rs` | Wire evaluator into the per-bar loop; emit `WatcherEventV1`; skip trader call on non-fire |
| Export | `crates/xvision-engine/src/eval/export.rs` | `WatcherEventV1` rows; `watcher_summary` block |
| API | `crates/xvision-engine/src/api/eval.rs` | Surface `watcher_summary` on run detail |
| API | `crates/xvision-dashboard/src/routes/strategies*` (existing) | Accept/reject watcher JSON via save endpoint |
| Frontend types | `frontend/web/src/api/types.gen/Watcher.ts` etc. via `ts-rs` | TS bindings for the new types |
| Frontend editor | `frontend/web/src/components/strategy/WatcherCard.tsx` (new, read-only in v1) | Render condition cards + scan summary; JSON pane is the v1 edit surface |
| Frontend run | `frontend/web/src/features/eval-runs/review/WatcherEventsPanel.tsx` (new) | Render `WatcherEventV1` rows + `watcher_summary` |
| Fixture | `docs/superpowers/specs/fixtures/watcher-v1-pullback.toml` (new) | The acceptance-criterion-5 fixture |
| Migration | `crates/xvision-engine/migrations/032_strategy_watcher.sql` (new) | Add `activation_mode TEXT NOT NULL DEFAULT 'every_bar'`, `watcher_json TEXT NULL` |

## Risk and rollback

- **Risk: silent regression in `EveryBar` runs.** Mitigation: acceptance
  criterion #4 (byte-identical decisions vs `main`) is the long pole.
- **Risk: cross-day cap drift.** Mitigation: cap resets are computed
  from bar timestamps, not wall clock; covered by determinism tests.
- **Risk: in-position resolver gap.** Mitigation: explicit collapse to
  `SuppressWakeups` with an export note, so it surfaces in review rather
  than producing surprising trader calls.
- **Rollback:** the migration is additive and the new fields default to
  `EveryBar` / NULL. Reverting the engine code while leaving the
  migration in place is safe.

## Open questions deferred (not v1 blockers)

- Multi-asset / multi-timeframe scans inside one watcher.
- Watcher reuse across strategies / marketplace shape.
- DSL versioning policy beyond `schema_version = 1`.
- How `agent-graph-composition` exposes watcher-emitted signals as
  edges in the graph.

These are open questions, not v1 bugs. Each gets its own intake when
v1 is in.

## References

- `docs/superpowers/specs/2026-05-19-eval-evidence-and-agent-watchers.md`
  — full design proposal this spec narrows.
- `team/intake/2026-05-21-eval-honesty-and-agent-graph.md` — context for
  the agent-graph composition primitive Watcher will live inside.
- `crates/xvision-engine/src/eval/executor/backtest.rs:496` — the bar
  loop the evaluator hooks into.
- `crates/xvision-data/src/indicators.rs::compute_panel_from_fixture`
  — the panel the v1 `IndicatorRef` set draws from.
