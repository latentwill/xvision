---
track: eval-broker-error-circuit-breaker
lane: integration
wave: qa-operator-2026-05-19
worktree: .worktrees/eval-broker-error-circuit-breaker
branch: task/eval-broker-error-circuit-breaker
base: origin/main
status: ready
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/eval/executor/mod.rs
  - crates/xvision-engine/tests/eval_broker_circuit_breaker.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-eval/src/result.rs
  - frontend/web/src/features/eval-runs/RunSummary.tsx
  - frontend/web/src/features/eval-runs/RunSummary.test.tsx
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-execution/src/alpaca.rs
  - crates/xvision-execution/src/broker_surface.rs
  - crates/xvision-risk/**
  - crates/xvision-core/src/trading.rs
interfaces_used:
  - BrokerSurface::submit_order (rejection outcome surface)
  - broker_call.finished span attributes (error_class, severity, outcome)
  - RunStatus::Failed
acceptance:
  - Eval run loop in `crates/xvision-engine/src/eval/executor/paper.rs`
    (around `run_inner`, line 302+) tracks consecutive identical
    `error_class` counts on broker-rejected outcomes where
    `severity >= warn` AND `outcome == rejected`. Successful broker
    outcomes (filled / accepted / partial fill) reset the counter.
    Different error classes do not accumulate together — a switch from
    `broker_min_order_size` to a transient network error resets.
  - When the consecutive count reaches the threshold (default `N = 3`,
    configurable via run config / strategy config — pick one and
    document the choice in the status note), the run aborts with
    `RunStatus::Failed`. The eval result carries a structured failure
    reason: `repeated_broker_error` class, the offending `error_class`,
    the count, and the last error message. Surface a one-line classified
    reason in `EvalResult` / `RunSummary` so the trace dock and eval
    list can display it.
  - The trader is not invoked again after the abort — the loop exits
    on the same iteration that hit the threshold. No additional broker
    calls fire post-abort.
  - Regression test `crates/xvision-engine/tests/eval_broker_circuit_breaker.rs`:
    a mock broker surface that returns `broker_min_order_size` rejections
    every call; assert the run aborts on the Nth rejection, that the
    final RunStatus is `Failed`, that the error message carries the
    structured reason, and that the broker mock recorded exactly N
    calls (not N+1, not infinite).
  - Second regression test: a broker mock that fails twice then
    succeeds on the third call — assert the run does NOT abort (counter
    resets on success) and reaches its natural end.
  - Third regression test: a broker mock that alternates error_classes
    (`broker_min_order_size`, then `broker_timeout`, then
    `broker_min_order_size`) — assert the run does NOT abort within 3
    cycles, because the classes differ.
  - Frontend: `RunSummary` displays the failure reason when present
    (`"Aborted after 3 consecutive broker_min_order_size rejections"`
    or similar). Existing `Failed` runs without the structured reason
    keep their current rendering — do not break the no-reason path.
  - No changes to `BrokerSurface`, `RiskLayer`, `TraderDecision`, or
    `VetoReason`. This track is purely a loop-control + status-reporting
    addition.
  - Threshold and reset semantics are explicit in code comments — a
    future reader can trace why the counter is gated on
    `(error_class, severity, outcome)` from the source alone.
parallel_safe: false
parallel_conflicts:
  - "risk-gate-min-notional: same wave, both touch crates/xvision-engine/src/eval/executor/paper.rs in (likely) disjoint regions. This track instruments the post-submit rejection-handling path; risk-gate-min-notional adds a pre-submit veto. If risk-gate-min-notional lands first, the immediate operator failure mode (broker_min_order_size) no longer reaches this track's counter at all — that's the intended layering. Coordinate via team/queue/ if diffs overlap."
  - "qa-trace-broker-spans (merged #283): owns the broker.call span schema (error_class, severity, outcome attributes). This track reads those attributes but must not change their schema."
verification:
  - cargo test -p xvision-engine --test eval_broker_circuit_breaker
  - cargo test -p xvision-engine
  - cargo test -p xvision-observability
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- --run RunSummary
---

# Scope

Defense-in-depth for the operator-reported broker rejection loop on
2026-05-19. Even with the root-cause fix in `risk-gate-min-notional`
preventing too-small orders from reaching the broker, a future
deterministic broker error class (tick size, max notional, asset
suspension, etc.) could still loop today's eval run forever. This
track adds a generic abort-on-repeated-error mechanism to the eval
run loop so a misconfigured run can't burn the operator's session.

Both layers ship: `risk-gate-min-notional` eliminates the known
failure mode pre-submit; this track is the safety net for unknown
future failure modes.

**Context as of 2026-05-19 (post-#314 + #286):** The error-classifier
fix in #314 + the self-healing feedback loop in #286 should mean a
broker rejection is now classified as recoverable, fed back to the
trader as `agent_error_feedback`, and the trader re-decides with a
corrected size next cycle. So under normal operation the loop the
operator saw shouldn't recur. **But the operator's run
`01KRZ18JTMZ1S7W1MBKC1PNNSJ` on 2026-05-19 ~02:33 UTC (post-#314
merge timestamp) DID loop** — same `broker_min_order_size` error
class, multiple consecutive rejections, no self-correction. Two
possible causes:

1. The live tailnet deployment lagged behind #314's merge — operator
   was running the old classifier/no-feedback image.
2. #314 + #286 are in the deployed image but the trader agent
   ignored the injected feedback and re-emitted the same size
   anyway.

Either way, this circuit-breaker is the safety net. It catches both
cases: (1) un-deployed fixes, and (2) cases where the agent simply
doesn't self-correct. **Priority: P1 — operator hit this 2026-05-19
even with the fixes notionally in place.**

The investigation of which root cause is actually responsible (1 vs 2)
is **out of scope for this track** — it's an operations question
(image rollout) and a separate agent-behavior question. File a
follow-up if (2) turns out to be the cause.

Anchor reading:

- `team/intake/2026-05-19-qa-operator-round-4.md` "Round-4 addendum"
  section, item 5 (Finding B — second half).
- `crates/xvision-engine/src/eval/executor/paper.rs:302+` for the
  `run_inner` decision loop.
- `team/archive/2026-05-18-qa-rounds/contracts/qa-trace-broker-spans.md`
  for the `error_class` / `severity` / `outcome` attribute schema this
  track reads.

# Out of scope

- Changing the broker error taxonomy. The classes (`broker_min_order_size`,
  `broker_timeout`, etc.) live in the broker-surface code; this track
  consumes them.
- Retry policy: this track aborts, it does not retry. If the operator
  wants intelligent retry with backoff, that's a follow-up.
- Risk-layer changes — `risk-gate-min-notional` owns the pre-submit veto.
- Per-asset / per-venue abort thresholds — single global N (or
  per-strategy config) for v1.
- Multi-asset cohort runs — same threshold applies per-run; cross-run
  policy is a separate observability concern.
- UI for configuring the threshold. v1 ships with a default constant
  (`N = 3`); a settings UI can land as a follow-up.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-broker-error-circuit-breaker status
git -C .worktrees/eval-broker-error-circuit-breaker log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-broker-error-circuit-breaker \
  -b task/eval-broker-error-circuit-breaker origin/main
```

# Notes

Append checkpoints / PR links below. The choice between
run-config-driven vs strategy-config-driven threshold is
acceptance-bearing — document in the status note before opening the
PR.
