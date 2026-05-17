---
track: agent-run-observability-foundation
worktree: .worktrees/agent-run-observability-foundation
branch: task/agent-run-observability-foundation
phase: pr-open
last_updated: 2026-05-17T00:30:00Z
owner: claude-opus
---

# What I'm doing right now

PR open: https://github.com/latentwill/xvision/pull/197

Three operator decisions are locked in `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`:

1. Cline SDK is the harness adapter (sibling crate `xvision-cline-adapter`).
2. SQLite is the canonical execution ledger; OTel is an optional derived
   sink. Shared `spans` skeleton + specialized detail tables, no giant JSON
   rows. OTel attribute leakage of full prompts is forbidden at the
   recorder API.
3. Prompt retention is a three-mode policy (`hash_only` default |
   `redacted` | `full_debug`) with CLI > env > config > default
   precedence, a startup WARN line, and TTL + size caps on the
   content-addressed blob store.

Spec at `2026-05-15-xvn-agent-run-system-spec.md` is stamped "Evaluated
2026-05-17" with pointer to the new plan.

# Blocked on

Operator review of the plan. No follow-up leaves should be opened until
the plan is approved (the conductor freelance prevention rule).

# Next up (post-merge)

Conductor opens these leaves, in this order:

1. `agent-run-observability-schema` (foundation) — new `xvision-observability`
   crate, migration 018, recorder trait, `SqliteRecorder`, `NoopRecorder`,
   `xvision-redactor` v1, blob store, config loader.
2. `agent-run-observability-cline-adapter` (foundation) — new
   `xvision-cline-adapter` crate; wires Cline SDK into the existing
   `execute_slot` boundary.
3. `agent-run-observability-emission` (foundation) — insert recorder calls
   at the four emission sites in `crates/xvision-engine/src/agent/`.
4. `agent-run-observability-otel-bridge` (leaf) — cargo feature `otel`,
   `tracing-opentelemetry` plumbing, OTLP exporter.
5. `agent-run-observability-export-cli` (leaf) — `xvn run inspect <id>` +
   `GET /api/agent-runs/:id/export.{json,md}`.
6. `agent-run-observability-ui` (leaf) — `/agent-runs/:id` route + agent
   timeline.
7. `agent-run-observability-retention-cli` (leaf) — `xvn obs retention
   {show,set}` + per-invocation `--retention` flag + janitor.

# Notes

- The four emission sites are named with file:line in the plan so the
  conductor can carry them into the emission contract verbatim.
- The `agent-run-observability` wave does NOT replace V2B item 5 ("Audit
  and observability" — audit log + provider/broker health). The plan calls
  this out explicitly so the V2B item doesn't get reabsorbed.
- OTel is gated behind a cargo feature `otel` so the default
  `xvision:latest` build doesn't pick up OpenTelemetry crates.
