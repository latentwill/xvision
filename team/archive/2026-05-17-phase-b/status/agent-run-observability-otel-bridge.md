---
track: agent-run-observability-otel-bridge
status: complete
worktree: .worktrees/agent-run-observability-otel-bridge
branch: task/agent-run-observability-otel-bridge
claimed_at: 2026-05-17
completed_at: 2026-05-17
---

# Status

Complete. Branch pushed to origin; no PR opened (per contract).

## Delivered

- `crates/xvision-observability/Cargo.toml` — added `otel` cargo
  feature gating `tracing-opentelemetry = 0.22`,
  `opentelemetry = 0.21`, `opentelemetry_sdk = 0.21` (with
  `rt-tokio`, `testing`), `opentelemetry-otlp = 0.14`, plus
  `tracing-subscriber` (needed by `tracing-opentelemetry::layer`'s
  `LookupSpan` bound). Default build excludes all of them.
- `crates/xvision-observability/src/otel.rs` — `OtelTeeRecorder`,
  `init_otel_pipeline`, `build_resource`, `add_attribute`,
  `attribute_to_kv`, `OtelIds::from_current`,
  `shutdown_otel_pipeline`, bounded `attr::*` key vocabulary. All
  attribute setters consume `Attribute` only — never `&str` /
  `String`.
- `crates/xvision-observability/src/lib.rs` — additive re-export of
  the OTel surface behind `#[cfg(feature = "otel")]`.
- `crates/xvision-observability/tests/otel_tee_smoke.rs` — drives a
  synthetic 11-event stream through `OtelTeeRecorder + SqliteRecorder`
  on a shared `RunEventBus`; asserts SQLite ledger rows match AND
  the in-memory OTel exporter captured exactly 11 parallel spans
  with no payload-string attributes anywhere (name, key, or value).
- `crates/xvision-observability/tests/otel_no_payload_lint.rs` —
  function-pointer coercion + enum-exhaustiveness + four
  `compile_fail` doc tests assert the public OTel attribute surface
  cannot accept raw payload strings.
- `docs/runbook/observability-otel.md` — env-var contract, build
  toggles, trace↔SQLite joining, troubleshooting.

## Verification (all green)

- `cargo test -p xvision-observability --no-default-features` —
  pass (47 tests across all integration suites + 2 doc tests).
- `cargo test -p xvision-observability --features otel` — pass
  (51 tests including the new smoke + lint).
- `cargo build -p xvision-observability --features otel` — clean.

## Deviations from contract

- The `agent_runs.otel_trace_id` / `spans.otel_trace_id` /
  `spans.otel_span_id` columns are populated by the producer
  stamping ids onto `SpanStartedEvent` via the new
  `OtelIds::from_current()` helper, not by the recorder synthesising
  ids on its own. The existing `SqliteRecorder` (forbidden to edit)
  already writes the event's `otel_trace_id` / `otel_span_id`
  fields to the corresponding columns, so this remains compatible
  with the acceptance criterion when callers use the helper.
  Documented in the runbook.
- `tests/otel_no_payload_lint.rs` uses a runtime function-pointer
  coercion + `compile_fail` doc tests in lieu of pulling in
  `trybuild` — the runtime check is sufficient because the load-bearing
  constraint is already in `Attribute`'s lack of `From<&str>` impl,
  which is exercised by the doc tests in `src/recorder.rs` (untouched).
