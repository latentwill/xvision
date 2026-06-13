# Pine Script Ingestion — Gap Analysis, Inheritance Map, Reuse Survey & Execution Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> (recommended) or superpowers:executing-plans to implement this plan work-unit-by-work-unit.
> Each work unit (WU) below is a contract: exact files, the failing test to write first, the
> verification that proves it, and its dependencies. Expand each WU via the mandatory TDD
> workflow (`superpowers:test-driven-development`); the coverage gate
> (`.coverage-thresholds.json`) is BLOCKING before PR creation. All branch work in
> `.worktrees/<name>` with `CARGO_TARGET_DIR` set; build via `scripts/cargo`.

**Date:** 2026-06-12 · **Method:** direct read of the live filter/mechanistic DSL surface
(`xvision-filters`, `strategies/mechanistic.rs`) + a live web survey of open-source Pine
tooling, all repo facts fetched and verified the same day, + a design/ecosystem inventory of
Pine Script & the TradingView Strategy Tester (Part 2). No code modified.

**Plan review gate:** Iteration 1 (2026-06-13) — **FAILED 3/3** (Feasibility, Completeness,
Scope). Findings drove the rev-2 rewrite below (see *Revision log*). Operator scope decision
(2026-06-13): implement **all five** inherited ideas, but **no Python sidecar and no external
`yata` crate** — everything native and in-repo. Re-gate before execution.

**Goal:** Let a user upload a TradingView Pine Script and get a usable xvision `Strategy` —
not a byte-exact replica of the source, but an **approximate, honestly-labelled starting
point that the autooptimizer can immediately evolve.** Turn a competitor's format into a
cold-start funnel and a feeder for the one capability xvision has that TradingView /
trader.dev do not: self-optimizing strategies.

**Architecture (rev 2 — native, in-repo):** A hand-written **native Rust** recursive-descent
parser for a pragmatic **Pine v5 subset** (the importable archetype — `input.*`, `ta.*`,
variable assignments, comparison/boolean/cross expressions, `strategy.entry/close/exit`).
**No out-of-process sidecar, no Python.** Map the AST onto xvision's native filter +
mechanistic DSL, harvest `input.*` knobs into the optimizer's mutation surface (extending the
mutator to reach `MechanisticConfig` scalars, which it currently cannot), and emit a
**fidelity diff** so dropped semantics are visible instead of silent. Anything outside the
parsed subset is recorded as an `Unsupported` node and surfaced in the diff (never panics).
Indicators added for catalog parity are implemented **natively** in `xvision-filters` (the
same hand-rolled style as the existing EMA/RSI/etc.), **not** via an external TA crate.
Deliberately **do not** build a lossless Pine execution engine.

**Tech stack:** existing Rust workspace only (engine + `xvision-filters` IndicatorEngine +
`autooptimizer` mutator + dashboard routes + Vite SPA). Zero new runtime dependencies, zero
new languages. The Pine corpus ships as committed test fixtures.

---

## Part 1 — Gap Analysis

Convention: **(F)** = verified by direct file/doc read (path cited), **(J)** = judgment.

### 1.1 What the xvision DSL can express today

- **Filter DSL** (`crates/xvision-filters/src/`): `Operand = Indicator | Numeric | Range(lo,hi)`,
  combined in an `all`/`any` `ConditionTree` of `{lhs, op, rhs}`. Operands are **atomic** —
  no nested arithmetic. (F: `xvision-filters/src/validate.rs`, `types.rs`)
- **Operators** (F: `docs/operator/filter-dsl-catalog.md`): `> < >= <= ==`,
  `crosses_above/below`, `between`, plus *parameterized temporal* ops `above_for_N`,
  `crossed_above_N`, `slope_gt/lt_N`, `zscore_gt/lt_N`, `within_pct`.
- **Indicator catalog**: ~60 fixed tokens (OHLCV, SMA/EMA/WMA, ADX/DI, Donchian,
  highest/lowest, opening-range, Ichimoku, RSI/ROC/Stoch/StochRSI/CCI/MFI/WilliamsR, ATR/ATR%,
  Bollinger, Keltner, MACD, VWAP/OBV/RVOL/volume-z, session & prev-period levels, gaps),
  each period-parameterized. (F: `docs/operator/filter-dsl-catalog.md`)
