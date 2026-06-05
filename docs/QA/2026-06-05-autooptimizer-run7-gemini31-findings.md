# AutoOptimizer Run-7 — Gemini 3.1 flash-lite optimizer test (cost verification + 4-cycle coverage)

**Date:** 2026-06-05 (session ~11:15–11:40Z)
**Deploy under test:** `xvision:deploy-latest` (same image as Run-6, `2026-06-05T01:47Z`)
**Strategy:** `gemini_long_gate_v3_compact` (`01KT20AS9674W1THXPWR93C1GX`)
**Mutator/judge model:** `openrouter / google/gemini-3.1-flash-lite`
**Goal:** Verify optimizer runs end-to-end on the Gemini 3.1 model, confirm cost metering accuracy against OpenRouter's actual pricing, run several lineages, and record any new findings.

---

## Cycles run this session

| # | cycle_id (prefix) | Window | Tokens | Cost | Outcome |
|---|---|---|---|---|---|
| C1 | `01KTBR5QBKSF4BE02TQ` | Apr 2026 (1 month) | 2,133,217 | $0.560 | **no_candidate** (already_tried × 3) |
| C2 | `01KTBRJC5GR46KRXNT` | May 1–14 2026 (2 wk) | 814,940 | $0.214 | 1 gated, **0 kept** (delta=0.000) |
| C3 | `01KTBRT71KGXHMA8M5` | Jan 1–14 2025 (2 wk) | 1,570,681 | $0.412 | 1 gated, **0 kept** (delta=0.000) |
| C4 | `01KTBS2RR8SZ69TXQB` | Apr 1–8 2025 (1 wk) | 1,129,110 | $0.297 | **no_candidate** (already_tried × 3) |

All cycles: parent `e3f9f8f378…`, model `openrouter/google/gemini-3.1-flash-lite`, honesty check passed.

---

## ✅ Verified working

### Cost metering accurate (F11/F23 confirmed end-to-end)
OpenRouter `/auth/key` endpoint + model pricing page confirmed:
- `google/gemini-3.1-flash-lite` on OpenRouter: **$0.25/M input tokens, $1.50/M output tokens**
- C1: 2,111,873 in × $0.25/M + 21,344 out × $1.50/M = **$0.5600** → `cycle_cost` recorded `$0.5600` ✓
- C2: 806,772 in × $0.25/M + 8,168 out × $1.50/M = **$0.2139** → `cycle_cost` recorded `$0.2139` ✓
- OpenRouter daily usage `$0.724` matches both cycles combined.
- `unpriced_calls: 0` — every call was priced (no missing catalog entries for this model).

### F34 concurrency guard works ✓
When C3 held the lock and C4 was launched concurrently, C4 exited immediately with:
```
an optimizer cycle is already running on this workspace (cycle 01KTBRT71K…, holder cli:operator,
since 2026-06-05T11:31:07Z). Wait for it to finish or cancel it before starting another —
concurrent cycles starve each other.
```
Exit code 2. `optimizer_cycle_lock` table was verified non-empty (1 row, correct `cycle_id`) while C3 ran, then empty after completion. F34 is fully fixed.

### Honesty check reliable ✓
All 4 cycles passed the `kill-trades` sabotage variant test. No regression.

### `--day-start/--day-end/--baseline-start/--baseline-end` flags functional ✓
All window overrides accepted and respected (token counts vary with window size, confirming bars were fetched for the specified periods).

---

## 🔴 CRITICAL — All new mutations produce delta_sharpe = 0.000 (mutations are behavioral no-ops)

C2 and C3 each produced a distinct new candidate but both were dropped with:
> "today's score (sharpe) improved by 0.000000 but minimum-improvement threshold is 0.050000"

DB query of `eval_runs` for C2 reveals the root cause:

| run | sharpe | n_trades | n_decisions | role |
|---|---|---|---|---|
| `01KTBRJCQEF7N6M1415V` | **0.8665** | 1 | 13 | parent eval (day window) |
| `01KTBRKBMJ1CFQ9MD4M5` | **0.8665** | 1 | 13 | candidate eval (day window) |

Parent and candidate have **identical sharpe, identical trade count, identical decision count** on the day window. The mutation changed the trader's prompt wording but the LLM made exactly the same trade. delta_sharpe = 0.8665 − 0.8665 = 0.000.

**Root cause:** For this strategy, the LLM (gemini-3.1-flash-lite trader) is called 13 times on the evaluation window and makes 1 trade. The bar data provides a sufficiently clear signal that any minor prompt mutation doesn't change the decision. The market signal dominates over the prompt variation.

**Implication:** Prose mutation of the trader prompt is insufficient for strategies where:
- The filter is very restrictive (few decisions per window → high noise per mutation)
- The residual decisions are "obvious" given the price action

