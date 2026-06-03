# Eval trader risk parity — model/config sizing, stop-loss & take-profit

**Status:** draft (awaiting review)
**Author:** Claude (paired with @latentwill)
**Date:** 2026-06-03
**Branch:** `codex/multi-asset-tool-asset-guard` (worktree `.worktrees/eval-multiasset-fixes`)
**Source QA:** `docs/QA/2026-06-03-deepseek-v4-multiasset-1h-eval-findings.md` (finding #3, plus the
`max_concurrent_positions` and metrics findings tracked separately as T3/T4)
**Sibling fix (landed on this branch):** T1 multi-asset bar-cache contamination —
`api/eval.rs::load_bars_for_scenario` now computes a per-asset cache key
(`load_bars_for_scenario_routes_per_asset_not_scenario_key` regression test).

---

## 1. Problem

The QA run found that a trend strategy's risk management is not honored in eval:

1. The model emits precise stop-loss / take-profit brackets **in prose**
   ("SL at ~100879 (1.5×ATR), TP at ~103453 (3×ATR)") and the harness discards
   them — they cannot be enforced.
2. `risk.stop_loss_atr_multiple` exists in config but is **applied nowhere**
   (zero references in `xvision-risk`; the eval executor never reads it).
3. There is **no take-profit mechanism at all** — neither config nor decision.
4. Position sizing is **mechanical** — `equity × risk.risk_pct_per_trade` — with
   no way for the model (or a per-decision signal) to express conviction-scaled
   size.

Net: the per-trade asymmetric SL/TP and sizing a trend strategy needs for a
high Sharpe are not expressible in eval.

### 1.1 Root cause (verified)

The QA write-up conflated two trader paths. The **production** trader prompt
(`crates/xvision-trader/src/prompt.rs:143-158`) does ask for
`stop_loss_pct`/`take_profit_pct`, but **eval does not run that path**. The eval
trader's actual contract is:

- **Output schema** (advertised in the agent starter templates,
  `crates/xvision-engine/src/agents/templates.rs` — e.g. lines 83, 137, 168,
  211, 244, 278, 337, 359, 382): `{action: long_open|short_open|flat|hold,
  conviction: 0..1, justification: string}` — **no size, no SL/TP**.
- **Parser** (`crates/xvision-engine/src/eval/executor/trader_output.rs:8-13`):
  `TraderOutput { action, conviction, justification }`, `#[serde(deny_unknown_fields)]`.
- **Sizing** (`crates/xvision-engine/src/eval/executor/backtest.rs:1457-1460`):
  `usd_at_risk = equity × risk.risk_pct_per_trade; qty = usd_at_risk / next_bar_open`.
- **Exit logic:** none. A position changes **only** when the model emits a new
  action on a wake bar. The backtest never checks a bar's range against a
  stop/target between decisions.

For contrast, the production decision type already carries the target shape:
`crates/xvision-core/src/trading.rs:196-217` —
`TraderDecision { size_bps (0..2000), direction, stop_loss_pct (0.1..20),
take_profit_pct (0.1..50), … }`.

---

## 2. Goals / non-goals

**Goals**
- The eval trader can **optionally** emit `size_bps`, `stop_loss_pct`,
  `take_profit_pct` per decision; each falls back to a **config default** when
  omitted.
- Add a deterministic **config** risk floor: apply `stop_loss_atr_multiple`
  (today unapplied) and a new `take_profit_atr_multiple`.
- The backtest gains an **intrabar stop/target exit engine** that closes a
  position when a bar's range crosses the active level — regardless of whether
  the model woke that bar.
- Sizing reaches parity: model `size_bps` overrides the mechanical default when
  present.

**Non-goals**
- No change to the **live** executor in this work (backtest-only per
  `project_backtest_only_no_paper`). Live parity is a follow-up, tracked in the
  Deferred section.
- No change to the production `xvision-trader` path.
- No new chart/UI surface for SL/TP in this spec (a follow-up may render exit
  reasons in the trace dock).

---

## 3. Design decisions