- **One `timeframe`, one symbol** per filter (v1). (F: catalog "asset_scope … exactly one
  symbol in v1")
- **Mechanistic mode** (deterministic, no LLM — `crates/xvision-engine/src/strategies/mechanistic.rs`):
  `entry_rules: Vec<{signal_name, Long|Short}>`; `close_policies: Vec<{StopLoss% |
  TakeProfit% | TrailingStop% | TimeExit(bars) | TargetPnl($)}>`. (F)
- **Optimizer mutation surface** already addresses operand values by path, e.g.
  `conditions.<i>.rhs.numeric`, `conditions.<i>.rhs.range.lo/hi`, `cooldown_bars`. (F:
  `crates/xvision-engine/src/autooptimizer/mutator.rs:380–400`)

### 1.2 Where Pine→xvision conversion leaks

Ranked by how often it bites a real published script (J, informed by the DSL surface above):

| Pine capability | xvision today | Verdict |
|---|---|---|
| Arithmetic in conditions (`close*1.02`, `(high-low)/close`) | atomic operands only | ❌ **#1 loss** — approximate (`within_pct`) or drop |
| Custom / out-of-catalog indicators (SuperTrend, HMA, pivots, divergence, user-coded) | fixed ~60-token catalog | ❌ dropped or crudely faked |
| Stateful / since-entry logic (`var`, `ta.barssince`, `ta.valuewhen`, "highest-high-since-entry") | only canned `_N` ops | ⚠️ partial |
| Order management (pyramiding, scale in/out, partial exits, limit/stop entries, per-leg brackets, %-equity / risk sizing) | one entry per signal, one flat close-policy set, market-at-bar-close | ❌ collapsed |
| Multi-timeframe / multi-symbol (`request.security`) | one TF, one symbol | ❌ dropped |
| Exit on arbitrary condition ("close when MACD<0 and RSI>70") | policy-based or one wired signal | ⚠️ clunky |
| Backtest economics (commission, slippage, intrabar fills) | xvision eval's own (simpler) model | ❌ **trust bomb** — P&L won't reconcile with TradingView |

**Clean-conversion archetype** (J): "EMA/MA cross + RSI/ADX filter + ATR/%% stop" maps almost
1:1 → ~40–60% of *simple* public scripts. The remainder degrades, and today nothing tells the
user it degraded.

**The load-bearing risk** (J): silent backtest-P&L divergence from TradingView reads as
"the converter is broken," which poisons trust in the whole product. The fidelity diff (WU4)
is therefore not optional polish — it is the deliverable that keeps trust alive.

---

## Part 2 — What to inherit from Pine Script (beyond the importer)

The importer (Part 4) is about *cribbing code to parse Pine*. This section is the wider
inheritance: design and ecosystem decisions Pine/TradingView got right over a decade that
xvision should adopt **regardless of whether the importer ships**. Legend: **✓** already
captured in this plan or elsewhere in the repo · **◆** new inheritable candidate (not yet
tracked) · **⚠** anti-pattern to consciously *not* inherit.

### 2.1 Authoring & declarative-config wisdom

| # | Pine pattern | Status | xvision mapping |
|---|---|---|---|
| 1 | `input.*` declarations carry rich metadata (title, tooltip, group, min/max/step, inline) | ◆ | Beyond feeding the optimizer (WU3), the same metadata can **auto-generate the strategy settings UI** — Pine renders its whole settings panel from `input()` calls. One declarative param spec → optimizer search space *and* config form. |
| 2 | `//@version=N` pragma + strict language versioning + upgrade tooling | ◆ | Stamp every `Strategy`/`Filter` with a schema version and a documented migration path; today back-compat is implicit via serde defaults (`strategies/mod.rs`). Make it explicit. |
| 3 | Explicit series/history (`[n]`) discipline + `barstate.*` (isconfirmed / isrealtime / ishistory / islast) | ◆ | The confirmed-vs-unconfirmed-bar distinction *is* the repaint-safety model. xvision's eval already does T+1 fills (`backtest.rs`); surfacing an explicit barstate concept hardens against look-ahead bugs in new operators. |
| 4 | `strategy()` vs `indicator()` split — tradeable vs pure signal | ✓ | Mirrors xvision's `filter` (signal) vs `Strategy` (tradeable). Already aligned; keep the boundary clean. |
| 5 | Built-ins as a *curated, documented, stable* vocabulary | ✓ partial | `docs/operator/filter-dsl-catalog.md` is the seed; §2.4 item 17 extends it to a full manual. |

### 2.2 Backtest-tester conventions (the trust vocabulary)

TradingView's Strategy Tester is the format millions of traders already trust. Matching its
*vocabulary and transparency* is how an xvision backtest earns belief — directly reinforcing
the WU4 fidelity diff and the trust theme in `2026-06-12-profit-path-audit-and-plan.md`.

| # | Pine/TV pattern | Status | xvision mapping |
|---|---|---|---|
| 6 | Canonical performance report: net profit, **profit factor**, max drawdown, Sharpe, **Sortino**, win rate, avg trade, # trades, % profitable | ◆ | Adopt the same metric names/layout users already read. Lowers the cognitive cost of trusting xvision's number. |
| 7 | Cost assumptions surfaced explicitly (commission, slippage, initial capital, order size) | ◆/✓ | The eval already models fills/slippage/fees richly (`eval/executor/traits.rs`); the inheritance is **showing** those assumptions in the report, not hiding them. |
| 8 | "List of Trades" drill-down: headline number → every individual trade | ◆ partial | Pair the equity pane with a per-trade table so users can audit *why* the number is what it is. |
| 9 | Strategy Properties: initial capital, base currency, order size (% equity / fixed / contracts), pyramiding, margin, recalc-on-every-tick | ◆ | A ready-made spec for xvision's sizing/risk config surface (`RiskConfig`); also the schema the importer's order-management gap (Part 1.2) would target if Path B is ever pursued. |
| 10 | Bar-replay / deep-backtest step-through | ◆ | Step bar-by-bar through a run to watch decisions form — strong debugging/explainability UX for agentic strategies especially. |

### 2.3 Ecosystem & distribution patterns

This is where Pine's real moat lives, and where xvision's existing **marketplace** is the
natural home (`docs/superpowers/plans/2026-06-11-marketplace-*`,
`2026-06-12-marketplace-ui-overhaul.md`). These are the patterns that convert a tool into a
community.

| # | Pine/TV pattern | Status | xvision mapping |
|---|---|---|---|
| 11 | Visibility tiers: open-source / **protected** (usable but closed) / invite-only | ◆ | A protected tier lets authors monetize a strategy without revealing the pipeline — a clean fit for the NFT/mint marketplace. |
| 12 | Fork / "make a copy" of a published script → tweak | ◆ | Fork a marketplace strategy → **mutate via the autooptimizer** → re-list. Forking + evolution is a loop neither Pine nor trader.dev has. |
| 13 | Reputation & social proof: likes, boosts, comments, Editor's Picks, author following | ◆ | Discovery/ranking signals for the marketplace beyond raw backtest score. |
| 14 | Published-script corpus as a cold-start seed | ✓ | Already the strategic spine of this plan (importer → seed library → optimizer fuel). |
| 15 | Alerts: `alertcondition` + templated dynamic messages w/ placeholders → webhook delivery | ◆ | xvision has `fire` metadata + a live path; inherit the **templated-message + webhook** bridge so signals can drive external execution/notification. |
| 16 | Tight in-browser authoring loop: instant compile, line-numbered errors | ◆/✓ | The chat-rail + `xvision-filters::validate` error codes already approximate this; keep the edit→validate latency low. |

### 2.4 Documentation & the anti-patterns to *not* inherit

| # | Pattern | Status | Note |
|---|---|---|---|
| 17 | Docs culture: Reference Manual + User Manual + community FAQ, example-rich | ◆ | Grow `filter-dsl-catalog.md` into a full operator manual; it's the difference between a DSL people *can* use and one they *do*. |
| 18 | **Repainting / backtest≠live divergence** — Pine's single biggest trust wound | ⚠ | Inherit the *vigilance*, never the footgun. This is exactly what `2026-06-12-profit-path-audit-and-plan.md` targets (backtest↔live parity). The importer's fidelity diff (WU4) is the same instinct applied to conversion. |
| 19 | Single-file scripts + hard compute caps | ⚠ | xvision has no such limit — don't recreate one. Multi-agent pipelines are a feature, not a constraint to cap. |
| 20 | Vendor lock-in to a proprietary closed runtime | ⚠ | xvision is self-hostable (CLI + own engine). Keep the DSL open and documented; that openness is a differentiator, not an oversight. |

### 2.5 The three highest-leverage inheritances (J)

If only three things are taken from this section: **(1)** `input.*`-as-one-spec driving both
optimizer search space *and* auto-generated settings UI (item 1+WU3); **(2)** the TradingView
Strategy-Tester metric/transparency vocabulary so xvision backtests inherit existing trust
(items 6–7); **(3)** fork→optimize→re-list in the marketplace (item 12) — the one ecosystem
loop that turns Pine's distribution pattern into something neither Pine nor trader.dev can do.

---

## Part 3 — Reuse Survey (open-source, verified 2026-06-12)

| Repo | Role | License | State | Use for |
|---|---|---|---|---|
| **pynescript** (elbakramer) | Pine→AST parser (ANTLR), unparse; **no** execution/indicators | LGPL-3.0 | 92★, v0.3.0 Dec 2025, v5 | ✅ **Path A parser** — the reusable 60–80% |
| **yata** (amv-dev) | Rust TA lib, 30+ indicators, streaming `.next()`, custom-indicator trait | Apache-2.0 | 395★, v0.7.0 Mar 2024 | ✅ **Path C** catalog expansion |
| **PineTS** (QuantForgeOrg) | Full transpiler **+ runtime**, 60+ indicators, v5/exp-v6 | ⚠️ **AGPL-3.0 + Commercial** | 403★, v0.9.22 Jun 2026 | ⛔ Reserved (Path B) — AGPL = open your server or buy commercial license |
| **PyneCore** (PyneSys) | Python runtime, claims `strategy.*` + `request.security()` parity | Apache-2.0 (runtime) | 154★, v6.5.0 Jun 2026 | ⛔ Reserved (Path B) — license-clean, but the Pine→Python converter is paywalled (PyneSys) |

**Finding:** there is **no Rust-native Pine parser or runtime.** Any Pine handling is inherently
out-of-process for this codebase — which favors a one-shot parse-at-import (Path A) over an
embedded runtime (Path B).

**Licensing landmine (J — confirm with counsel):** PineTS is AGPL-3.0; AGPL's network clause
would require releasing xvision's server source if PineTS is embedded in a user-facing
service. For a commercial product with a marketplace/mint this is effectively a paid
dependency. pynescript's LGPL is satisfied trivially by running it as a separate process.

---

## Part 4 — Execution Plan (rev 2 — native, all five ideas)

**Chosen path: native A (parse → map → optimize) + native C (indicator parity), in-repo.**
Path B (lossless runtime) remains out of scope. Per operator decision (2026-06-13) there is
**no Python sidecar and no external TA crate** — the parser is hand-written Rust over a Pine v5
subset and the new indicators are hand-rolled in `xvision-filters`. Import is a **one-shot
operation at upload time** (`xvn strategy import-pine <file>` and `POST /api/strategy/import/pine`),
not a per-bar runtime.

**WU dependency graph:**
```
WU1 (native parser) ──► WU2 (mapper) ──► WU3 (inputs→optimizer, needs WU3a mutator ext)
                                    ├──► WU4 (fidelity diff) ──► WU10 (cost vocabulary)
WU3a (mutator: mechanistic paths) ─┘
WU5 (native indicators) ── independent (sequenced by WU1 corpus gaps)
WU2/WU4 ──► WU6 (CLI verb) ──┐
WU2/WU4 ──► WU7 (HTTP route) ─┴─► WU8 (frontend import UI) ──► WU9 (cold-start library funnel)
```

### WU1 — Native Pine v5 (subset) parser → typed AST
- **Files:** new module `crates/xvision-engine/src/strategies/pine_import/{mod.rs,lexer.rs,parser.rs,ast.rs}`;
  **hand-authored** corpus committed at `crates/xvision-engine/tests/fixtures/pine/*.pine`; AST doc under `docs/operator/`.
- **Contract:** hand-written lexer + recursive-descent parser for the importable subset:
  `//@version=5`, `indicator(...)`/`strategy(...)` headers, `input.int/float/bool/string(...)`,
  `var`/plain assignments, `ta.*(...)` calls, arithmetic/comparison/boolean/ternary expressions,
  `ta.crossover/crossunder`, and `strategy.entry/close/exit`. Anything outside the subset becomes
  a typed `AstNode::Unsupported{ source_span, raw }` — **never panics**. No subprocess, no Python.
  `mod.rs` exposes the single public entry the rest of the system calls:
  `pub fn import_pine(src: &str) -> Result<ImportOutcome, PineImportError>` where
  `ImportOutcome { strategy: Strategy, fidelity: FidelityReport }` (assembled across WU2+WU4).
- **Corpus sourcing (licensing-clean):** the ≥10 v5 fixtures are **written by us** — minimal,
  purpose-built scripts that each exercise specific supported syntax nodes (one per archetype:
  RSI-threshold, MA-cross + stop/target, BB-mean-revert, SuperTrend-follow, a multi-input knob
  script, a deliberately-fuzzy script for the agentic-fallback path, a malformed script, an
  unsupported-construct script, …). **Not** copied from TradingView's published library (those
  remain author-copyright). Each fixture carries a one-line header comment stating it is an
  original test fixture.
