---
track: agent-run-observability-foundation
lane: foundation
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-foundation
branch: task/agent-run-observability-foundation
base: origin/main
status: merged
pr: https://github.com/latentwill/xvision/pull/197
merged_at: 2026-05-17T01:00:00Z
merge_commit: 3104b9f3c42218ba71b9404798f822056a94745a
depends_on: []
blocks:
  - agent-run-observability-schema
  - agent-run-observability-otel-bridge
  - agent-run-observability-export
  - agent-run-observability-ui
stacking: none
allowed_paths:
  - docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md
  - docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md
  - team/intake/2026-05-17-agent-run-observability.md
forbidden_paths:
  - crates/**
  - frontend/web/src/**
  - team/contracts/**
  - team/board.md
  - team/OWNERSHIP.md
interfaces_used:
  - (planning only — no production interfaces touched)
parallel_safe: true
parallel_conflicts: []
verification:
  - test -f docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md
  - grep -q "Evaluation Gate" docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md
acceptance:
  - The spec's three open questions (harness choice, span storage shape, prompt retention toggle) each have a written resolution or explicit deferral with rationale.
  - A new plan file `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md` exists, scoped to **only** the trace/report layer (spec follow-up item #1) — not the full harness rewrite.
  - The plan enumerates the leaf tracks the conductor will open in the next wave (schema, OTel bridge, export, UI), with allowed_paths sketched per track.
  - The plan calls out reuse vs. new-build decisions against existing surfaces (`eval_runs`, `eval_decisions`, `api_audit`, `cli_jobs`, eval-review-agent post-hoc reviews).
  - The spec is amended with a "Status: evaluated" stamp pointing at the new plan, or kept as-is with a rationale logged in the plan.
---

# Scope

Move the agent run observability work from V2-territory into v1 by writing
the implementation plan the spec gates on. This is a **planning-only** track:
no Rust, no frontend, no schema changes. Output is a plan in
`docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md` that the
conductor can decompose into leaf contracts.

See `team/intake/2026-05-17-agent-run-observability.md` for the operator-side
framing.

Scope of the plan (not of this track):

- Tables, Rust models, and migration order for `agent_runs`, `run_spans`,
  `model_calls`, `tool_calls`, `approvals`, `sandbox_results`,
  `supervisor_notes` — or a justified merged-schema variant.
- Where OTel emission attaches in `crates/xvision-engine/src/agent/`
  (likely `agent/llm.rs`, `agent/execute.rs`, `agent/pipeline.rs`).
- The `xvn_run.json` export schema (schema_version `xvn.agent_run.v1`) and
  `xvn_report.md` template — both stable enough for autooptimizer
  ingestion.
- The Run Detail UI route and its relationship to the existing
  `/eval-runs/:id` view (separate route vs. tab on the same page).

# Out of scope

- Writing any Rust code or migrations.
- Touching `crates/**` or `frontend/web/src/**`.
- Decomposing leaf contracts (the conductor does this after the plan
  lands).
- Harness adapter rewrite — explicitly deferred to a later wave per the
  spec's own follow-up sequence.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-foundation \
  -b task/agent-run-observability-foundation origin/main
```

# Notes

Spec gate quoted in the intake doc. Key constraint: the spec deliberately
narrows the first deliverable to the trace/report layer — do not let the
plan sprawl into the harness adapter or the autooptimizer ingestion
contract. Those land later, after the trace/report schema stabilizes.
