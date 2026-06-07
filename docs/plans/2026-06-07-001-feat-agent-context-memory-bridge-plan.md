---
title: "feat: Run-local episodic memory and enriched decision seed context"
status: active
created_at: 2026-06-07
type: feat
adr: docs/adr/ADR-0001-context-management-trading-agents.md
---

# feat: Run-local episodic memory and enriched decision seed context

## Summary

Adds a run-local episodic memory store that accumulates structured decision observations during an eval run and surfaces relevant past decisions — by semantic similarity, not recency — into each new decision seed. Simultaneously enriches the `portfolio_state` seed field with position data the trader can currently not see (entry price, unrealized PnL %, bars held, SLTP levels). Documents the existing fresh-per-decision architecture and the two-tier memory model in an ADR.

---

## Problem Frame

A post-mortem on eval run `01KTGEHNHMGZ4N0Q1ME4EXSJ6Y` (strategy `lfm_rsi_macd_15m_scalper`, model `lfm2.5:8b` via Ollama) revealed two independent gaps:

**Gap 1 — Incomplete position context.** The trader receives `position_size`, `equity`, and `mark_price` but not `entry_price`, unrealized PnL, bars held, or active SLTP levels. After the stop-loss at decision ix=109 the model had no way to reason about its own trade history from the seed alone, contributing to increasingly muddled justifications in decisions 110–114.

**Gap 2 — No within-run memory.** The existing cortex-mem architecture (Observation write → autooptimizer distillation → Pattern recall) is designed for cross-run, cross-strategy learning. It deliberately blocks same-run Pattern recall via a temporal leakage filter. This is correct for training integrity — but it means an agent has no mechanism to recognise "I tried this setup two hours ago and got stopped out" during an active run.

A third finding from the post-mortem corrects a misconception: **context is already fresh per decision**. Each decision call starts with a clean `messages` vec; no cross-decision transcript accumulates. The context growth observed (~14.5k input tokens/decision) reflects the fixed cost of the bar-history window plus within-decision tool-use turns, not cross-decision accumulation. The model degradation was behavioral, not architectural.

---

## Requirements

- R1. An `EpisodicObservation` struct captures the structured outcome of each decision cycle: bar timestamp, indicators at decision time, action, conviction, entry price (if entering), exit reason (if exiting), and a one-line rationale extracted from the trader output.
- R2. An `EpisodicStore` accumulates `EpisodicObservation` entries in-memory for the lifetime of an eval run. It is scoped to one run and is not persisted to SQLite.
- R3. Before each LLM dispatch, the `EpisodicStore` is queried using the current bar's indicator features as the query vector. The top-K most similar past observations are returned regardless of how early in the run they occurred.
- R4. Retrieved observations are injected into the decision seed as a `prior_episodes` structured field, not as free-form prompt text.
- R5. `DecisionSeedInput` and `build_decision_seed` expose `entry_price`, `unrealized_pnl_pct`, `bars_held`, `stop_loss_price`, and `take_profit_price` in `portfolio_state`.
- R6. The cortex-mem Observation write is replaced with a structured serialization of `EpisodicObservation`, improving distillation quality for offline Pattern extraction.
- R7. An ADR is written at `docs/superpowers/specs/2026-06-07-context-management-adr.md` documenting the fresh-per-decision architecture, the two-tier memory model, and the rationale for the run-local episodic store.
- R8. The `EpisodicStore` has a configurable `max_observations` cap (default 500) to bound memory use for very long runs.
- R9. When no past observations exist (early in a run), the `prior_episodes` field is omitted from the seed rather than injected as an empty array.

---

## Key Technical Decisions

- **Feature-vector similarity, not neural embeddings, for within-run recall.** Embedding each observation via the cortex-mem `LocalEmbedder` on every decision adds latency and I/O overhead inappropriate for the tight backtest loop. The indicator features available at decision time (RSI, MACD histogram, EMA cross direction, volume zscore, position side) form a meaningful, low-dimensional feature vector. L2 or cosine similarity over normalized features is cheap and directly interpretable. If the cortex-mem embedder is configured and available, the store can optionally delegate to it; the fallback is always the indicator feature vector.

