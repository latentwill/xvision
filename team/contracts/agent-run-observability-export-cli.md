---
track: agent-run-observability-export-cli
lane: leaf
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-export-cli
branch: task/agent-run-observability-export-cli
base: origin/main
status: ready
depends_on:
  - agent-run-observability-ipc-emission
blocks:
  - agent-run-observability-ui
stacking: none
allowed_paths:
  - crates/xvision-cli/src/commands/run/**
  - crates/xvision-cli/src/commands/mod.rs
  - crates/xvision-cli/src/json/object_shapes.rs
  - crates/xvision-cli/tests/run_inspect.rs
  - crates/xvision-dashboard/src/routes/agent_runs.rs
  - crates/xvision-dashboard/src/routes/mod.rs
  - crates/xvision-observability/src/export.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/Cargo.toml
  - crates/xvision-observability/tests/export_schema.rs
forbidden_paths:
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-observability/src/rows.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-agent-client/**
  - xvision-agentd/**
  - crates/xvision-engine/migrations/**
  - frontend/web/**
interfaces_used:
  - xvision_observability::SqliteRecorder (read path)
  - xvision_observability::AgentRunRow / SpanRow / ModelCallRow / ToolCallRow / ...
  - xvision_dashboard::AppState
parallel_safe: true
parallel_conflicts:
  - qa-trace-json-download (crates/xvision-dashboard/src/routes/mod.rs registration row, crates/xvision-dashboard/src/routes/agent_runs.rs — coordinate via team/queue; qa-trace-json-download may land first, this track stacks on its registration)
verification:
  - cargo test -p xvision-observability --test export_schema
  - cargo test -p xvision-cli --test run_inspect
  - cargo test -p xvision-dashboard
  - cargo build -p xvision-cli
acceptance:
  - New CLI verb `xvn run inspect <id>` reads the SQLite agent-run rows and writes two files into the current directory (or `--out <dir>`): `xvn_run.json` (schema `xvn.agent_run.v1`) and `xvn_report.md` (plaintext markdown).
  - `xvn_run.json` includes the keys listed in the plan: `schema_version`, `run_id`, `objective`, `strategy_id`, `eval_run_id`, `status`, `retention_mode`, `started_at`, `finished_at`, `otel_trace_id`, `totals`, `spans` (recursive tree), `model_calls`, `tool_calls`, `approvals`, `sandbox_results`, `supervisor_notes`, `final_artifact`, and the IPC-emission additions `sidecar_version`, `cline_sdk_version`, `protocol_version`, `mcp_servers`, `skills`.
  - `xvn_report.md` header includes a `Retention: <mode>` line so reports never imply more retention than was on.
  - `--retention full_debug` runs surface a top-of-file warning banner in the report.
  - New HTTP routes on `xvision-dashboard`: `GET /api/agent-runs/:id` (returns the same JSON shape), `GET /api/agent-runs/:id/export.json` (same payload with `Content-Disposition: attachment`), `GET /api/agent-runs/:id/export.md` (markdown).
  - Auth: the new routes follow the same auth gating as the rest of `/api/agent-runs/**` (token / single-user gate per `qa-dashboard-auth-hardening` once merged; until then, behind the existing dashboard auth surface).
  - Snapshot test `export_schema.rs` produces a golden `xvn_run.json` from a deterministic fixture and asserts byte-for-byte stability (schema-version bump = intentional file change).
  - Idempotent: repeated `xvn run inspect <id>` on a finished run produces identical bytes.
---

# Scope

Read-side surface for the canonical agent-run SQLite ledger. The CLI verb
and the three HTTP routes together produce the deliverables the operator
attaches to PRs and the autoresearcher ingests downstream.

Schema-version discipline (`xvn.agent_run.v1`) is the load-bearing rule
here — see plan risk #5. Any future shape change must bump
`schema_version` rather than mutate v1 in place.

# Out of scope

- IPC emission — `agent-run-observability-ipc-emission`.
- OTel — `agent-run-observability-otel-bridge`.
- UI — `agent-run-observability-ui` (consumes these routes).
- Autoresearcher ingestion contract — deferred (per plan §Out of scope).
- Migration 018 — already merged.
- Write path of any kind on `agent_runs` / `spans` / detail tables.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-export-cli status
git -C .worktrees/agent-run-observability-export-cli log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-export-cli \
  -b task/agent-run-observability-export-cli origin/main
```

# Notes

Append checkpoints / PR links below.