- **Test first:** the hand-authored corpus (≥10 scripts) parses to a **stable snapshot AST**; a
  malformed script returns a structured `PineParseError` (line/col), not a panic; an unknown
  construct yields `Unsupported`, not error.
- **Verify:** `scripts/cargo test -p xvision-engine pine_import::parse`; snapshot stable
  (plain `assert_eq!` on serialized AST JSON; no new snapshot-crate dependency required).
- **Deps:** none.

### WU2 — AST → Strategy mapper (+ idea 4: indicators-as-agent-context)
- **Files:** `crates/xvision-engine/src/strategies/pine_import/map.rs`; reuse `xvision_filters::validate`
  (`crates/xvision-filters/src/validate.rs`, re-exported `xvision_filters::validate`) and
  `validate_strategy` (`crates/xvision-engine/src/strategies/validate.rs`) as acceptance gates;
  reuse `MechanisticConfig`/`EntryRule`/`ClosePolicy` (`strategies/mechanistic.rs`).
- **Contract:** deterministically map the clean archetype: `ta.*` exprs → `IndicatorName` operands;
  comparison/cross exprs → `Filter` `Condition`s; `strategy.entry` → `EntryRule`;
  `strategy.exit(stop=/limit=/trail=)` → `ClosePolicy::{StopLoss,TakeProfit,TrailingStop}`.
  Every emitted Strategy must pass `validate` + `validate_strategy`. **Idea 4 (concrete payload &
  storage):** when an entry/exit predicate references indicators we *can compute* but *cannot*
  reduce to a mechanistic `Condition` (fuzzy node), the strategy is emitted in
  `DecisionMode::Agentic` with a **new typed, serialized field on the imported strategy** —
  `briefing_indicators: Vec<BriefingIndicator { name: IndicatorName, params: Vec<f64>, source_token: String }>`
  (defined in `strategies/mechanistic.rs` or a sibling, `#[serde(default, skip_serializing_if = "Vec::is_empty")]`
  so existing strategies are unaffected). This is a **static, per-strategy import-time annotation**
  — explicitly *not* the per-bar runtime `filter_context` slot. A small WU2 sub-task wires the
  decision-seed builder to compute these via the existing `xvision-filters` `IndicatorEngine` each
  cycle and include them in the seed's `market_data` indicator snapshot, so the agent sees them as
  briefing context at run time. (No live LLM call at import time.) Unmapped nodes are recorded
  (feeds WU4), never silently dropped.