- **Seed injection as structured JSON, not system-prompt text.** The existing cortex-mem Pattern recall injects into the system prompt as `<prior_observations>` XML. For episodic recall the seed is the better home: it keeps the memory signal in the same structured channel the trader already reads (via `upstream_inputs`), it appears consistently regardless of how the system prompt is authored, and it avoids inflating the system prompt cache key. The `prior_episodes` array lives alongside `portfolio_state` and `market_data` in the seed.

- **Write only state-changing decisions.** Writing every `flat` decision produces noise with no signal. The write path fires when `action != flat` — entries, exits, stop-outs, and position modifications. Flat decisions during filter-gate windows are already suppressed by the filter hook and add no episodic value.

- **Run-local store, not shared SQLite.** Cross-run leakage prevention is cortex-mem's job. The episodic store is a pure in-memory `Vec` with no persistence. It is created per run in the executor and dropped at run completion. This eliminates the temporal-filter complexity entirely for within-run recall.

- **Enrich both backtest and live seed paths.** `build_decision_seed` is called at two sites: the backtest loop (line ~1069) and the live execution path (line ~3123). Both receive `DecisionSeedInput`. Both call sites already have `book.entry_price(asset_sym)` and `short_bars_held` available; `sltp_state` is also maintained at both sites. Enrichment targets `DecisionSeedInput` so both paths update together.

- **ADR lives in `docs/superpowers/specs/`** following the existing design-doc convention. It is the durable architectural record; the plan is the implementation contract.

---

## High-Level Technical Design

Two-tier memory architecture showing the run-local episodic store (within-run) alongside the existing cortex-mem cross-run tier:

```mermaid
flowchart TB
  subgraph PerDecision["Per-decision cycle (fresh each time)"]
    Seed["build_decision_seed\n(market_data + portfolio_state\n+ prior_episodes)"]
    Exec["execute_slot\n(LLM dispatch)"]
    Out["TraderOutput\n(action, conviction, justification)"]
    Seed --> Exec --> Out
  end

  subgraph EpisodicTier["Run-local episodic store (in-memory, scoped to run)"]
    EStore["EpisodicStore\n(Vec<EpisodicObservation>)"]
    FVec["IndicatorFeatureVector\n(RSI, MACD, EMA_cross,\nvol_zscore, side)"]
    Query["similarity_query()\n→ top-K observations"]
    EStore --> Query
    FVec --> Query
    Query -->|prior_episodes| Seed
  end

  subgraph WriteBack["After non-flat decision"]
    Obs["EpisodicObservation\n(structured)"]
    Out -->|extract| Obs
    Obs --> EStore
    Obs -->|structured write| CortexMem
  end

  subgraph CortexMem["cortex-mem (cross-run, persisted)"]
    ObsTier["Tier::Observation\n(structured EpisodicObservation JSON)"]
    PatternTier["Tier::Pattern\n(distilled by autooptimizer)"]
    ObsTier -->|offline distillation| PatternTier
    PatternTier -->|cross-run recall\n(future runs)| Exec
  end
```

Enriched `portfolio_state` shape (additions in context):

```
portfolio_state:
  position_size      f64   (existing)
  equity             f64   (existing)
  mark_price         f64   (existing)
  entry_price        f64   (new — 0.0 when flat)
  unrealized_pnl_pct f64   (new — 0.0 when flat)
  bars_held          u32   (new — 0 when flat)
  stop_loss_price    f64   (new — 0.0 when none active)
  take_profit_price  f64   (new — 0.0 when none active)
```

`prior_episodes` seed shape (injected when observations exist):

```
prior_episodes: [
  {
    bar_timestamp:      string
    action:             string
    conviction:         f64
    entry_price:        f64 | null
    exit_reason:        string | null
    rationale_excerpt:  string   (≤120 chars)
    indicators: { rsi_14, macd_hist, ema_cross, volume_zscore }
  },
  ...
]
```

---

## Scope Boundaries

In scope:
- `DecisionSeedInput` / `build_decision_seed` enrichment (backtest + live paths)
- `EpisodicObservation` struct and `EpisodicStore` (new module in engine)
- Episodic recall injection into seed JSON
- Structured cortex-mem Observation write
- ADR document

