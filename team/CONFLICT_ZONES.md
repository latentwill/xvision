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
| `frontend/web/package.json` | `v2a-driver-tour` | v2a | Driver.js dependency lands |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | v2a | Tour mount point lands |
| `crates/xvision-dashboard/src/server.rs` | `agent-run-observability-blob-fetch-route`; `q15-tailscale-serve-api-reachability` is deferred | agent-run-observability-followups / q15 | Blob route merges, or revived q15 declares stacking |
| `crates/xvision-dashboard/src/state.rs` | `agent-run-observability-blob-fetch-route`; `q15-tailscale-serve-api-reachability` is deferred | agent-run-observability-followups / q15 | Blob route merges, or revived q15 declares stacking |
| `crates/xvision-observability/src/lib.rs` | `agent-run-observability-blob-fetch-route` | agent-run-observability-followups | Blob route merges |
| `crates/xvision-dashboard/src/routes/agent_runs.rs` | `agent-run-observability-blob-fetch-route` | agent-run-observability-followups | Blob route merges |
| `frontend/web/src/features/agent-runs/SpanInspector.tsx` | `agent-run-observability-blob-fetch-route` | agent-run-observability-followups | Blob route merges |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `alpaca-paper-crypto-submit` | post-q15 | Alpaca paper crypto submit merges |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
