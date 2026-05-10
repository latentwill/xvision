# ADR 0005 — Lookahead bias audit

> **2026-05-10:** Project renamed `xianvec` → `xvision`. References below reflect the post-rename name; project history prior to this date used `xianvec`.

**Date:** 2026-05-03
**Status:** PASS
**Phase:** 4.5

## Question

Do Stage 1 (Intern) inputs avoid leaking future market data into briefings? Specifically:
- Do all indicator calculations respect the decision timestamp boundary?
- Does briefing context summarize history only, without forward-peeks?
- Are cache keys (`setup_id`) immutable and non-colliding per decision time?
- Are precomputed features computed left-to-right at fixed timestamps?

## Audit method

**Code review** of all market data reads in `xvision-intern`, `xvision-data`, `xvision-core` (market.rs, store.rs).

The codebase is in **early Phase 1.x** — stub status dominates. `xvision-data` is a pure-function indicator library with no state or orchestration. `xvision-intern` is a prompt builder and HTTP client that reads pre-populated `MarketSnapshot` structs. **No runtime orchestration code exists yet** that builds snapshots from raw OHLCV — that belongs in Phase 9 (backtest harness and forward pipeline).

**Audit scope:** verify type signatures, data flow boundaries, and the cache-key architecture. The actual lookahead-risk surface will be materialized when the snapshot-building code lands (Phase 9). This audit certifies that v1's *input contracts* are sound; the Phase 9 data pipeline must respect those contracts.

## Read inventory

### xvision-core (decision-time boundary)

| File | Line | Read site | Bounding mechanism | Status |
|------|------|-----------|-------------------|--------|
| `crates/xvision-core/src/market.rs:34` | MarketSnapshot.timestamp | Decision timestamp; read-only data holder | `MarketSnapshot.timestamp` is the authoritative snapshot time. Included in briefing; no code reads *beyond* this timestamp. | CLEAN |
| `crates/xvision-core/src/market.rs:37` | MarketSnapshot.recent_bars: Vec<Ohlcv> | Bar array, oldest-first. Last entry should match snapshot.timestamp. | Type allows arbitrary bars; no validation that last bar ≤ snapshot.timestamp. Validation is a Type-level concern in Phase 9 snapshot construction. | SMELL (deferred validation) |
| `crates/xvision-core/src/market.rs:47–62` | IndicatorPanel (RSI, SMA, MACD, etc.) | Pre-computed single point-in-time values | Indicators are scalar Option<f64>; timestamp of origin is implicit in MarketSnapshot.timestamp. No vector history, no sliding window. | CLEAN |
| `crates/xvision-core/src/store.rs:9` (comment) | Tier 1 fix #1 | "`briefings` keyed on `setup_id` alone" | Cache key is `(setup_id, provider, model)`. Two setups with different timestamps but same `setup_id` (impossible by design — setup_id is Uuid) would collide. The cache key includes provider + model, guaranteeing invalidation on backend swap. | CLEAN |

### xvision-intern (prompt building + caching)

| File | Line | Read site | Bounding mechanism | Status |
|------|------|-----------|-------------------|--------|
| `crates/xvision-intern/src/cache.rs:16–20` | CacheKey | `(setup_id, provider, model)` | Three-tuple key. `setup_id` is Uuid; per-setup uniqueness is guaranteed by the caller's setup orchestration (Phase 9). Provider + model enforce backend invalidation. | CLEAN |
| `crates/xvision-intern/src/prompt.rs:32–79` | `build_intern_prompt()` | Reads MarketSnapshot fields: timestamp, price, recent_bars, indicators, onchain, regime, horizon_hours | All fields are scalar or fixed-size array (recent_bars capped by PromptOpts.recent_bars_limit = 12 default). No cross-bar lookahead; prompt includes only the snapshot's current timestamp and recent history. | CLEAN |
| `crates/xvision-intern/src/prompt.rs:47–62` | Recent OHLCV iteration | Takes `.take(opts.recent_bars_limit)` from *reversed* vector, then reverses to chronological order. Bounds to 12 bars by default. | Reversal logic is correct: `rev().take(12).rev()` selects the 12 most recent bars in chronological order. No look-ahead. | CLEAN |
| `crates/xvision-intern/src/backend.rs:46–57` | `InternBackend::brief()` trait | Signature: `prompt: &str, setup_id, asset, regime, horizon_hours` | Prompt is a string (built by `build_intern_prompt`). Method assembles InternBriefing by filling setup_id, asset, regime, horizon_hours, created_at at call time. No forward data in the filling logic. | CLEAN |

