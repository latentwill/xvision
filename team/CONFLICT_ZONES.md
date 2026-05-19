# Conflict zones - single-writer files

Files where exactly one active contract may write at a time, regardless of
ownership. A contract whose `allowed_paths` glob covers a row here must check
`Current claim` before editing.

Conductor: see `team/CONDUCTOR.md`.

## Zones

| Path | Current claim | Wave | Released when |
|---|---|---|---|
| `team/MANIFEST.md` | conductor | process | Always conductor-only |
| `team/board.md` | conductor | process | Always conductor-only |
| `team/OWNERSHIP.md` | conductor | process | Always conductor-only |
| `team/CONFLICT_ZONES.md` | conductor | process | Always conductor-only |
| `crates/xvision-engine/migrations/**` | (none) | - | A new migration number is reserved in the contract |
| `Cargo.toml` (workspace) | (none) | - | Crate add/remove proposed via a foundation contract |
| `frontend/web/package.json` | (none) | - | v2a-driver-tour merged 2026-05-18 |
| `frontend/web/src/routes/index.tsx` | (none) | - | v2a-driver-tour merged 2026-05-18 |
| `crates/xvision-dashboard/src/server.rs` | `strategy-edit-top-level-fields`; `q15-tailscale-serve-api-reachability` is deferred | qa-operator-2026-05-19 / q15 | strategy-edit PR merges, or revived q15 declares stacking |
| `crates/xvision-dashboard/src/state.rs` | (none); `q15-tailscale-serve-api-reachability` is deferred | q15 | q15 revival declares stacking |
| `crates/xvision-observability/src/lib.rs` | (none) | - | agent-run-observability-blob-fetch-route merged via #244 |
| `crates/xvision-dashboard/src/routes/agent_runs.rs` | (none) | - | agent-run-observability-blob-fetch-route merged via #244 |
| `frontend/web/src/features/agent-runs/SpanInspector.tsx` | (none) | - | qa-retention-prompt-storage-bug merged via #282 |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `risk-gate-min-notional` + `eval-broker-error-circuit-breaker` (disjoint regions per Multi-owner Exemptions in OWNERSHIP.md) | qa-operator-2026-05-19 | Both PRs merge |
| `crates/xvision-engine/src/agent/observability.rs` | `harness-payload-blob-write` | qa-operator-2026-05-19 | harness-payload-blob-write PR merges |
| `crates/xvision-engine/src/agent/execute.rs` | `harness-payload-blob-write` | qa-operator-2026-05-19 | harness-payload-blob-write PR merges |
| `crates/xvision-observability/src/events.rs` | `harness-payload-blob-write` | qa-operator-2026-05-19 | harness-payload-blob-write PR merges |
| `crates/xvision-dashboard/src/routes/eval_runs.rs` | `eval-rerun-from-completed` | qa-operator-2026-05-19 | eval-rerun-from-completed PR merges |
| `crates/xvision-dashboard/src/routes/eval/review.rs` | `eval-review-400-diagnose` | qa-operator-2026-05-19 | eval-review-400-diagnose PR merges |
| `frontend/web/src/features/eval-runs/review/ReviewPanel.tsx` | `eval-review-400-diagnose` | qa-operator-2026-05-19 | eval-review-400-diagnose PR merges |
| `frontend/web/src/routes/eval-runs-detail.tsx` | `eval-rerun-from-completed` | qa-operator-2026-05-19 | eval-rerun-from-completed PR merges |
| `frontend/web/src/routes.tsx` | `stale-chunk-import-retry` | qa-operator-2026-05-19 | stale-chunk-import-retry PR merges |
| `frontend/web/src/App.tsx` | `stale-chunk-import-retry` | qa-operator-2026-05-19 | stale-chunk-import-retry PR merges |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
