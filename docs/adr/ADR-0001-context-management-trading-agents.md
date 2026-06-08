# ADR-0001: Context management for AI trading agents

Date: 2026-06-07
Status: accepted
Deciders: operator (edkenne)
Implementation plan: [docs/plans/2026-06-07-001-feat-agent-context-memory-bridge-plan.md](../plans/2026-06-07-001-feat-agent-context-memory-bridge-plan.md)

---

## Context

A post-mortem on eval run `01KTGEHNHMGZ4N0Q1ME4EXSJ6Y` (strategy `lfm_rsi_macd_15m_scalper`, model `lfm2.5:8b` via Ollama) surfaced two questions:

1. Is the eval harness accumulating context across decisions, causing model degradation?
2. Can the agent learn from decisions it made earlier in the same run?

Source investigation covered `crates/xvision-engine/src/eval/executor/backtest.rs`, `crates/xvision-engine/src/agent/execute.rs`, and `crates/xvision-memory/src/`.

---

## Findings

### Finding 1: Context is already fresh per decision

Every decision cycle calls `execute_slot` with a newly initialized `messages: Vec<Message>`. There is no cross-decision transcript accumulation. The fixed seed cost per decision is approximately 14–16k input tokens, driven by:
- System prompt (agent-authored)
- Bar-history window (capped at `warmup_bars` entries)
- `portfolio_state` JSON (position size, equity, mark price)
- Within-decision tool-use turns (appended during the tool loop; dropped at next decision)

The `lfm2.5:8b` degradation in decisions 111–114 (schema confusion leaking into the justification field) and the empty response at decision 115 (`stop_reason=EndTurn`, 76 output tokens, no text content) were behavioral failures of a small 8B model across sequential independent calls — not context overflow or cross-decision accumulation.

**Consequence:** Designs that propose "make context fresh per decision" are proposing something that already exists. The real gaps are: *within-run continuity* (the agent cannot recall its own earlier decisions) and *incomplete position context* (the agent cannot see its entry price or SLTP levels from the seed).

### Finding 2: Cortex-mem's temporal filter correctly blocks within-run Pattern recall

The existing cortex-mem architecture (gambletan/cortex, via `xvision-memory`) has two tiers:
- `Tier::Observation` — written after each decision, never recalled at decision time
- `Tier::Pattern` — recalled at decision time; created only by the offline autooptimizer distillation pass

The temporal safety filter excludes any Pattern whose `training_window_end >= current_scenario_start`, preventing a backtest from recalling knowledge learned from data inside the window it is currently replaying. This filter is correct and must be preserved.

**Consequence:** Cortex-mem cannot provide within-run continuity by design. Patterns from the same scenario window would be a form of backtest leakage.

### Finding 3: The portfolio_state seed is structurally incomplete

The trader receives `position_size`, `equity`, and `mark_price`. The following are tracked internally by the executor but not surfaced in the seed:
- `entry_price` — available via `book.entry_price()`
- `unrealized_pnl_pct` — derivable from entry price, position size, mark price
- `bars_held` — maintained in `short_bars_held` BTreeMap
- `stop_loss_price` / `take_profit_price` — computed by `effective_sl_price` / `effective_tp_price` in `sltp.rs`

After the stop-loss at decision ix=109 in the failing run, the model had no way to know its entry price, how long it had held, or where its stop was, contributing to increasingly confused justifications in decisions 110–114.

---

## Decision

### D1: Fresh-per-decision is the confirmed correct architecture. Do not change it.

Cross-decision transcript accumulation is the wrong tool for trading agent context. It grows token cost linearly, inflates latency, and causes degradation in smaller models. The current architecture — fresh seed per decision — is correct.

### D2: Within-run continuity uses a run-local episodic store, not cortex-mem

A run-local `EpisodicStore` (in-memory, scoped to one eval run, not persisted) accumulates structured observations for state-changing decisions (entries, exits, stop-outs). Before each new decision, the store is queried by semantic similarity against the current bar's indicator features, and the most relevant past observations — from anywhere in the run — are injected into the seed as `prior_episodes`.

This design:
- Provides **relevance-based recall** (not recency-based), so an early decision matching the current setup is surfaced regardless of its position in the run
- Avoids backtest leakage entirely — the store is destroyed at run end and never crosses scenario boundaries
- Does not require an external embedder — indicator feature-vector cosine similarity is sufficient for within-run recall
- Keeps cortex-mem focused on its intended role: cross-run, cross-strategy learning via offline autooptimizer distillation

### D3: The two-tier memory model is preserved and extended

| Tier | Mechanism | Scope | Recalled by |
|---|---|---|---|
| Run-local episodic | `EpisodicStore` (in-memory Vec) | One eval run | Indicator feature-vector similarity |
| Cross-run patterns | cortex-mem `Tier::Pattern` (SQLite) | Agent lifetime | Embedding similarity (temporal filter applied) |

Cortex-mem Observation writes are improved from raw LLM response text to structured `EpisodicObservation` JSON, improving the quality of offline Pattern distillation.

### D4: portfolio_state is enriched with the full position context the trader needs

`entry_price`, `unrealized_pnl_pct`, `bars_held`, `stop_loss_price`, and `take_profit_price` are added to `DecisionSeedInput` and `build_decision_seed`. All values are already computed inside the executor; this change surfaces them in the structured seed.

---

## Rationale for feature-vector similarity over embeddings in the episodic store

Neural embeddings would give richer semantic matching. For within-run recall they are the wrong trade-off:

- The backtest executor is a tight loop. Embedding each observation adds synchronous I/O and model inference overhead on every decision.
- For within-run recall, the relevant similarity is market-structural: RSI range, MACD momentum, EMA trend direction, volume context. These map directly to a normalized indicator feature vector.
- A four-dimensional feature vector with cosine similarity is interpretable, debuggable, and fast. Embeddings are opaque.
- The `EpisodicStore` query interface is designed to accept an optional embedding vector alongside the feature vector in future, so the embedder path can be added without a breaking change.

---

## Consequences

- `DecisionSeedInput` gains five new required fields. All construction sites (backtest, live, autooptimizer eval adapter) must be updated.
- The cortex-mem Observation format changes from raw LLM text to structured JSON. Downstream tooling that parses raw Observation text must adapt.
- The `prior_episodes` field in the decision seed is new. It is omitted when the store is empty (early in a run), so system prompts must not assume it is always present.
- Live execution path wiring of the `EpisodicStore` is deferred; the first wave targets the backtest path.

---

## Alternatives considered

**A. Fixed recency window in the seed (last N decisions).** Rejected: recency is the wrong axis. A decision from 50 bars ago at the same RSI/MACD setup is more relevant than the last 3 flat decisions. A fixed window also inflates token cost at a predictable rate regardless of relevance.

**B. Enable cortex-mem Pattern recall within the same run.** Rejected: requires bypassing the temporal leakage filter. The filter exists for backtest integrity. The run-local episodic store achieves within-run continuity without touching the filter.

**C. Include full decision history in the seed.** Rejected: grows token cost linearly (~23k extra tokens by run end at 115 decisions). Causes the degradation pattern diagnosed — small models struggle to synthesize long structured lists. Relevance-based retrieval is strictly better.
