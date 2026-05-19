---
track: engine-trade-guardrails-pyramid-flip-block
lane: integration
wave: eval-traces-2026-05-19
worktree: .worktrees/engine-trade-guardrails-pyramid-flip-block
branch: task/engine-trade-guardrails-pyramid-flip-block
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/guardrails.rs           # NEW FILE — typed guardrail enum + apply-side check
  - crates/xvision-engine/src/eval/mod.rs                  # only `pub mod guardrails;` declaration
  - crates/xvision-engine/src/eval/executor/paper.rs       # only the apply-decision section that calls broker; insert guardrail check
  - crates/xvision-engine/src/eval/executor/backtest.rs    # only the apply-decision section; insert guardrail check
  - crates/xvision-engine/src/eval/behavior.rs             # extend BehaviorSummary with a `guardrail_interventions` counter if natural; otherwise leave alone
  - crates/xvision-engine/src/eval/store.rs                # supervisor_notes write seam if not already present
  - crates/xvision-engine/migrations/020_supervisor_notes_seed.sql    # ONLY if a migration is genuinely required to seed `supervisor_notes` constraints — coordinate migration number with eval-causal-input-sanitization track (it claims 020); use 021 if 020 is taken on disk by the time you start
  - crates/xvision-engine/tests/**
forbidden_paths:
  - frontend/web/**
  - crates/xvision-engine/src/eval/executor/mod.rs         # the F-3 watchdog touched this; stay out
interfaces_used:
  - xvision-engine::eval::behavior::derive_behavior_summary (read-side detection of direct_flips already exists)
  - xvision-engine::eval::executor::{paper, backtest}::run (apply-decision section)
parallel_safe: true
parallel_conflicts:
  - eval-causal-input-sanitization (paper.rs/backtest.rs — different functions; bar_seed vs apply-decision)
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine eval::guardrails
  - cargo test -p xvision-engine eval::executor
acceptance:
  - New `eval::guardrails` module exposes a typed `GuardrailDecision` enum: `Allow`, `RewriteTo(action)`, `RejectWithNote(reason)`. Specifically:
    * `long_open` when the asset already has an open long → `RewriteTo(Hold)` with reason `"pyramid blocked"`.
    * `short_open` when the asset already has an open short → `RewriteTo(Hold)` with reason `"pyramid blocked"`.
    * `short_open` immediately after `long_open` (or vice versa) on the same asset → `RewriteTo(Flat)` to close first; the next decision can open the other side. Reason: `"one-step flip blocked"`.
    * All other (action, prior_state) combinations → `Allow`.
  - The guardrail runs **server-side at apply time** in both `paper.rs` and `backtest.rs`, before the broker call (paper) or position update (backtest). The model's original decision is preserved in `eval_decisions` for audit; the *applied* action is what hits the portfolio.
  - Each `RewriteTo`/`RejectWithNote` outcome writes a row to `supervisor_notes` with `role='guard'`, `severity='warn'`, and a content string containing the reason + the (original, applied) actions + decision_index + asset.
  - Tests:
    * Unit tests covering all four guardrail branches via a pure-function entry point.
    * Integration test: run a synthetic backtest where the model emits `[long_open, long_open, long_open, long_open]`; assert only the first opens a position, decisions 2-4 apply as `hold`, and `supervisor_notes` has 3 `pyramid blocked` rows.
    * Integration test: model emits `[long_open, short_open]`; assert decision 2 applies as `flat`, with one `one-step flip blocked` supervisor note.
  - No regression in existing `eval_decisions` semantics — the `action` column still records the *model's* decision; a new column or derived flag may record the *applied* action if desired, but is not required. (Defer to F-11 if you want a persisted `applied_action` column — out of scope here.)
  - The audit's worst offenders (run `01KRZ18JTMZ1S7W1MBKC1PNNSJ` with 26 consecutive long_opens; `01KRZKG8A1FHTBE88NPWTVQVYS` with 22 consecutive short_opens and 12 one-step flips) become tractable: in future runs of those same agents, the guardrail forces hygiene and the trace dock surfaces the count.
---

# Scope

Intake F-7 of `team/intake/2026-05-19-eval-traces-end-to-end-audit.md`.

The audit found that prompt-level rules ("Never add to a position",
"Never flip long↔short in one step") are violated in many runs:

- 26 consecutive `long_open` in `01KRZ18JTMZ1S7W1MBKC1PNNSJ`
- 22 consecutive `short_open` in `01KRZKG8A1FHTBE88NPWTVQVYS`
- 12 one-step flips in the same run

Move enforcement from prompt to engine. `crates/xvision-engine/src/eval/behavior.rs`
already counts these on the read side (`direct_flips`, etc.); this
contract is the apply-side enforcement counterpart.

# Out of scope

- A separately-tracked "applied_action" column on `eval_decisions`
  (defer to F-11).
- Changing the read-side behavior summary fields (only **extending**
  with a counter is allowed).
- Anything in `executor/mod.rs` — that file was touched by F-3 (watchdog
  PR #345). Stay clear.
- Frontend changes — the trace-dock surface will pick up the new
  supervisor_notes rows automatically.

# Migration coordination

`eval-causal-input-sanitization` claims migration 020. If you need a
new migration (you probably don't — `supervisor_notes` already exists
from migration 018), use **021** and update the MANIFEST.md migration
registry alongside `eval-causal-input-sanitization`'s update.

# Sync-before-work ritual

```bash
cd /root/deploy/xvision
git fetch --prune origin
git -C .worktrees/engine-trade-guardrails-pyramid-flip-block status
git -C .worktrees/engine-trade-guardrails-pyramid-flip-block log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/engine-trade-guardrails-pyramid-flip-block -b task/engine-trade-guardrails-pyramid-flip-block origin/main
```

# Notes

Coordinate with `eval-causal-input-sanitization` on paper.rs/backtest.rs.
Different functions; first to merge stays, second rebases.