### Deferred to Follow-Up Work
- Operator-facing `EpisodicStore` configuration surface (max_observations, K for recall, feature weights) — default constants are sufficient for the first wave
- Dashboard visualization of `prior_episodes` in the eval detail decision trace
- Neural-embedding path for the episodic store (beyond the feature-vector baseline)
- Enabling cortex-mem memory mode by default for new strategies — that is an operator-surface change and depends on the full cortex-mem deployment spec (`2026-06-05-optimizer-ui-redesign-design.md`)
- Cross-run episodic summarization (the autooptimizer distillation path already handles this)

---

## Implementation Units

### U1. Architecture Decision Record

**Goal:** Create a durable ADR documenting the fresh-per-decision architecture, the two-tier memory model, and the root cause of the `lfm2.5:8b` degradation.

**Requirements:** R7

**Dependencies:** none

**Files:**
- `docs/adr/ADR-0001-context-management-trading-agents.md` (already written)

**Approach:** The ADR covers: (1) the confirmed fresh-per-decision architecture with evidence from `execute_slot`'s `messages` initialization; (2) the post-mortem on run `01KTGEHNHMGZ4N0Q1ME4EXSJ6Y` — behavioral model degradation, not context accumulation; (3) the two-tier memory design rationale (run-local episodic for within-run, cortex-mem for cross-run); (4) the temporal leakage filter and why within-run recall must bypass it via a separate store; (5) the decision to use indicator feature similarity rather than embeddings within a run.

**Test scenarios:**
- Test expectation: none — document, no behavioral change

**Verification:** ADR is readable standalone without this plan. It does not contradict the existing `v2d-memory-overview.md` or `2026-05-24-cortex-memory-cline-dspy-flywheels.md`.

---

### U2. Enrich DecisionSeedInput and build_decision_seed

**Goal:** Expose `entry_price`, `unrealized_pnl_pct`, `bars_held`, `stop_loss_price`, and `take_profit_price` in the `portfolio_state` seed field for both the backtest and live execution paths.

**Requirements:** R5

**Dependencies:** none — purely additive field additions

**Files:**
- `crates/xvision-engine/src/eval/executor/backtest.rs` (modify `DecisionSeedInput` struct, `build_decision_seed`, and both call sites at lines ~1069 and ~3123)
- `crates/xvision-engine/src/eval/executor/backtest_tests.rs` or inline tests (modify)

**Approach:**
- Add five new fields to `DecisionSeedInput`: `entry_price: f64`, `unrealized_pnl_pct: f64`, `bars_held: u32`, `stop_loss_price: f64`, `take_profit_price: f64`.
- `unrealized_pnl_pct` is derived from the existing `unrealized_pnl_pct` function (line ~3706) using `position_size`, `entry_price`, and `mark_price`.
- `stop_loss_price` and `take_profit_price` come from `effective_sl_price` / `effective_tp_price` in `sltp.rs`; pass `0.0` when no position is open.
- `bars_held` comes from `short_bars_held.get(&asset_sym).copied().unwrap_or(0)`.
- Both fields are `0.0`/`0` when `position_size.abs() < f64::EPSILON`.
- Both call sites (~1069 and ~3123) already have `book.entry_price(asset_sym)`, `short_bars_held`, and `sltp_state` in scope. The `sltp_state` BTreeMap at the live path needs to be confirmed available; if not, pass `0.0` and file a follow-up.
- Both `Raw`/`Oracle` and `Causal` branches of `build_decision_seed` receive the new fields.

**Patterns to follow:** The existing `build_decision_seed` pattern — add fields to `DecisionSeedInput`, propagate into the `serde_json::json!` macro block.

**Test scenarios:**
- When position_size is 0.0: all new fields are 0.0/0 in the returned JSON.
- When long position open: `entry_price` matches `book.entry_price`, `unrealized_pnl_pct` is positive when mark > entry, negative when mark < entry.
- When stop-loss is active: `stop_loss_price` reflects the effective SL price from `sltp_state`.
- `bars_held` increments correctly across consecutive bars in position.
- Both `Causal` and `Raw` InputsPolicy branches include the new fields.

**Verification:** Existing eval integration tests pass. The `DecisionSeedInput` struct is fully constructed at both call sites without compile errors.

---

### U3. EpisodicObservation struct and IndicatorFeatureVector

**Goal:** Define the structured observation type and the indicator feature vector used for similarity retrieval.

