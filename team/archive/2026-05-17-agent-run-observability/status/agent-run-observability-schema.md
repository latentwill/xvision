---
track: agent-run-observability-schema
worktree: .worktrees/agent-run-observability-schema
branch: task/agent-run-observability-schema
phase: pr-open
last_updated: 2026-05-17T02:00:00Z
owner: claude-opus
---

# What I'm doing right now

PR open: https://github.com/latentwill/xvision/pull/200

New `xvision-observability` crate (Apache-2.0 via workspace baseline) plus
migration 018 (10 tables: `agent_runs`, `spans`, `checkpoints`,
`model_calls`, `tool_calls`, `approvals`, `sandbox_results`,
`supervisor_notes`, `artifacts`, `events`). Redactor v1, content-addressed
blob store, observability.toml loader with env-var precedence and
`full_debug` startup WARN. No event bus, no recorder, no emission — those
are the next two Phase A leaves.

# Blocked on

Nothing. Waiting on conductor merge.

# Next up (post-merge)

- Open `agent-run-observability-event-bus` (depends on this schema).
- Open `agent-run-observability-retention-cli` (depends on this schema).
- Both can run in parallel after this lands.

# Notes

- 24/24 tests passing locally (20 unit + 4 integration).
- Engine build clean with migration registered in `api/mod.rs`.
- `cli_jobs.job_id` is the PK (not `id`) — caught by the integration test
  before merge; FK in 018 references `cli_jobs(job_id)`.
