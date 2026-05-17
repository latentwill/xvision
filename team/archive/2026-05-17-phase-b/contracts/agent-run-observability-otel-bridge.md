---
track: agent-run-observability-otel-bridge
lane: leaf
wave: agent-run-observability
worktree: .worktrees/agent-run-observability-otel-bridge
branch: task/agent-run-observability-otel-bridge
base: origin/main
status: ready
depends_on:
  - agent-run-observability-ipc-emission
blocks: []
stacking: none
allowed_paths:
  - crates/xvision-observability/Cargo.toml
  - crates/xvision-observability/src/otel.rs
  - crates/xvision-observability/src/lib.rs
  - crates/xvision-observability/tests/otel_tee_smoke.rs
  - crates/xvision-observability/tests/otel_no_payload_lint.rs
  - docs/runbook/observability-otel.md
forbidden_paths:
  - crates/xvision-observability/src/bus.rs
  - crates/xvision-observability/src/sqlite.rs
  - crates/xvision-observability/src/recorder.rs
  - crates/xvision-observability/src/redactor.rs
  - crates/xvision-observability/src/blobs.rs
  - crates/xvision-observability/src/config.rs
  - crates/xvision-observability/src/janitor.rs
  - crates/xvision-observability/src/retention.rs
  - crates/xvision-observability/src/rows.rs
  - crates/xvision-observability/src/events.rs
  - crates/xvision-observability/src/types.rs
  - crates/xvision-agent-client/**
  - xvision-agentd/**
  - frontend/web/**
  - crates/xvision-engine/migrations/**
interfaces_used:
  - xvision_observability::RunEventBus
  - xvision_observability::AgentRunRecorder
  - xvision_observability::RunEvent
parallel_safe: true
parallel_conflicts: []
verification:
  - cargo test -p xvision-observability --no-default-features
  - cargo test -p xvision-observability --features otel
  - cargo build -p xvision-observability --features otel
acceptance:
  - New cargo feature `otel` on `xvision-observability`. Default build does NOT pull in `tracing-opentelemetry`, `opentelemetry`, or `opentelemetry-otlp`.
  - When `otel` feature is enabled: an `OtelTeeRecorder` wraps any other recorder, subscribes to the same `RunEventBus`, and emits a `tracing::span!()` per recorder call mapped to the OpenTelemetry span model.
  - `OTEL_EXPORTER_OTLP_ENDPOINT`, `OTEL_SERVICE_NAME`, `OTEL_RESOURCE_ATTRIBUTES` env vars are honored as the standard contract.
  - `agent_runs.otel_trace_id` and `spans.otel_trace_id` / `otel_span_id` columns are populated on every recorder write when the OTel feature is on; left NULL when off.
  - **Attribute-API lint rule**: the `OtelTeeRecorder` exposes attributes via a typed `Attribute` enum (already declared in `recorder.rs`) that does NOT permit raw payload strings — only hashes, counts, ids, and small fixed-vocabulary tags. Test `otel_no_payload_lint.rs` asserts at compile time that the public OTel attribute surface cannot accept `&str` payload fields.
  - Tee test: `otel_tee_smoke.rs` drives a synthetic event stream through `OtelTeeRecorder + SqliteRecorder`, asserts SQLite rows match and an in-memory OTel exporter captured the parallel span tree with no payload attributes.
  - `docs/runbook/observability-otel.md` documents enabling the feature and the env-var contract.
---

# Scope

Add the optional OpenTelemetry tee to `xvision-observability` per the
observability plan's "OpenTelemetry boundary" section. Default builds stay
slim; OTel is opt-in via cargo feature so the production image
(`xvision:latest`) does not ship OTel deps unless deploy targets ask for
them.

The recorder API surface for attributes is hardened so a careless
`attribute.set("prompt", &prompt)` cannot leak full prompts to a remote
collector — this is the plan's hard rule, enforced at the type level.

# Out of scope

- IPC emission of events — `agent-run-observability-ipc-emission`.
- Export CLI / HTTP routes — `agent-run-observability-export-cli`.
- UI — `agent-run-observability-ui`.
- Any change to migration 018 or the `AgentRunRecorder` trait signature.
- Wiring an OTel collector into the deploy stack — runbook only; ops
  decision is a separate ticket.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/agent-run-observability-otel-bridge status
git -C .worktrees/agent-run-observability-otel-bridge log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/agent-run-observability-otel-bridge \
  -b task/agent-run-observability-otel-bridge origin/main
```

# Notes

Append checkpoints / PR links below.