**Requirements:** R1, R3

**Dependencies:** none

**Files:**
- `crates/xvision-engine/src/agent/episodic.rs` (create)

**Approach:**
- `EpisodicObservation` captures: `bar_timestamp: String`, `decision_idx: u32`, `action: String`, `conviction: f64`, `entry_price: Option<f64>`, `exit_reason: Option<String>`, `rationale_excerpt: String` (first 120 chars of the trader justification), `indicators: IndicatorSnapshot`.
- `IndicatorSnapshot` mirrors the filter context shape where available: `rsi: Option<f64>`, `macd_hist: Option<f64>`, `ema_cross: Option<f64>` (derived as `ema_12 - ema_26`), `volume_zscore: Option<f64>`.
- `IndicatorFeatureVector` is a normalized `[f64; 4]` (one per indicator) extracted from `IndicatorSnapshot`. Missing indicators use `0.0`. Normalization uses fixed expected ranges per indicator (RSI 0–100, MACD hist ±500, EMA cross ±500, vol zscore ±3.0). Values are clamped then divided by range half.
- A `cosine_similarity(a: &[f64; 4], b: &[f64; 4]) -> f64` utility lives in this module.
- `EpisodicObservation::feature_vector(&self) -> [f64; 4]` extracts the feature vector from `self.indicators`.

**Patterns to follow:** The `IndicatorSnapshot` shape in `crates/xvision-engine/src/eval/filter_hook.rs` (the existing filter event records the same indicator fields).

**Test scenarios:**
- `feature_vector` returns `[0.0; 4]` when all indicator fields are `None`.
- RSI=70.0 normalizes to approximately +0.4 (RSI midpoint is 50, half-range is 50 → (70-50)/50 = 0.4).
- `cosine_similarity` returns 1.0 for identical vectors, 0.0 for orthogonal, -1.0 for opposite.
- Indicator values outside expected ranges are clamped (RSI=110 treated as 100).
- `rationale_excerpt` truncates at 120 chars with no panic on short strings.

**Verification:** Unit tests in `episodic.rs` pass. The module compiles as part of the `xvision-engine` crate.

---

### U4. EpisodicStore — accumulation and similarity retrieval

**Goal:** Implement the in-memory store that accumulates observations and returns the top-K most similar observations for a given indicator query.

**Requirements:** R2, R3, R8, R9

**Dependencies:** U3

**Files:**
- `crates/xvision-engine/src/agent/episodic.rs` (extend)

**Approach:**
- `EpisodicStore { observations: Vec<EpisodicObservation>, max_observations: usize }`.
- `push(&mut self, obs: EpisodicObservation)`: appends; when `len() >= max_observations`, drops the oldest observation (front of vec). Default `max_observations` = 500.
- `query(&self, query_vec: [f64; 4], k: usize) -> Vec<&EpisodicObservation>`: computes cosine similarity of `query_vec` against each stored observation's `feature_vector()`, returns the top-k by descending similarity. When `observations` is empty, returns an empty vec (drives R9).
- The query is O(n) over stored observations — at most 500 entries, negligible cost per decision.
- Expose a `to_seed_json(&self, query_vec: [f64; 4], k: usize) -> Option<serde_json::Value>` convenience that returns `None` when the store is empty (drives the omit-when-empty requirement R9).

**Patterns to follow:** The existing `BTreeMap`-based state stores in `backtest.rs` (e.g., `short_bars_held`, `sltp_state`) as the model for per-run stateful structures.

**Test scenarios:**
- Empty store: `query()` returns empty vec, `to_seed_json()` returns `None`.
- Single observation: `query()` returns it regardless of similarity.
- Multiple observations: the highest-cosine-similarity observation is first in the result.
- When `push` exceeds `max_observations`: the oldest observation is dropped, len stays at max.
- `to_seed_json` with k=3 returns at most 3 entries, serialized correctly.
- Tie-breaking: when two observations have identical similarity, order is deterministic (stable sort by decision_idx descending).

**Verification:** Unit tests pass. `EpisodicStore::new(500)` compiles and is usable from `backtest.rs`.

---

### U5. Wire EpisodicStore into the backtest executor (write path)

**Goal:** Create an `EpisodicStore` per run in the backtest executor and write a structured `EpisodicObservation` after each state-changing decision.

