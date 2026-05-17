# OpenTelemetry tee — operator runbook

The agent-run observability stack writes a canonical SQLite ledger
(`agent_runs`, `spans`, `model_calls`, `tool_calls`, ...) for every
run. An optional **OpenTelemetry tee** mirrors each recorder call as
an OTel span and ships it to a configured OTLP collector (Jaeger,
Tempo, Honeycomb, etc.) so traces can be inspected in standard tools.

The tee is **off by default**. The production `xvision:latest` image
does not ship OTel dependencies unless built with the `otel` cargo
feature on the `xvision-observability` crate.

---

## What gets exported

| Data                  | SQLite | OTel                       |
|-----------------------|--------|----------------------------|
| run id                | yes    | yes (`xvision.run.id`)     |
| span hierarchy        | yes    | yes                        |
| span kind / status    | yes    | yes                        |
| token count           | yes    | yes (`xvision.model.*`)    |
| cost                  | yes    | no (SQLite only)           |
| prompt / response hash| yes    | yes (`*_hash` attributes)  |
| tool input hash       | yes    | yes (attribute)            |
| tool exit code        | yes    | yes (attribute)            |
| approval flag         | yes    | yes (attribute)            |
| full prompt           | gated  | **no, ever**               |
| full tool payload     | gated  | **no, ever**               |
| replay checkpoint     | yes    | id only (no payload)       |
| supervisor note text  | yes    | role + severity only       |

> **Hard rule.** Full prompts, full tool inputs, and full tool outputs
> never leave the local SQLite / blob store via OTel. OTel collectors
> are commonly remote; payload-string export is rejected at the type
> level in `crates/xvision-observability/src/otel.rs` and asserted by
> `tests/otel_no_payload_lint.rs`.

If you need full-prompt visibility for debugging, use the canonical
SQLite ledger (`xvn observe run <run_id>`) — never reach for OTel for
this.

---

## Building with the OTel feature

```bash
cargo build -p xvision-observability --features otel
cargo build -p xvision-engine        --features otel   # downstream consumer
```

For the production Docker image, set the build arg / cargo profile
that flips `otel = true` in the engine crate's feature list (track:
`agent-run-observability-otel-bridge` ships the recorder feature;
the engine integration ticket toggles it at deploy time).

`cargo test -p xvision-observability --no-default-features` and
`cargo test -p xvision-observability --features otel` are both required
to pass — CI runs both.

---

## Environment variables

The tee honors the standard OTel SDK contract:

| Variable                       | Meaning                                  | Default                  |
|--------------------------------|------------------------------------------|--------------------------|
| `OTEL_EXPORTER_OTLP_ENDPOINT`  | OTLP gRPC endpoint                       | `http://localhost:4317`  |
| `OTEL_SERVICE_NAME`            | Reported as `service.name` resource attr | `xvision`                |
| `OTEL_RESOURCE_ATTRIBUTES`     | Extra `key=value,key=value` resource attrs| *(none)*                 |

Example:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.internal:4317
export OTEL_SERVICE_NAME=xvision-prod
export OTEL_RESOURCE_ATTRIBUTES=deployment.environment=prod,host.name=$(hostname)
xvn serve
```

---

## Joining OTel traces back to SQLite rows

When the OTel feature is on, `agent_runs.otel_trace_id` and
`spans.otel_trace_id` / `spans.otel_span_id` are populated on every
recorder write. The producer (Phase B `xvision-agent-client` IPC
handler) stamps these by calling
`xvision_observability::OtelIds::from_current()` immediately before
publishing a `SpanStarted` event. The recorder then writes them
through to the canonical row.

This means: given an OTel trace id from Jaeger / Tempo, you can
`SELECT * FROM agent_runs WHERE otel_trace_id = ?` to find the
matching local run, and vice versa.

When the feature is OFF, those columns are left NULL.

---

## Wiring the tee at process start

```rust
use std::sync::Arc;
use tracing_subscriber::prelude::*;
use xvision_observability::{
    init_otel_pipeline, shutdown_otel_pipeline,
    AgentRunRecorder, OtelTeeRecorder, RunEventBus, SqliteRecorder,
};

# async fn boot(pool: sqlx::SqlitePool) -> anyhow::Result<()> {
let otel_layer = init_otel_pipeline()?;
let subscriber = tracing_subscriber::registry().with(otel_layer);
tracing::subscriber::set_global_default(subscriber)?;

let sqlite = Arc::new(SqliteRecorder::new(pool));
let tee: Arc<dyn AgentRunRecorder> =
    Arc::new(OtelTeeRecorder::new(sqlite.clone()));
let _bus = RunEventBus::new(vec![tee]);

// ... run the engine ...

shutdown_otel_pipeline();
# Ok(()) }
```

`shutdown_otel_pipeline()` MUST be called on process exit so the
batch exporter flushes its queue; dropping the provider alone is not
enough.

---

## Disabling the tee

Three options, in increasing order of permanence:

1. **Per-process** — leave the cargo feature on but set
   `observability.otel_enabled = false` in
   `$XVN_HOME/config/observability.toml`. The bus subscribes only
   the `SqliteRecorder`.
2. **Per-build** — drop `--features otel` and rebuild. The OTel
   crates are not linked; `xvision:latest` ships this way by default.
3. **Per-host** — leave the feature on but point
   `OTEL_EXPORTER_OTLP_ENDPOINT` at a sink that drops everything
   (e.g. an unreachable address). Not recommended; the pipeline
   will retry and log warnings.

---

## Troubleshooting

- **No spans reach the collector.** Check that
  `tracing::subscriber::set_global_default` ran exactly once early
  in process boot, before any agent-run work. A per-thread
  `set_default` does NOT propagate to the bus consumer's Tokio task
  and will silently drop every exported span. The smoke test
  (`tests/otel_tee_smoke.rs`) was written specifically because of
  this gotcha.
- **Spans appear in SQLite but `otel_trace_id` columns are NULL.**
  The producer is not stamping ids on `SpanStarted` events. Confirm
  the producer calls `OtelIds::from_current()` from inside the
  active tracing span (after `span.enter()`), not before.
- **CI build fails on the OTel feature.** The default-feature build
  must stay slim; rebuild with `--no-default-features` to confirm
  the failure is otel-specific. If it is, check the
  `opentelemetry` / `opentelemetry-otlp` / `tracing-opentelemetry`
  versions in `crates/xvision-observability/Cargo.toml` — they are
  pinned to a known-compatible matrix (`opentelemetry = 0.21`,
  `opentelemetry-otlp = 0.14`, `tracing-opentelemetry = 0.22`).

---

## References

- Plan: `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`,
  "OpenTelemetry boundary" section.
- Contract: `team/contracts/agent-run-observability-otel-bridge.md`.
- Source: `crates/xvision-observability/src/otel.rs`.
- Tests: `tests/otel_tee_smoke.rs`, `tests/otel_no_payload_lint.rs`.
