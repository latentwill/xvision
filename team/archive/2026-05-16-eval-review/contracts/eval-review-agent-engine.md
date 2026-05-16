---
track: eval-review-agent-engine
lane: foundation
wave: eval-review
worktree: .worktrees/eval-review-agent-engine
branch: task/eval-review-agent-engine
base: origin/main
status: merged
pr: 186
depends_on:
  - eval-review-data-model      # merged via #176
blocks:
  - eval-review-api-cli
  - eval-review-run-detail-ui
stacking: none
allowed_paths:
  - crates/xvision-engine/src/eval/review/**
  - crates/xvision-engine/src/eval/store.rs       # review insert helpers only
  - crates/xvision-core/src/agent_profiles.rs
  - docs/superpowers/specs/2026-05-15-eval-review-agent.md
forbidden_paths:
  - crates/xvision-engine/migrations/**
  - crates/xvision-dashboard/**
  - frontend/web/**
  - crates/xvision-cli/**
interfaces_used:
  - EvalRunStore::load_run
  - EvalRunStore::load_decisions
  - EvalRunStore::load_equity_samples
  - AgentProfileRegistry
  - LlmProviderDispatcher
parallel_safe: false
parallel_conflicts:
  - eval-review-api-cli         # both will edit eval/review module surface
verification:
  - cargo test -p xvision-engine eval::review
  - cargo test -p xvision-core agent_profiles
acceptance:
  - Bounded review payload built from persisted artifacts (metrics_json, eval_decisions, eval_equity_samples, scenario + agent metadata).
  - Strict-JSON contract enforced on review response; malformed → inconclusive verdict, no panic.
  - Evidence references validated against the payload; unverifiable references → review marked inconclusive with explanation.
  - Sparse payloads produce `inconclusive` verdict with reason, not invented findings.
  - Review record persisted via the data-model layer; findings normalized into `eval_findings`.
  - Low-temperature deterministic dispatch parameters wired through.
---

# Scope

Implement the engine half of the Eval Review Agent feature from
`docs/superpowers/specs/2026-05-15-eval-review-agent.md`. The data-model layer
(eval_reviews + agent_profiles seeds + review-linked findings) already
landed via PR #176. This track adds the runtime that builds the review
payload, dispatches it through a selected review agent profile, validates
the response, and persists the normalized review.

# Out of scope

- Dashboard API routes (next track: `eval-review-api-cli`).
- `xvn eval review` CLI verb (next track: `eval-review-api-cli`).
- Run-detail UI (next track: `eval-review-run-detail-ui`).
- Schema changes — the migrations and tables are already in place.
- Autoresearcher mutation loop, lineage, marketplace, settlement (deferred per spec).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/eval-review-agent-engine -b task/eval-review-agent-engine origin/main
```

# Notes

- Review prompt must explicitly enumerate what the payload contains and what
  it does not — preventing the model from inventing orders, fills, or logs.
- A bounded retry on parse failure is allowed only if it is idempotent and
  recorded in `eval_events`.

- PR: https://github.com/latentwill/xvision/pull/186 (merged 2026-05-16).