**Requirements:** R2, R6 (partial — write side)

**Dependencies:** U3, U4, U2 (for enriched position fields)

**Files:**
- `crates/xvision-engine/src/eval/executor/backtest.rs` (modify run loop)
- `crates/xvision-engine/src/agent/mod.rs` (re-export `episodic` module)

**Approach:**
- Instantiate `EpisodicStore::new(500)` at the start of `run_inner()`, alongside the existing `book`, `sltp_state`, and `short_bars_held` state.
- After `TraderOutput` is parsed and the decision action is known, if `action != flat && action != hold`: extract `EpisodicObservation` from the parsed trader output and current bar context, then call `store.push(obs)`.
- The `rationale_excerpt` comes from `parsed.justification` (first 120 chars, trimmed).
- The `IndicatorSnapshot` is populated from `filter_trigger_context` when present (it already carries the indicator values that fired the filter), falling back to `None` fields when no filter context is available.
- The write happens after the decision is recorded to the DB but before advancing to the next bar.
- The live execution path (line ~3123) is excluded from this unit — it is a separate context with different state lifetimes. File a follow-up for live-path wiring.

**Patterns to follow:** The existing `short_bars_held` maintenance pattern in the backtest loop (mutated in-place at the decision point, passed by reference to helpers).

**Test scenarios:**
- After a `long_open` decision: the store contains one observation with `action="long_open"`, correct `entry_price`, and `rationale_excerpt` ≤ 120 chars.
- After a `flat` decision: the store is not written to.
- After a `stop_loss` synthetic decision (from SLTP trigger): write an observation with `exit_reason="stop_loss"`.
- After `max_observations` writes: the store length stays at `max_observations`.
- Integration: a backtest run with 10 non-flat decisions produces a store with ≤ 10 observations.

**Verification:** An existing eval integration test with a known strategy produces a store with the expected number of non-flat observations (add an assertion or log line under test flag).

---

### U6. Inject episodic recall into the decision seed (read path)

**Goal:** Before each LLM dispatch, query the `EpisodicStore` using the current bar's indicator features and inject the result as `prior_episodes` into the seed JSON.

**Requirements:** R3, R4, R9

**Dependencies:** U4, U5

**Files:**
- `crates/xvision-engine/src/eval/executor/backtest.rs` (modify seed construction at lines ~1069)
- `crates/xvision-engine/src/agent/execute.rs` or `backtest.rs` (injection site)

**Approach:**
- At the seed construction site (after `build_decision_seed` returns the base seed, around line ~1086 where `filter_context` is currently injected), extract the indicator feature vector from `filter_trigger_context` if present, otherwise build a zero-vector.
- Call `episodic_store.to_seed_json(query_vec, k=5)` — returns `None` when empty.
- When `Some(episodes_json)`: insert into the seed object as `"prior_episodes"`.
- When `None`: no insertion — the seed is unchanged (R9).
- `k=5` is a starting constant; it can be made configurable later.
- The injection happens in `backtest.rs` at the seed construction point, not inside `execute_slot`, to keep `execute_slot` unaware of the episodic store (it is an executor-level concern, not an agent-slot concern).

**Execution note:** Add a unit test that constructs a minimal seed with known `prior_episodes` and asserts the JSON shape before touching execute_slot.

**Test scenarios:**
- When store is empty: seed JSON has no `prior_episodes` key.
- When store has 3 observations and k=5: `prior_episodes` contains ≤ 3 entries.
- When store has 10 observations: `prior_episodes` contains exactly 5 entries (top-5 by similarity).
- The injected `prior_episodes` entries match the expected `EpisodicObservation` JSON shape (all required fields present, `rationale_excerpt` ≤ 120 chars).
- Seed JSON is valid (no parse errors) with and without `prior_episodes`.

**Verification:** A backtest run with known non-flat early decisions produces a seed with `prior_episodes` for later decisions. The field is absent at decision 0 (store empty).

---

### U7. Structured cortex-mem Observation write

**Goal:** Replace the raw LLM response text in the cortex-mem `MemoryRecorder::record` call with a structured serialization of `EpisodicObservation`, improving distillation quality for the autooptimizer's offline Pattern extraction.

**Requirements:** R6