- **Test first:** 3 fixtures → expected `Filter`+`MechanisticConfig` JSON; a `var` counter / fuzzy
  predicate produces an Agentic strategy whose `briefing_indicators` field contains the referenced
  indicator + an "unmapped" record (not a wrong filter); a decision-seed built from that strategy
  includes the indicator value in `market_data`; an entirely-unmappable script yields a recorded
  `PineImportError`, not an empty/invalid Strategy.
- **Verify:** mapper output passes `validate_strategy`; round-trips through the strategy store
  (including the new `briefing_indicators` field).
- **Deps:** WU1.

### WU3a — Mutator: `MechanisticConfig` tunable paths (keystone fix)
- **Files:** `crates/xvision-engine/src/autooptimizer/mutator.rs` (extend `filter_tunable_paths`
  /`set_filter_value` siblings with a `mechanistic.*` scheme; add `mechanistic_tunable_paths`
  + setter/getter).
- **Contract:** today the mutator only addresses `conditions.*` / `cooldown_bars` /
  `max_wakeups_per_day` (verified `mutator.rs:402+`) — `MechanisticConfig` scalars are
  **unreachable**. Add paths `mechanistic.close_policies.<i>.pct` / `.bars` / `.usd` (per
  `ClosePolicy` variant) so stop/target/trail/time/pnl values become mutation targets. The setter
  is **variant-aware**: a path's leaf (`.pct` / `.bars` / `.usd`) must match the `ClosePolicy`
  variant at that index, else it returns a structured `MutatePathError` (never silently writes the
  wrong field). Existing paths unchanged (backward-compatible).