### xvision-data (indicators)

| File | Line | Read site | Bounding mechanism | Status |
|------|------|-----------|-------------------|--------|
| `crates/xvision-data/src/indicators.rs:14–27` | `sma(prices, period)` | Iterates left-to-right: `out[period-1] = sum / period`, then `for i in period..n`. | **Left-to-right only.** Index `i` uses `prices[i-period..i]` (causal window). No future data. | CLEAN |
| `crates/xvision-data/src/indicators.rs:33–47` | `ema(prices, period)` | Seeded with SMA of first `period` values, then forward recursion: `prev = alpha * prices[i] + (1-alpha) * prev`. | **Causal recursion.** Computes only from index `period` onward; uses prior EMA state. No future samples. | CLEAN |
| `crates/xvision-data/src/indicators.rs:53–82` | `rsi(prices, period)` | Seed: simple average of first `period` deltas. Wilder smoothing: `alpha = 1/period`. Forward loop: `for i in period+1..n`. | **Causal.** Accumulator-style; `prices[i] - prices[i-1]` is the only per-iteration data access. No window beyond `[i-1, i]`. | CLEAN |
| `crates/xvision-data/src/indicators.rs:101–122` | `bollinger(prices, period)` | SMA of period-length window at each position: `prices[i+1-period..=i]`. | **Causal bounds check at line 107:** `for i in (period-1)..n` ensures we never read before index 0. The slice `[i+1-period..=i]` is the trailing window — never forward. | CLEAN |
| `crates/xvision-data/src/indicators.rs:133–158` | `atr(high, low, close, period)` | True Range: `tr[i] = max(hl, \|hc\|, \|lc\|)`. Wilder smoothing same as RSI. | **Causal.** TR uses only `i` and `i-1`; no future bars. Smoothing is forward recursion from index `period`. | CLEAN |
| `crates/xvision-data/src/indicators.rs:162–191` | `macd(prices, fast, slow, signal)` | Computes fast/slow EMA, then EMA of MACD line restricted to valid prefix. | **Causal chaining.** Fast/slow EMA are causal; signal EMA is computed only over positions where macd is non-NaN (valid prefix starts at max(fast, slow) period). No future data. | CLEAN |

### xvision-core (persistence + cache boundary)