**Dependencies:** U3, U5

**Files:**
- `crates/xvision-engine/src/agent/execute.rs` (modify the post-EndTurn memory write path)
- `crates/xvision-engine/src/agent/memory_recorder.rs` (check `record` signature compatibility)

**Approach:**
- The current write in `execute_slot` sends the raw `assistant_text` (the full JSON blob `{"action":"...", "conviction":..., "justification":"..."}`) as the Observation text.
- Replace with a compact JSON serialization of a minimal `EpisodicObservation` built from the parsed `TraderOutput` — `action`, `conviction`, `rationale_excerpt`, and indicator context if available via `SlotInput`.
- The `MemoryRecorder::record` takes `text: &str` — pass `serde_json::to_string(&obs).unwrap_or_default()`.
- Only write when `action != flat` (consistent with the store write gate in U5).
- The `IndicatorSnapshot` is not available in `execute_slot` (that context lives in the executor). Pass `IndicatorSnapshot::default()` (all None) from `execute_slot`; the executor-level episodic store write (U5) already captures the full indicator context.

**Patterns to follow:** The existing `recorder.record(...)` call site in `execute.rs` after the final `EndTurn` detection.

**Test scenarios:**
- Cortex-mem observation text is valid JSON (parseable as `EpisodicObservation`).
- `flat` action: no cortex-mem write occurs.
- `long_open` action: observation text includes `action="long_open"` and `conviction` field.
- The `rationale_excerpt` in the written observation does not exceed 120 chars even when the justification is very long.
- Existing cortex-mem integration tests (if any) still pass after the text format change.

**Verification:** A backtest run with memory mode enabled writes cortex-mem observations that are valid `EpisodicObservation` JSON rather than raw LLM response blobs. Confirm via `xvn` memory inspection or the agent-runs memory-events endpoint.

---

## System-Wide Impact

- `DecisionSeedInput` is a `pub` struct used in integration tests and potentially in the autooptimizer eval adapter (`crates/xvision-engine/src/autooptimizer/eval_adapter.rs`). All construction sites must be updated when new fields are added (compiler enforces this as a struct literal, not a builder pattern).
- The cortex-mem Observation format change in U7 is a soft breaking change for any downstream tool that parses raw Observation text expecting the old LLM-response format. The autooptimizer distillation pass should be verified to handle the new structured JSON shape (it currently embeds and stores text; the content change affects distillation quality, not the pipeline contract).
- The `EpisodicStore` adds a per-run heap allocation. At 500 observations × ~500 bytes/observation ≈ 250 KB per run in steady state. Negligible.

---

## Risks & Dependencies

- `sltp_state` availability at the live execution call site (line ~3123): the SLTP state may not be threaded through the same way as in the backtest path. U2 should verify and document the gap; fall back to `0.0` if unavailable.
- `filter_trigger_context` is only populated when a DSL filter is attached. Strategies without a filter have `None` indicator context, so episodic observations will have `IndicatorSnapshot { all None }` and the feature vector degrades to `[0.0; 4]`. Similarity retrieval still functions but is uninformative. Acceptable for the first wave; a follow-up can derive indicators from `bar_history` directly.
- `autooptimizer/eval_adapter.rs` constructs `DecisionSeedInput` for optimizer evaluation cycles. Adding non-optional fields requires updating that construction site — verify at U2 compile time.

---

## Sources & Research

- Post-mortem on eval run `01KTGEHNHMGZ4N0Q1ME4EXSJ6Y`: context IS fresh per decision; degradation was behavioral in `lfm2.5:8b`, not architectural.
- `crates/xvision-engine/src/eval/executor/backtest.rs` lines 4250–4310: `DecisionSeedInput` and `build_decision_seed` structure.
- `crates/xvision-engine/src/agent/execute.rs`: `execute_slot` memory write/recall hooks.
- `crates/xvision-memory/src/types.rs`: `Tier::Observation` vs `Tier::Pattern` distinction; temporal leakage filter.
- `docs/v2d-memory-overview.md`: operator-facing two-tier memory explanation.
- `docs/superpowers/specs/2026-05-24-cortex-memory-cline-dspy-flywheels.md`: cortex-mem + autooptimizer distillation architecture and gambletan/cortex attribution requirements.
