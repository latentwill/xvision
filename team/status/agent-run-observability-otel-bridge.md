---
track: agent-run-observability-otel-bridge
status: claimed
worktree: .worktrees/agent-run-observability-otel-bridge
branch: task/agent-run-observability-otel-bridge
claimed_at: 2026-05-17
---

# Status

Claimed. Starting implementation of `otel` cargo feature on
`xvision-observability` per
`team/contracts/agent-run-observability-otel-bridge.md`.

## Plan

1. Add `otel` cargo feature + dependencies to `Cargo.toml`.
2. Create `src/otel.rs` with `OtelTeeRecorder`, env-var-driven tracer init,
   `Attribute`-only attribute helper surface.
3. Re-export from `src/lib.rs` behind `#[cfg(feature = "otel")]`.
4. Smoke test (`tests/otel_tee_smoke.rs`) — synthetic events through tee +
   sqlite, assert SQLite rows AND in-memory OTel exporter span tree, no
   payload-string attributes.
5. Lint test (`tests/otel_no_payload_lint.rs`) — runtime check that
   `Attribute` rejects `&str`/`String` payloads.
6. Runbook `docs/runbook/observability-otel.md`.
7. Verify all three `cargo test`/`build` commands; push branch.
