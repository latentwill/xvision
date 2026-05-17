---
track: qa-role-normalization
lane: leaf
wave: qa-2026-05-17
worktree: .worktrees/qa-role-normalization
branch: task/qa-role-normalization
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/strategies/validate.rs
  - crates/xvision-engine/src/strategies/agent_ref.rs
  - crates/xvision-engine/src/agent/pipeline.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/tests/role_normalization.rs
forbidden_paths:
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/src/agent/execute.rs
  - crates/xvision-engine/src/strategies/store.rs
  - crates/xvision-engine/migrations/**
  - frontend/**
interfaces_used:
  - "xvision_core::strategy::AgentRef"
  - "xvision_engine::agent::pipeline::PipelineOutputs"
  - "xvision_engine::eval::executor::{backtest,paper}::trader_model_id"
parallel_safe: false
parallel_conflicts:
  - qa-execute-slot-cap
verification:
  - cargo build -p xvision-engine
  - cargo test -p xvision-engine strategies::validate
  - cargo test -p xvision-engine strategies::agent_ref
  - cargo test -p xvision-engine agent::pipeline
  - cargo test -p xvision-engine eval::executor
  - cargo test -p xvision-engine --test role_normalization
acceptance:
  - "`AgentRef.role` is normalized at the mutation boundary (trim + reject empty + canonical case). Persisted role strings never contain leading/trailing whitespace"
  - "A single canonicalization is chosen and applied consistently — either preserve case + exact comparison everywhere OR lowercase canonical keys everywhere. No mixing"
  - "`validate_agent_pipeline` inserts the normalized role into its role set; edge validation comparisons use the same canonical form"
  - "`run_agent_pipeline` output assignment uses the canonical role key, so attached roles like `Trader`, `TRADER`, or `\" trader \"` produce a populated `PipelineOutputs.trader` field (not `MissingResponse`)"
  - "`trader_model_id` helpers in `eval/executor/{backtest,paper}.rs` use the same canonical comparison, so the reasoning-class truncation hint fires for all accepted trader-role variants"
  - "Regression tests in `tests/role_normalization.rs` cover: (a) attached `Trader`/`TRADER`/` trader ` produces `PipelineOutputs.trader`; (b) whitespace-padded roles cannot be persisted; (c) `trader_model_id` returns the right model for all variants"
  - "No backwards-compat shim for stored roles with leading/trailing whitespace — surface a validation error at the next mutation (pre-launch breaking change is acceptable per repo guardrails)"
---

# Scope

Implements remediation step 4 of `qa/2026-05-17-comprehensive-codebase-review.md`,
combining three related findings:

- **P2 — Attached `Trader` role passes eval validation but is dropped from
  pipeline outputs.**
- **P2 — Role validation allows whitespace variants that can bypass exact
  graph role references.**
- **P3 — Reasoning-class truncation hint misses accepted whitespace-padded
  trader roles.**

All three stem from the same drift: `AgentRef.role` is inconsistently
trimmed / case-folded / compared across the engine. The fix is to
canonicalize at the mutation boundary in `agent_ref.rs` (or its accessor
in `strategies/validate.rs`) and then make every downstream comparison use
the canonical form.

# Out of scope

- Anything in `crates/xvision-engine/src/api/eval.rs` — that file is
  owned this wave by `qa-eval-retry-params-override`. The existing
  `validate_eval_trader_source` `eq_ignore_ascii_case` check is already
  permissive and becomes a no-op once upstream normalization lands.
- `agent/execute.rs` — owned by `qa-execute-slot-cap`.
- Strategy filesystem store path validation — owned by
  `qa-strategy-id-path-safety`.
- Migrations to scrub existing stored roles with whitespace padding. The
  contract is pre-launch breaking; whitespace roles fail at next mutation
  and the operator re-saves with the trimmed value.

`parallel_conflicts` lists `qa-execute-slot-cap` because both tracks
touch `crates/xvision-engine/src/agent/` — but on disjoint files
(`execute.rs` vs `pipeline.rs`). Coordinate on rebases; do not edit each
other's files.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/qa-role-normalization \
  -b task/qa-role-normalization origin/main
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision"
git -C .worktrees/qa-role-normalization status
```

# Notes

Implementation hints (do not rewrite the contract — use as starting points):

- The 2026-05-12 strategies refactor made slot names free-text; the
  canonical role key is local to comparisons, not a hardcoded set.
- Lowercase canonical keys is probably the cheaper choice — fewer touch
  points than preserving case everywhere. Document whichever you pick in
  a one-liner near `AgentRef::role`.
- Single-writer claim on `eval/executor/{backtest,paper}.rs` is currently
  released; this track re-claims via the contract.
- The QA reviewer specifically calls out that drift here will worsen as
  graph execution lands — the goal is to leave the codebase in a state
  where any new comparison site picks the canonical form by default.