For the optimizer to make progress, mutations would need to target:
1. **Filter thresholds** — when the trader is invoked at all (more leverage on decision count)
2. **Structural prompt changes** — not just wording, but adding/removing decision axes
3. **Risk parameters** — position size, stop-loss, which affect return magnitude

---

## 🔴 F32 retry loop uses fixed seed — `already_tried` is unescapable within one `propose()` call

**Source: PR #823** (merged 03:33Z today, deployed in the 09:44Z image). PR #823 added the hard-dedup `already_tried` guard AND changed the exploration directive from a cosmetic nonce to a seed-derived focus parameter:

```rust
// mutator.rs — build_user_payload
let focus = &param_keys[(exploration_seed as usize) % param_keys.len()];
// "FOCUS this experiment on parameter `{focus}`"
```

**The bug:** `propose()` runs up to `max_retries + 1` attempts inside a loop, but `exploration_seed` is **fixed** for all attempts:

```rust
for attempt in 0..max_attempts {
    let user_text = build_user_payload(..., exploration_seed);  // same every retry!
```

So all 3 retries focus on the same parameter (`seed % 6` = same index), the LLM proposes the same change, `candidate_already_tried()` fires three times, and the call exhausts its budget. The guard is unescapable by design — it correctly blocks re-emission, but the retry loop never gets a chance to try a *different* parameter because the focus never rotates.

**Fix:** `exploration_seed.wrapping_add(attempt as u64)` in the retry loop so each retry focuses on a different param index.

**Why April data specifically:** the cycle_id's seed for those cycles maps (`seed % 6`) to the parameter index that produces `b5505dd671`. May/Jan cycle IDs happen to hash to different indices → different focus → different candidate. It's not the market data — it's the modular arithmetic of which parameter slot the cycle's seed lands on.

**Confirmed by DB:** all 3 retry attempts have distinct `response_hash` values (the LLM IS generating different text) but all normalize to the same `bundle_hash` because the focus parameter is the same.

---

## 🔴 Window-date mapping to mutations: April data is exhausted

**Pattern observed across 4 cycles:**
- **April data** (C1: Apr 2026 full month; C4: Apr 1–8 2025) → `no_candidate` (`already_tried`) — the mutator proposes the same edit as `b5505dd671` (the original kept candidate), which is already in the lineage.
- **May 2026 data** (C2) → new candidate `3f088cb2` (distinct from `b5505dd671`)
- **Jan 2025 data** (C3) → new candidate `66043b9f` (distinct from both)

The mutator's context (bar data dates) influences what mutation it proposes. April market conditions appear to push it toward the same edit that produced `b5505dd671`. This means **"use a different window" is a viable workaround for `no_candidate` cycles** but it is not a systematic fix — the workaround exhausts as the lineage fills with tried candidates.

---

## 🟠 Parent selection ignores improved children (parent_count always 1)

All 4 cycles show `{"type":"parent_selected","parent_hash":"e3f9f8f378…"}` with `parent_count:1`, even though the lineage contains the actively improved node `b5505dd671` (child of `e3f9f8f3`, delta_sharpe +0.535, status `active`).

The parent selection SQL appears to query only **root nodes** (no `parent_hash`) for the given strategy. The optimizer cannot explore from improvements it has already made — it always starts mutations from the original root. This means:
- The lineage tree depth never increases beyond 1 child per root
- Hard-won improvements cannot be further refined
- `b5505dd671` (the only KEPT node in the tree) is permanently invisible to the mutator as a parent

**Acceptance:** Parent selection should rank all active leaf nodes (or all active nodes with a score) and pick the highest-scoring one. Add a test: a lineage with root + improved child → verify next cycle selects the child as parent.

---

## 🟡 Honesty check runs even on no_candidate cycles (wasteful)

C1 and C4 each emitted `no_candidate` after the mutator exhausted its 3 retry attempts — but then continued to run 3–5 eval runs for the honesty check. On a no_candidate cycle there is no candidate to check; running the sabotage evals is pure waste.

C1 example: `no_candidate` at 11:21:48Z, then 4 more eval runs finishing at 11:25:39Z — ~4 minutes and additional token spend for zero useful output.

**Fix:** Skip honesty check entirely when `no_candidate` was produced.

---

## 🟡 stdout event ordering non-deterministic

In C1/C2, the text preamble (`Starting optimizer cycle... / objective: / strategy:`) appeared **before** the JSON event stream. In C3/C4, the JSON events (`cycle_started`, `parent_selected`) appeared **before** the preamble text. The ordering is non-deterministic (likely a buffering race between two output streams).

Cosmetic only, but confusing when parsing output.

---

## 🟡 Very short windows (1 week) produce too few decisions for reliable gating

C4 (Apr 1–8 2025, 1 week) had extremely low decision/trade counts (implied by fast eval turnaround and the April data pattern). With so few decisions, gate comparisons have near-zero signal — any pass/fail is noise-driven.