### D1 — Both config and model (operator decision 2026-06-03)
Levels and size resolve by priority: **model value when present → else config
default → else open-ended** (no stop/target; current behavior). This keeps the
config floor optimizer-searchable (`feedback_strategy_config_over_harness`,
`project_dspy_prompt_optimization`) while letting an expressive model override
per decision.

### D2 — Sizing parity
`size_bps` is basis points of current NAV (matching production
`TraderDecision.size_bps`, capped at 2000 = 20% — consistent with
`risk.max_position_pct_nav`). When the model omits it, sizing stays mechanical
(`risk_pct_per_trade`). When present: `target_notional = equity × size_bps/10_000`,
`qty = target_notional / next_bar_open`, then existing broker/precision rules apply.

### D3 — Level resolution
- Stop level at entry:
  - model: `stop_loss_pct` → `entry × (1 − pct/100)` long / `× (1 + pct/100)` short.
  - else config: `entry ∓ atr_at_entry × stop_loss_atr_multiple`.
  - else: none.
- Target level at entry: symmetric, using `take_profit_pct` / `take_profit_atr_multiple`.
- Levels are **fixed at entry** (no trailing in v1).

### D4 — Intrabar exit semantics (standard backtest conventions)
- Checked on **every bar** the position is open (not just wake bars), in the
  per-asset per-timestamp visit, after marking.
- **Gap handling:** if the bar **opens** already beyond the level, fill at the
  bar **open** (gap-through); else fill at the **exact level**.
- **Both SL and TP within one bar:** assume **stop hit first** (conservative /
  worst-case). Documented as a known pessimistic bias.
- An exit produced by the engine is a **close** (flat), recorded with an
  exit-reason (`stop_loss` | `take_profit`) so decision provenance stays
  distinguishable (`eval-dev` guardrail: keep synthesized rows distinct from
  model decisions).

### D5 — ATR source
Config-driven levels need an ATR series in the executor, which today only the
filter-hook path computes. Add a per-asset ATR (Wilder, period =
`atr_period`, default 14) computed over each asset's `[warmup… , window…]`
bars at run start, indexed alongside the existing per-asset bar vecs. Model-pct
levels need no ATR. If config asks for an ATR stop but ATR is unavailable
(insufficient warmup), record a `Finding` (warn) and treat as open-ended rather
than fabricating a level.

---

## 4. Schema changes

### 4.1 `TraderOutput` (`eval/executor/trader_output.rs`)
```rust
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct TraderOutput {
    pub(crate) action: String,
    pub(crate) conviction: f64,
    pub(crate) justification: String,
    #[serde(default)] pub(crate) size_bps: Option<u32>,
    #[serde(default)] pub(crate) stop_loss_pct: Option<f64>,
    #[serde(default)] pub(crate) take_profit_pct: Option<f64>,
}
```
`#[serde(default)]` keeps every existing 3-field output valid under
`deny_unknown_fields`. Validate ranges when present (`size_bps ≤ 2000`,
`0.1 ≤ stop_loss_pct ≤ 20`, `0.1 ≤ take_profit_pct ≤ 50`); out-of-range →
`TraderFailureKind::InvalidField` (existing machinery).

### 4.2 Agent templates (`agents/templates.rs`)
Extend the advertised schema string in the trader starter prompts to:
```
{"action":"long_open|short_open|flat|hold","conviction":0..1,
 "justification":"string","size_bps":0..2000 (optional),
 "stop_loss_pct":0.1..20 (optional),"take_profit_pct":0.1..50 (optional)}
```
Wording: "size_bps/stop_loss_pct/take_profit_pct are optional; omit to use the
strategy's configured risk defaults." Update `agents/validator.rs` /
`agents/validate.rs` if they assert the schema enum/fields.

### 4.3 `RiskConfig` (`strategies/risk.rs`)
Add `#[serde(default = "default_take_profit_atr_multiple")] pub
take_profit_atr_multiple: f64` (default e.g. 3.0) + set it in the three presets
(Conservative/Balanced/Aggressive). `#[serde(default)]` so persisted
filesystem strategies hydrate without migration (strategies are JSON, not DB —
`project_strategy_color_field`). Add `atr_period` (default 14) if not already
present on the strategy.