- **Test first:** a `MechanisticConfig` with `StopLoss{pct}` + `TimeExit{bars}` enumerates exactly
  the two correct-leaf paths (`.pct` for index 0, `.bars` for index 1); set/get round-trips; a
  **cross-variant mismatch** (setting `.pct` on the `TimeExit` index) returns `MutatePathError`,
  does not mutate; a flat `Filter`-only strategy's path set is unchanged.
- **Verify:** `scripts/cargo test -p xvision-engine autooptimizer::mutator`.
- **Deps:** none (independent; gates WU3).

### WU3 — `input.*` → optimizer search space (the differentiator)
- **Files:** `crates/xvision-engine/src/strategies/pine_import/inputs.rs`; emit targets in the
  mutator path format (Filter paths from WU2 **and** mechanistic paths from WU3a).
- **Contract:** parse `input.int/float/bool(defval, minval, maxval, step, title)` → optimizer
  mutation targets bound to the operand path WU2 produced for that knob (e.g. an RSI-length input
  → `conditions.<i>.rhs.numeric`; a stop-% input → `mechanistic.close_policies.<i>.pct`). `input.bool`
  → discrete mutation. Bounds carried from `minval/maxval/step`. Uploaded script lands **already
  parameterized** for evolution.
- **Test first:** a script with 3 inputs (one bound to a Filter numeric, one to a stop-% mechanistic
  path, one bool) yields 3 mutation targets with correct paths + bounds.
