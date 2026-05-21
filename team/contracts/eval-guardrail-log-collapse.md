---
track: eval-guardrail-log-collapse
lane: leaf
wave: eval-honesty-2026-05-21
worktree: .worktrees/eval-guardrail-log-collapse
branch: task/eval-guardrail-log-collapse
base: origin/main
status: in-progress
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/guardrail_summary.rs
  - crates/xvision-engine/src/eval/mod.rs
  - crates/xvision-engine/src/eval/executor/backtest.rs
  - crates/xvision-engine/src/eval/executor/paper.rs
  - crates/xvision-engine/src/api/eval.rs
  - crates/xvision-engine/tests/eval_guardrails.rs
  - crates/xvision-engine/tests/eval_guardrail_summary.rs
  - team/contracts/eval-guardrail-log-collapse.md
forbidden_paths:
  - crates/xvision-engine/src/eval/guardrails.rs
  - crates/xvision-engine/src/eval/store.rs
  - frontend/**
interfaces_used:
  - RunStore::read_supervisor_notes
  - RunStore::read_decisions
  - RunStore::record_finding
  - Finding (crates/xvision-engine/src/eval/findings/mod.rs)
  - Severity (crates/xvision-engine/src/eval/findings/mod.rs)
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-engine -- -D warnings
  - cargo test -p xvision-engine -- eval::executor
acceptance:
  - Both per-decision tracing::warn! calls in backtest.rs:760 and paper.rs:932 are demoted to tracing::debug! (same fields, same message string).
  - At run finalize (after fire_auto_review in api/eval.rs), guardrail_summary::fire_guardrail_summary is called best-effort on the same pattern as fire_auto_review.
  - guardrail_summary::summarise_notes() is a pure function that accepts &[(role,severity,content)] and total_decisions:usize and returns Option<GuardrailSummaryResult> with severity, counts by (original, applied, reason), and most-common pair.
  - 0 guardrail notes → no Finding emitted, function returns None.
  - guardrail_note_count/total_decisions >= 0.5 → Severity::Critical finding.
  - guardrail_note_count/total_decisions >= 0.10 (and < 0.5) → Severity::Warning finding.
  - guardrail_note_count/total_decisions > 0 (and < 0.10) → Severity::Info finding.
  - Finding kind is "guardrail_rewrite_rate"; title matches "guardrail rewrote {n}/{total} trader actions ({pct}%)".
  - Finding body (description) includes per-reason counts and most-common (original, applied) pair.
  - One tracing::warn! emitted at finalize when at least one guardrail-block occurred, summarising counts by (original_action, applied_action, reason).
  - Unit tests cover the four severity thresholds (0, 1-in-100, 15-in-100, 60-in-100).
---

# Scope

Demotes per-decision guardrail `tracing::warn!` calls in the backtest and paper
executors to `tracing::debug!` (the `supervisor_notes` row is the durable
record). At run finalize, reads the guard-role supervisor notes, aggregates them
into a per-run `Finding` with severity based on the rewrite rate, and emits one
`tracing::warn!` summary line. Implements the `eval-guardrail-log-collapse` item
from `team/intake/2026-05-21-eval-honesty-and-agent-graph.md`.

QA motivation: 432/432 trader actions were rewritten on xvnej-app in 24 h, each
emitting a WARN line that buried the actual pattern signal.

# Out of scope

- The guardrails module itself (`eval/guardrails.rs`) — pure rule logic unchanged.
- The `supervisor_notes` table writes — already correct.
- Bar-history limit, indicator wiring, provider attestation, or any other intake track.
- Frontend.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/eval-guardrail-log-collapse status
git -C .worktrees/eval-guardrail-log-collapse log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/eval-guardrail-log-collapse -b task/eval-guardrail-log-collapse origin/main
```

# Notes

2026-05-21: Initial implementation. Created guardrail_summary.rs, wired into
api/eval.rs finalize paths, demoted per-decision WARN → DEBUG in both executors,
added unit tests in eval_guardrail_summary.rs and extended eval_guardrails.rs.