| File | Line | Read site | Bounding mechanism | Status |
|------|------|-----------|-------------------|--------|
| `crates/xvision-core/src/store.rs:78–96` | `upsert_setup()` | Writes setup_id, asset, horizon_h, market_state_json, created_at. | Creates or updates a setup row. `setup_id` is the caller's Uuid; timestamp immutability is enforced at the application layer (Phase 9 harness). | SMELL (application-level validation missing in v1) |
| `crates/xvision-core/src/store.rs:103–121` | `upsert_briefing()` | Writes briefing_json keyed on setup_id. All paired strategy arms read the same row (Tier 1 fix #1). | **Correct by design.** One briefing per `setup_id`; every strategy arm is a side-effect reader via `get_briefing()`. The cache key (provider, model) ensures different LLM backends don't collide. | CLEAN |
| `crates/xvision-core/src/store.rs:124–134` | `get_briefing(setup_id)` | SELECT briefing_json WHERE setup_id = ? | One row per setup_id. No cross-setup leakage; schema has FK to setups(setup_id). | CLEAN |
| `crates/xvision-core/src/store.rs:138–152` | `insert_decision()` | Writes (setup_id, arm_name, decision_json). Primary key prevents duplicates per arm within a setup. | **Paired-arm keying:** arm_name distinguishes strategies. Two arms with identical decisions collide intentionally (deterministic on same briefing, temperature=0). Tier 1 fix #1 works correctly. | CLEAN |

## Cache-key analysis

**v1 cache model:** in-memory `HashMap<CacheKey, InternBriefing>` where `CacheKey = (setup_id, provider, model)`.

**Risk:** Two calls with the same `setup_id` but **different decision timestamps** could share a cached briefing if orchestration is buggy.

**Mitigation:** 
- `setup_id` is a Uuid (128 bits). Callers must create *distinct* Uuids for each setup; sharing `setup_id` across decision times is a caller bug, not an architecture bug.
- The decision-timestamp binding is a Phase-9 responsibility. The v1 audit certifies that the *cache contract* is sound: "same setup_id → same briefing (deterministic LLM)." The Phase-9 harness must never reuse a `setup_id` across different decision times.
- **Follow-up:** Add an assertion in `upsert_setup()` (Phase 9 harness code) that rejects duplicate setup_id. Store both the decision timestamp and the setup_id in the setups table for audit-trail completeness.

**Verdict:** CLEAN (with noted follow-up for Phase 9 harness).

## Findings

1. **Line `crates/xvision-core/src/market.rs:37` (recent_bars bounds)** — Type signature allows arbitrary OHLCV bars; no assertion that the last bar timestamp ≤ snapshot.timestamp. **Verdict: SMELL.** Mitigation: Phase 9 snapshot builder must validate that `recent_bars.last().timestamp <= snapshot.timestamp` before creating the MarketSnapshot. Not a code leak, but a contract violation risk.

2. **Line `crates/xvision-data/src/indicators.rs:14–191` (all indicator functions)** — All six core indicators (SMA, EMA, RSI, Bollinger, ATR, MACD) use only causal data: left-to-right, no future windows, no sliding look-around. **Verdict: CLEAN.**

3. **Line `crates/xvision-intern/src/prompt.rs:47–62` (recent bars in prompt)** — Recent OHLCV bars are capped to 12 by default and presented in chronological order. The Intern prompt explicitly states the decision timestamp (`Timestamp (UTC): ...`) so the LLM receives the temporal anchor. No forward bars are ever visible. **Verdict: CLEAN.**

4. **Line `crates/xvision-core/src/store.rs:78–96, 103–121` (setup + briefing persistence)** — The briefing cache is keyed on `setup_id` + `provider` + `model`. Tier 1 fix #1 is correctly wired: every paired strategy arm reads the *same* briefing for the same setup. **Verdict: CLEAN.** Note: cache-key immutability depends on the caller (Phase 9 harness) never mutating a setup_id or creating duplicate setups with different timestamps.

5. **Line `crates/xvision-core/src/market.rs:12–62` (MarketSnapshot structure)** — All fields are scalar or fixed-size aggregates. No history buffer; no precomputed features that might be computed over a sliding or future window. Indicators are point-in-time values tied to MarketSnapshot.timestamp. **Verdict: CLEAN.**

6. **Briefing context leakage (narrative check)** — The prompt builder formats indicators, onchain signals, and recent bars as static text without any temporal reference beyond the setup timestamp. The Intern is instructed: "Do NOT include a `candidate_direction` field" and "Each case must read as if its author believed it" (prompt.rs:177–180). No "price moved to X over the next 15 min" phrasing is baked in. **Verdict: CLEAN.**

## Decision

**Status: PASS**

The v1 Intern input layer is free of lookahead bias in the code and type system. All indicator calculations are causal (left-to-right, no future windows), all briefing context is historical, and the cache-key architecture correctly enforces pairing (Tier 1 fix #1).

**Boundary:** The actual lookahead risk surface sits in Phase 9 (snapshot orchestration code that does not yet exist). The v1 audit confirms that the *contracts* are sound. Phase 9 must honor:
- `MarketSnapshot.recent_bars` must satisfy `recent_bars.last().timestamp <= snapshot.timestamp`.
- `setup_id` must be globally unique per decision time (no reuse across different calendar times).

## Follow-ups

1. **Phase 9 harness: add setup_id reuse guard.** When `upsert_setup()` is called, assert that the `setup_id` is new or that `market_state_json` differs from the prior row for that setup_id. Log a warning if the same setup_id appears at different timestamps (a sign of orchestration confusion).

2. **Test: recent_bars boundary.** Add a fixture test in `market.rs` that constructs a MarketSnapshot with `recent_bars.last().timestamp > snapshot.timestamp` and verifies the prompt builder includes a visible timestamp anchor. Confirm no "future bar" text leaks.

3. **Documentation: snapshot contract.** Add a comment to `MarketSnapshot` documenting the invariants: recent_bars ≤ timestamp, setup_id uniqueness, and the implications for cache behavior.

---

**Audit complete. V1 cleared for Phase 2 advancement.**