---

## 5. Executor changes (`eval/executor/backtest.rs`)

1. **ATR series:** after per-asset bars are resolved (post-T1 loader), compute
   `atr_by_asset: BTreeMap<AssetSymbol, Vec<f64>>` aligned to each asset's bar
   vec (Wilder, `atr_period`, seeded from warmup).
2. **Open path (`~1452`):** on a fresh open (`pre_fill_position == 0`),
   resolve size (D2) and capture the entry-time stop/target levels (D3) into the
   position's leg metadata. (Requires extending `PortfolioBook::Leg` or a side
   map `levels_by_asset` to carry `stop`/`target`/`reason`.)
3. **Per-bar exit check (in the per-asset per-timestamp visit, near `book.mark`):**
   if a leg is open and has a stop/target, test the current bar's `[low, high]`
   (D4); on a hit, synthesize a close fill at the resolved price, realize PnL via
   the existing fill/`set_position(_, 0.0, _)` path, clear the levels, and emit a
   decision/marker row tagged with the exit reason.
4. **Sizing:** thread `size_bps` into the `estimated_qty` computation at
   `~1457`.

Carry-overs: the synthesized exit must run through the same fill/fee/PnL code as
a model `flat` so accounting stays consistent (and so T4's round-trip counting
sees a real close).

---

## 6. Backward compatibility

- Existing strategies (3-field trader output, no `take_profit_atr_multiple`)
  behave **identically**: optional fields default to `None`; with no model
  levels and the historical default `stop_loss_atr_multiple` now *applied*, note
  that applying a previously-inert config value **changes results** for
  strategies that set it. → Gate: apply config ATR stop only when
  `stop_loss_atr_multiple > 0`, and call this out in the PR so existing
  baselines are re-pinned intentionally.
- `deny_unknown_fields` preserved.
- Live executor untouched.

---

## 7. Test plan (DoD)

1. `trader_output` parses a 6-field output and a legacy 3-field output; rejects
   out-of-range `size_bps`/`stop_loss_pct`.
2. Backtest: a long position **exits at the model `stop_loss_pct` level** within
   a bar whose low crosses it (assert fill price ≈ level, exit reason
   `stop_loss`).
3. Backtest: a long position **exits at the model `take_profit_pct` level**.
4. Backtest: with the model omitting brackets and `stop_loss_atr_multiple > 0`,
   the position exits at the **config ATR stop**.
5. Backtest: `size_bps` override changes the opened qty vs the mechanical default.
6. Gap-through: a bar opening past the stop fills at the **open**, not the level.
7. Regression: a 3-field strategy with `stop_loss_atr_multiple == 0` produces
   byte-identical decisions to pre-change (no phantom exits).

---

## 8. Deferred / follow-ups
- Live executor parity for sizing + SL/TP (separate track; backtest-only now).
- Trailing stops (v1 is fixed-at-entry).
- Trace-dock / chart rendering of exit reasons.
- "Both SL & TP in one bar" pessimistic bias — revisit with intrabar tick data
  if/when available.

---

## 9. File touch-list
| File | Change |
|---|---|
| `eval/executor/trader_output.rs` | +3 optional fields, range validation |
| `agents/templates.rs` | advertise optional fields in trader prompts |
| `agents/validator.rs`, `agents/validate.rs` | allow new optional fields |
| `strategies/risk.rs` | `take_profit_atr_multiple` (+`atr_period`) + presets |
| `eval/executor/backtest.rs` | ATR series, sizing override, intrabar exit engine, leg-level metadata |
| `eval/executor/book.rs` | leg stop/target metadata (or side map) |
| `crates/xvision-engine/tests/…` | DoD tests 1–7 |

## 10. Relationship to sibling tasks
- **T1 (done):** per-asset bar-cache key — landed on this branch with test.
- **T3:** enforce `max_concurrent_positions` — small, independent; `PortfolioBook::open_position_count()` already added.
- **T4:** count closed round-trips for `n_trades`/`win_rate` — benefits from this work, since engine-synthesized exits create real closes to count.
