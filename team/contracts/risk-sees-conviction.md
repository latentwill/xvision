---
track: risk-sees-conviction
lane: leaf
wave: eval-honesty-2026-05-21
worktree: .worktrees/agent-a0f48260fbcd498fa
branch: task/risk-sees-conviction
base: origin/main
status: pr-open
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-risk/**
  - crates/xvision-core/src/trading.rs
  - team/contracts/risk-sees-conviction.md
forbidden_paths:
  - frontend/**
  - crates/xvision-eval/**
interfaces_used:
  - RiskRule (trait)
  - RiskLayer::evaluate
  - RiskLayer::evaluate_with_conviction
  - RiskEvalContext
  - TraderDecision
  - RiskDecision
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-risk -- -D warnings
  - cargo test -p xvision-risk
acceptance:
  - "the risk-eval context (RiskEvalContext) exposes `conviction: f32`"
  - "with default risk config, evaluate() and evaluate_with_conviction() at any conviction level produce byte-identical RiskDecision — no default rule scales size by conviction"
  - "a new test (risk_sees_conviction.rs) demonstrates a user-authored rule that opts into scaling and reads ctx.conviction"
  - "RiskLayer::evaluate() unchanged signature — existing callers in xvision-eval/src/harness.rs compile without modification"
  - "RiskLayer::prepend_rule / append_rule allow composing user rules around the built-in chain"
---

# Scope

Exposes `conviction` to the risk evaluation layer so user-authored risk
policies can scale sizing by it if they choose. The engine never enforces a
default `size *= conviction` mapping.

Implementation: a new `RiskEvalContext<'a>` struct in
`crates/xvision-risk/src/context.rs` bundles `decision`, `portfolio`, `asset`,
and the new `conviction: f32` field. The `RiskRule::evaluate` trait method is
changed from `(&TraderDecision, &PortfolioState, AssetSymbol)` to
`(&RiskEvalContext<'_>)`. All 9 built-in rules are updated; none read
`conviction`. The public `RiskLayer::evaluate` signature is unchanged so
existing callers in `xvision-eval` and `xvision-harness` do not need
modification. A new `RiskLayer::evaluate_with_conviction` overload accepts the
conviction value from callers that have it. Two new hooks
`RiskLayer::prepend_rule` and `append_rule` let user policies inject rules
around the built-in chain.

# Out of scope

- Modifying `TraderDecision` struct (no `conviction` field added there).
- Any default `size *= conviction` logic in the engine.
- Wiring conviction from the LLM response through the eval executor to the
  risk layer (that is a follow-on plumbing task; the seam is now open).
- `frontend/**` and `crates/xvision-eval/**` changes.

# Sync-before-work ritual

```bash
git fetch --prune origin
git -C .worktrees/agent-a0f48260fbcd498fa status
git -C .worktrees/agent-a0f48260fbcd498fa log --oneline -3 origin/main..HEAD
```

# Notes

2026-05-22: Implemented. `RiskEvalContext` added to `xvision-risk/src/context.rs`.
All 9 rule impls updated to `fn evaluate(&self, ctx: &RiskEvalContext<'_>)`.
Tests in `crates/xvision-risk/tests/risk_sees_conviction.rs` cover:

- regression: default config ignores conviction at all values 0.0..1.0
- user rule: ConvictionScale reads ctx.conviction and scales size_bps
- sentinel: AssertConviction verifies the value propagates exactly
