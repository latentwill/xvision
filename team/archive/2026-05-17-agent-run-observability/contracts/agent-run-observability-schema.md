---
track: agent-run-observability-schema
lane: foundation
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-schema
branch: task/agent-run-observability-schema
base: origin/main
status: pr-open
depends_on: []
blocks:
  - agent-run-observability-event-bus
  - agent-run-observability-retention-cli
  - agent-run-observability-ipc-emission
stacking: none
allowed_paths:
  - crates/xvision-observability/**
  - crates/xvision-engine/migrations/018_agent_run_observability.sql
  - crates/xvision-engine/migrations/018_agent_run_observability.down.sql
  - Cargo.toml
  - crates/xvision-engine/Cargo.toml
  - LICENSE
  - NOTICE
forbidden_paths:
  - crates/xvision-engine/src/agent/**
  - crates/xvision-agent-client/**
  - xvision-agentd/**
  - frontend/web/src/**
  - team/board.md
  - team/OWNERSHIP.md
interfaces_used:
  - sqlx::SqlitePool (from existing xvision-engine store)
  - existing `eval_runs.id`, `cli_jobs.id` FK targets
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo build -p xvision-observability
  - cargo test -p xvision-observability
  - cargo build --workspace
  - sqlite3 :memory: < crates/xvision-engine/migrations/018_agent_run_observability.sql
acceptance:
  - New crate `crates/xvision-observability/` exists under Apache-2.0, added to workspace members.
  - Migration `018_agent_run_observability.sql` creates all 10 tables from the plan with the exact column lists (agent_runs, spans, checkpoints, model_calls, tool_calls, approvals, sandbox_results, supervisor_notes, artifacts, events).
  - Migration is reversible via `018_agent_run_observability.down.sql`.
  - Rust models for every table (`AgentRunRow`, `SpanRow`, `CheckpointRow`, …) with `sqlx::FromRow`, plus enum types for `RunStatus`, `SpanStatus`, `SpanKind`, `SideEffectLevel`, `RiskLevel`, `RetentionMode`, `CapabilityPath`.
  - `xvision-redactor` v1 — secret-pattern regex pass over a `&str` returning `RedactedString`. Covers: AWS/Anthropic/OpenAI/Alpaca/Orderly API key patterns, JWTs, hex private keys, mnemonic phrases.
  - Content-addressed blob store at `$XVN_HOME/agent_runs/blobs/<sha256>` with `write_blob(&[u8]) -> BlobRef` and `read_blob(&BlobRef) -> Vec<u8>`. Path resolution honours `XVN_HOME` env or `~/.config/xvn` fallback.
  - Config loader for `$XVN_HOME/config/observability.toml` with the schema from the plan (sqlite_enabled, otel_enabled, retention.{mode, store_*, redact_secrets, payload_ttl_days, max_payload_bytes}). Defaults match the plan: `mode = "hash_only"`, `sqlite_enabled = true`, `otel_enabled = false`.
  - LICENSE file at repo root is Apache-2.0 (or this contract notes if LICENSE is being added separately by the Cline migration's step 1 instead).
  - Unit tests for: blob store roundtrip, redactor on each secret pattern, config loader precedence stub (CLI > env > config > default), migration apply + rollback.
  - **No recorder trait, no event bus, no emission. Those land in `agent-run-observability-event-bus`.**
---

# Scope

Phase A leaf #1 of the agent-run-observability wave (plan:
`docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`).

Land the canonical row schema, the new crate, redactor v1, blob store, and
config loader. No event bus, no recorder, no emission — those are the next
two leaves. This contract is intentionally narrow so it can land in parallel
with Cline migration steps 0–7 (the new tables don't need the sidecar to
exist).

# Out of scope

- The `RunEventBus` and the `AgentRunRecorder` trait → `agent-run-observability-event-bus`.
- Subscribing recorders to event bus events → `agent-run-observability-event-bus`.
- CLI surface (`xvn obs retention …`) → `agent-run-observability-retention-cli`.
- IPC emission, OTel bridge, export, UI → Phase B leaves (gated on Cline migration step 3+).
- Touching `crates/xvision-engine/src/agent/**` — that directory is being deleted by the Cline migration; this contract must not edit it.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-schema \
  -b task/agent-run-observability-schema origin/main
```

# Notes

- Migration 018's exact column list is in the plan under "Schema (migration
  018)". Do not deviate — Phase A leaves down the chain typecheck against
  these column names.
- `LICENSE` (Apache-2.0) and `NOTICE` may already be landed by the time this
  contract opens via the Cline migration's step 1 baseline. If so, leave
  them alone — touch only the `xvision-observability` crate's per-crate
  license headers. If LICENSE is NOT yet landed, this contract may add the
  root LICENSE + NOTICE; coordinate with the conductor before doing so.
- The `xvision-redactor` v1 is allowlist-driven and small (one pass of
  regexes + a per-tool allowlist of fields to redact). It is NOT a general
  PII scrubber — that would be over-scope.
- Use `chrono::DateTime<Utc>` for `started_at` / `finished_at` and serialize
  as RFC3339 strings (matches the existing eval_runs columns).