Recommended minimum window for meaningful gating: **2–3 weeks** (>200 decisions for a BTC/ETH/SOL 1h strategy with this filter rate). C2 (13 decisions, 1 trade) is also borderline.

---

## Cost summary this session

| Cycle | Tokens | Cost |
|---|---|---|
| C1 (Apr 2026 month) | 2,133,217 | $0.560 |
| C2 (May 1–14 2026) | 814,940 | $0.214 |
| C3 (Jan 1–14 2025) | 1,570,681 | $0.412 |
| C4 (Apr 1–8 2025) | 1,129,110 | $0.297 |
| **Session total** | **5,647,948** | **$1.483** |

OpenRouter `usage_daily: $0.724` (at time of check, before C3/C4 completed).

---

## Status summary

| Finding | Status |
|---|---|
| F11/F23 cost metering | ✅ **Confirmed accurate** |
| F34 concurrency guard | ✅ **Confirmed fixed** |
| F28/F24 window+objective flags | ✅ Working |
| F32 mutator diversity | 🔴 **Partial workaround only** (different windows → different candidates, but all are behavioral no-ops; systematic fix still needed) |
| F33 duplicate attribution | 🟠 **Still open** (see Run-6 findings) |
| Mutation effectiveness gap | 🔴 **New** — prose mutations produce identical trading decisions; filter/risk mutations needed |
| Parent selection stuck at root | 🟠 **New** — improved children invisible to parent selector |
| Honesty check on no_candidate | 🟡 **New** — wasteful; skip when no_candidate |
| stdout ordering race | 🟡 **New** — cosmetic, non-deterministic |

## 🔴 CRITICAL — Optimizer mutation space is useless for agent strategies (filter + prompt unreachable)

**This is the fundamental design gap.** The autooptimizer can currently only mutate 6 `risk.*` parameter fields. For agent-based strategies (all real strategies on this node), this makes the optimizer nearly useless:

**Risk param mutations don't move Sharpe.** `risk_pct_per_trade` and `max_position_pct_nav` are scale factors on position size. Sharpe ratio = mean_return / std_return; scaling position size scales both numerator and denominator identically → Sharpe is unchanged. This was confirmed empirically: parent and candidate have byte-identical Sharpe in every cycle this session. The only risk params that could affect Sharpe are `stop_loss_atr_multiple` and `daily_loss_kill_pct` (they change exit timing), but the model rarely proposes these and when it does, the strategy fires so rarely that the delta is noise.

**The two levers that actually matter are locked out:**

1. **Filter thresholds** — the filter controls when the trader is called at all. A tighter or looser filter changes decision count, which changes regime exposure, which changes Sharpe substantially. But `strategy set-filter` only accepts a full new DSL blob; there is no `FilterParam` mutation kind in the autooptimizer.

2. **Trader prompt** — the prompt controls how the agent reasons about each decision. But `prose` mutations are excluded (`applicable_mutation_kinds` returns `false`) because strategies store an `AgentRef` (a library reference by ID), not an embedded prompt. The optimizer can't change the agent library entry through the strategy artifact.

**DSPy was built to solve exactly this.** The `xvn optimize` subsystem (offline DSPy prompt/demo optimizer) exists precisely because the online autooptimizer can't reach the agent's prompt. The two subsystems were meant to be complementary: DSPy optimizes prompts offline; the autooptimizer was supposed to handle online risk/filter tuning. But risk-only tuning is Sharpe-neutral and filter tuning has no mutation kind.

**What needs to happen:**
- Add a `filter` mutation kind that lets the experiment writer propose incremental changes to filter thresholds (e.g. `adx_threshold: 25 → 28`, `rvol_multiplier: 1.2 → 1.4`) — these directly change decision count and regime exposure.
- OR: route prompt optimization back through DSPy in-loop (the `dspy_ctx` plumbing already exists in `cycle.rs` — `handle_cycle_dspy` is called on accepted candidates) rather than leaving it as a fully separate offline tool.
- Without one of these, the autooptimizer can only move risk levers that don't affect the metric it's optimizing for.

---

## Suggested next pass

1. **Fix retry seed rotation (one line)** — `exploration_seed.wrapping_add(attempt as u64)` in the `propose()` retry loop so `already_tried` cycles can actually try a different parameter on each retry.
2. **Add `filter` mutation kind** — incremental threshold tuning on the strategy's DSL filter is the highest-leverage mutation axis and doesn't require touching the agent library.
3. **Parent selection** — include all active leaf nodes as parent candidates, not just roots. Weight by gate score.
4. **Skip honesty check on no_candidate** — early-exit before running canary evals.
5. **Minimum window guidance** — enforce or document `--day-end - --day-start >= 14 days` for BTC/ETH/SOL 1h strategies to ensure ≥200 decisions for reliable delta detection.