- **Verify:** a unit/integration test asserts the imported strategy's enumerated tunable paths
  include the mechanistic stop-% target and that applying a mutation via the mutator API actually
  perturbs it (asserts WU3a wiring, not just Filter paths). NB: `xvn optimize mutate-once` was
  removed in PR #972 — the optimizer's only CLI entry is now `xvn optimize run`; use a direct
  mutator-API test for the per-target assertion and (optionally) `xvn optimize run --max-cycles 1`
  as an end-to-end smoke, not the deleted `mutate-once`.
- **Deps:** WU2, WU3a.

### WU4 — Fidelity diff report (trust preservation)
- **Files:** `crates/xvision-engine/src/strategies/pine_import/fidelity.rs`; rendered in CLI (WU6)
  and inline in the import UI (WU8) — no popups, no right-rail box (per `/CLAUDE.md` SPA rules).
- **Contract:** per import emit `captured / approximated / dropped` with per-item reasons
  ("dropped: pyramiding", "approximated: `close*1.02` → `within_pct_2`", "agentic-fallback: HTF
  confirmation passed as briefing feature"). Serializable `FidelityReport` struct.
- **Test first:** a script with pyramiding + HTF confirmation lists both (dropped / agentic-fallback)
  with reasons; a clean archetype reports zero drops.
- **Verify:** snapshot test on `FidelityReport` JSON.
- **Deps:** WU2.

### WU5 — Native indicator catalog expansion
- **Files:** `crates/xvision-filters/src/types.rs` (new `IndicatorName` variants) +
  `crates/xvision-filters/src/indicators.rs` (native compute, same style as existing EMA/RSI) +
  catalog doc `docs/operator/filter-dsl-catalog.md`.
- **Contract:** add the highest-frequency missing `ta.*` indicators — **SuperTrend, HMA, VWMA,
  pivot points**, plus gaps surfaced by the WU1 corpus — implemented **natively** (no `yata`). Each
  new token parses, validates, and computes. Additive enum variants only; a round-trip test confirms
  pre-existing filter JSON still deserializes (the only relevant direction). **Binary-downgrade
  forward-compat is explicitly out of scope:** pre-launch there are no users and stored strategies
  are not version-pinned across downgrades — the operator stance is wipe-and-redeploy on schema
  change, so an old binary reading a new `super_trend` token is not a supported scenario and needs
  no migration/version-gate.
- **Test first:** new tokens parse + validate; computed values match a hand-checked reference
  series within tolerance; deserializing a pre-existing filter JSON still succeeds.
- **Verify:** `scripts/cargo test -p xvision-filters`. **Conversion metric:** a
  `pine_corpus_conversion` test computes `captured_nodes / total_nodes` across the WU1 corpus and
  asserts the ratio **after** WU5 ≥ a recorded **baseline** captured before WU5 (numbers logged in
  the test output — no silent "rises").
- **Deps:** independent; candidate set finalized from the WU1 corpus.

### WU6 — CLI verb `xvn strategy import-pine <file>`
- **Files:** `crates/xvision-cli/src/commands/strategy.rs` — add `StrategyAction::ImportPine{ file, name }`,
  wire into the existing `match` dispatcher; the handler reads the file, calls
  `pine_import::import_pine(&src)` (the WU1 entry), persists `outcome.strategy`, prints
  `outcome.fidelity`. No conversion logic in the CLI layer.
- **Test first:** CLI integration test imports a fixture `.pine`, asserts a valid Strategy is saved
  and the fidelity report is printed; a malformed file exits non-zero with the structured error.
- **Verify:** `scripts/cargo test -p xvision-cli`; manual `xvn strategy import-pine <fixture>`.
- **Deps:** WU2, WU4.

### WU7 — HTTP route `POST /api/strategy/import/pine`
- **Files:** `crates/xvision-dashboard/src/routes/strategies.rs` (handler) + register in
  `crates/xvision-dashboard/src/server.rs`.
- **Contract:** accept Pine text (body/multipart) → call `pine_import::import_pine(&src)` → return
  `{ strategy, fidelity_report }` JSON; malformed → 400 with the structured `PineImportError`. The
  same single engine entry as WU6 (no conversion logic in the route).
- **Test first:** route test posts a fixture → 200 with strategy + report; malformed → 400.
- **Verify:** `scripts/cargo test -p xvision-dashboard`.
- **Deps:** WU2, WU4.

### WU8 — Frontend import UI
- **Files:** `frontend/web/src/features/strategies/` (new import view/route + component) + nav entry;
  reuse existing list/card primitives.
- **Contract:** upload/paste a Pine script → call WU7 → render the fidelity diff **inline**
  (full-width, no popup/modal/right-rail — per `/CLAUDE.md`), with captured/approximated/dropped
  sections, a link to the created strategy, and an **"Optimize this"** CTA. Dark-mode-safe borders.
- **Test first:** component/route test (vitest/RTL): a mocked import response renders all three
  fidelity sections and the optimize CTA; error response renders inline error (no overlay).
- **Verify:** `npm test` in `frontend/web`; visual check via the run/verify skill.
- **Deps:** WU7.

### WU9 — Idea 2: cold-start Pine library funnel
- **Files:** seed-library index in `crates/xvision-engine/src/strategies/pine_import/library.rs`
  (catalogs the committed corpus + metadata), route `GET /api/strategy/pine-library` +
  `POST .../import/<id>`, frontend list under `features/strategies/`.
- **Contract:** surface the committed Pine corpus (and any operator-added scripts) as a **browsable
  seed library** — each entry shows name/source/what-it-does and a one-click **Import → Optimize**
  action that runs WU2 + lands an already-parameterized strategy. This is the blank-page /
  onboarding funnel (idea 2), built on the same import path (no new conversion logic).
- **Test first:** library lists ≥10 corpus entries; one-click import of an entry produces a valid,
  parameterized strategy (asserts WU2+WU3 reuse); empty-library state renders.
- **Verify:** route + frontend tests; manual click-through.
- **Deps:** WU2, WU6/WU7, WU8.

### WU10 — Idea 5: broker cost-model vocabulary as a fidelity reference
- **Files:** extend `pine_import/fidelity.rs` + the eval cost-model surface
  (`crates/xvision-engine/src/eval/` cost arrays / broker rules) read-only; report fields only.
- **Contract:** the fidelity report surfaces **xvision's own backtest cost assumptions**
  (commission, slippage, fill timing) using TradingView-Strategy-Tester-aligned vocabulary, so a
  user can reconcile expected divergence from the source's TradingView numbers. **Not** byte-exact
  reconciliation (still a non-goal) — a labelled reference, per idea 5 / §2.2 items 6–9.
- **Test first:** report includes a `cost_model` block naming the active commission/slippage model
  with values; vocabulary matches the catalog doc.
- **Verify:** snapshot test on the extended report.
- **Deps:** WU4.

---

## Non-goals (v1)

- Lossless Pine execution / a Pine *runtime* (Path B). The native parser targets a **subset**; the
  fidelity diff makes the gap honest.
- Byte-exact backtest reconciliation with TradingView (WU10 surfaces assumptions, does not match P&L).
- Multi-symbol / cross-asset conditions and `request.security` multi-timeframe.
- Pine plotting / alert / visual semantics.
- No Python sidecar; no external TA crate (`yata`); no new runtime dependency.

## Risks & open questions

- **Native-parser subset coverage vs real v5** — WU1's committed corpus + the `captured/total`
  metric (WU5) is the honest signal. Low coverage degrades to more `Unsupported` nodes + agentic
  fallback (WU2), never a panic or a wrong filter.
- **Corpus sourcing/licensing** — resolved: the corpus is **hand-authored** original test fixtures
  (WU1), not copied from TradingView's (author-copyright) published library. No third-party script
  text enters the repo.
- **Mapper correctness** (WU2) — the deterministic `validate`/`validate_strategy` gate is the
  guardrail; never persist an unvalidated mapping. Agentic fallback (idea 4) is the safety net for
  fuzzy nodes, not a silent drop.
- **Mutator back-compat** (WU3a) — new `mechanistic.*` paths must not alter existing
  `conditions.*` enumeration; covered by a regression test.

## Revision log

- **rev 2 (2026-06-13)** — responds to plan-review-gate iteration 1 (FAILED 3/3):
  - *Feasibility #1 (mutator can't reach `MechanisticConfig`)* → new **WU3a** adds `mechanistic.*`
    tunable paths; WU3 verify now asserts a stop-% input is actually perturbed.
  - *Feasibility #2 / Completeness #5 (Python not in deploy image)* → **eliminated**: native Rust
    parser, no sidecar (operator decision).
  - *Feasibility #3 (no corpus)* → WU1 commits ≥10 fixtures under `tests/fixtures/pine/`.
  - *Completeness #1/#2/#3 (no CLI/route/UI WUs)* → added **WU6/WU7/WU8**.
  - *Completeness #4 (`yata` dep unowned)* → **eliminated**: native indicators in WU5 (operator decision).
  - *Completeness WU5 metric undefined* → WU5 now defines the `captured/total` corpus metric with a
    recorded baseline.
  - *Scope #1 (idea 2 unscoped)* → **WU9** (cold-start library funnel).
  - *Scope #2 (idea 5 unscoped)* → **WU10** (cost-model vocabulary, not reconciliation).
  - *Scope #3 (idea 4 under-specified)* → WU2 now defines the agent-context payload + Agentic-mode fallback.
- **rev 3 (2026-06-13)** — responds to plan-review-gate iteration 2 (Feasibility PASS, Scope PASS,
  Completeness FAIL):
  - *Completeness #1 (WU3a setter cross-variant mismatch untested)* → WU3a setter is now
    variant-aware (`MutatePathError` on leaf/variant mismatch) with an explicit mismatch test.
  - *Completeness #2 (idea-4 payload had no home; `filter_context` is a per-bar runtime slot)* →
    WU2 now defines a static serialized `briefing_indicators: Vec<BriefingIndicator>` field on the
    imported strategy + a seed-builder sub-task that computes them via `IndicatorEngine`. The
    runtime `filter_context` channel is explicitly **not** reused.
  - *Completeness #3 (WU6/WU7 didn't name the engine entry)* → both now call the single
    `pine_import::import_pine(&src) -> ImportOutcome` entry defined in WU1's `mod.rs`.
  - *Completeness #4 (corpus absent + TradingView copyright)* → WU1 corpus is now **hand-authored**
    original fixtures; no third-party script text enters the repo.
  - *Completeness #5 (serde forward/downgrade compat)* → **rebutted with evidence**: pre-launch, no
    users, operator stance is wipe-DB-and-redeploy on schema change
    (`xvision-no-users-wipe-db-instead-of-migrations`); binary-downgrade compat is out of scope.
    Additive-only + the existing-JSON round-trip test stand.
  - *Carried (non-blocking)*: Part 3's reuse table (`pynescript`/`yata`) is **superseded** by the
    rev-2 native decision and retained only as survey history.

## Why this is the right shape (J)

xvision's edge is not running Pine perfectly — TradingView already does that for free. The edge is
**parameterizing** an uploaded script and **evolving** it. WU3 (+WU3a) is the keystone: it routes a
competitor's format into the optimizer's reach. WU4 keeps trust intact while conversion is admittedly
lossy; WU9 turns the corpus into an onboarding funnel; WU10 makes the cost gap honest. Going native
(no sidecar/no external crate) trades a bigger WU1 for a self-contained, shippable artifact with no
deploy/packaging tax — which is why two of the gate's blockers simply disappear.
