---
track: eval-flat-degeneracy-early-stop
lane: leaf
wave: eval-traces-2026-05-19
worktree: .worktrees/eval-flat-degeneracy-early-stop
branch: task/eval-flat-degeneracy-early-stop
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs       # outer-loop iteration only; do NOT touch bar_seed (F-6) or apply seam (F-7)
  - crates/xvision-engine/src/eval/executor/backtest.rs    # same
  - crates/xvision-engine/src/eval/store.rs                # supervisor_notes write helper (already added by F-7); may also need eval_decisions insert for inherited decisions
  - crates/xvision-engine/src/eval/early_stop.rs           # NEW — pure-function policy + counter state
  - crates/xvision-engine/src/eval/mod.rs                  # only `pub mod early_stop;`
  - crates/xvision-engine/tests/**
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-engine/src/eval/executor/mod.rs    # F-3 owned this; stay out
  - frontend/web/**
interfaces_used:
  - xvision-engine::eval::store::record_supervisor_note (added by F-7 / PR #353 — depend on it landing, or duplicate the helper if F-7 doesn't merge first)
parallel_safe: true
parallel_conflicts:
  - eval-causal-input-sanitization (PR #354, F-6 — owns bar_seed/ohlcv_to_json in same files)
  - engine-trade-guardrails-pyramid-flip-block (PR #353, F-7 — owns apply seam in same files; you depend on its supervisor_note helper)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::early_stop
  - cargo test -p xvision-engine eval::executor
acceptance:
  - New `eval::early_stop` module with a pure-function `should_skip_next_decision(&recent_actions: &[Action], &recent_convictions: &[f64], threshold_low_conviction: f64) -> Option<SkipPlan>` where `SkipPlan { skip_count: u32, reason: String }`. Returns `Some(SkipPlan { skip_count: M, reason: "..." })` only when:
    * the last K decisions (default K=8) are ALL `flat` (or `hold`), AND
    * every conviction in the window is `<= threshold_low_conviction` (default 0.2), AND
    * no portfolio state change occurred in the window (caller tracks this; helper receives a `portfolio_unchanged: bool` flag).
  - When triggered, the executor's outer per-bar loop skips the next M bars (default M=4): no LLM call for those bars; instead it writes inherited `eval_decisions` rows with `action='flat'`, `conviction=0.0`, `justification="inherited from early-stop policy"`, and a single `supervisor_notes` row at the entry of the skip window (`role='guard'`, `severity='info'`, `content="early-stop: <K> low-conviction flats; skipping <M> bars"`).
  - The counter is reset by:
    * any non-flat / non-hold action,
    * any portfolio state change (a position opens, closes, or its size changes),
    * a new asset entering the active set.
  - Defaults are constants in the module; environment overrides `XVN_EARLY_STOP_WINDOW`, `XVN_EARLY_STOP_SKIP`, `XVN_EARLY_STOP_CONVICTION` documented in the module docstring.
  - Tests:
    * Pure unit tests on `should_skip_next_decision` covering: 8 flats + low conviction + no state change → `Some(skip=4)`; 7 flats → `None`; 8 flats with one conviction above threshold → `None`; 8 flats + state change → `None`.
    * Integration backtest: emit a scenario where the model returns 12 consecutive flats; assert that 4 of them are inherited (no model call), the supervisor_notes row was written, and the equity samples are continuous.
    * Idempotency: a second skip window can trigger after the counter resets, not before.
  - Audit acceptance: `01KS03Z0BRCTDM1MX8BRRGMQP5` (the 720-decision BTC backtest that produced 20 consecutive low-conviction flats at the start) would, with these defaults, have saved ~4-5 model calls in that opening sequence and accrued the same on every subsequent flat-degeneracy episode.
---

# Scope

Intake F-9 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The audit found:
- Run `01KS03Z0BRCTDM1MX8BRRGMQP5` produced 20 consecutive `flat` decisions with conviction ≤ 0.2 and near-identical justifications at the start — ~460k input tokens spent to produce 20 copies of "I don't know".
- The justification *"No clear volatility expansion or breakout signal."* appears 14 times verbatim across runs; *"No overextended shock candle detected for mean reversion."* 15 times.

This is the cheapest token-saving mechanism we can add. It's strictly a *scheduling* change at the outer per-bar loop — it does not modify the LLM call path, the bar serialization (F-6), the apply seam (F-7), or the guardrails.

# Out of scope

- LLM call retries / backoff (F-2, PR #347).
- Bar serialization changes (F-6, PR #354).
- Apply-side guardrails (F-7, PR #353).
- Prompt caching / rolling window (F-8, not yet started).
- Frontend / dashboard surfacing of skip-window markers (eval-comparison dashboard can read the supervisor_notes rows as-is).

# Coordination

Three contracts now declare conflicts on paper.rs/backtest.rs:
- F-6 owns `bar_seed` / `ohlcv_to_json`
- F-7 owns the apply seam (broker-call site + simulate_fill)
- F-9 (this) owns the outer per-bar loop's iteration counter + the skip-window writeback

Different functions; whoever lands second rebases. F-7's `record_supervisor_note` helper is reused — depend on PR #353 landing OR duplicate the helper inline (small) if rebases get ugly.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/eval-flat-degeneracy-early-stop status
git -C .worktrees/eval-flat-degeneracy-early-stop log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-flat-degeneracy-early-stop -b task/eval-flat-degeneracy-early-stop origin/main
```

# Notes

Keep the policy pure-functional in `early_stop.rs` so it can be unit-tested without an executor. The executor integration is just a thin wrapper that maintains the window state across the loop.
