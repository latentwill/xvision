# Conflict zones — single-writer files

Files where exactly **one** active contract may write at a time, regardless of
ownership. A contract whose `allowed_paths` glob covers a row here must check
"Current claim" before editing.

Conductor: see `team/CONDUCTOR.md`.

## Zones

| Path | Current claim | Wave | Released when |
|---|---|---|---|
| `crates/xvision-engine/migrations/**` | (none) | — | A new migration number reserved in `v1-shipping-plan.md` |
| `Cargo.toml` (workspace) | (none) | — | Crate add/remove proposed via a Foundation contract |
| `frontend/web/package.json` | `v2a-driver-tour` | v2a | Driver.js dep landed |
| `frontend/web/pnpm-lock.yaml` | `v2a-driver-tour` | v2a | Driver.js install committed |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | v2a | Tour mount point landed |
| `crates/xvision-engine/src/eval/store.rs` | (none — released by eval-review-agent-engine #186 and q15-eval-json-export #187 merges) | — | — |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | (none — released by `q15-scenario-warmup-bars` PR #183 merge) | — | — |
| `crates/xvision-engine/src/eval/executor/paper.rs` | (none — released by `q15-scenario-warmup-bars` PR #183 merge) | — | — |
| `crates/xvision-engine/src/eval/dispatcher.rs` | (none — released by `q15-agent-max-tokens-from-model` PR #185 merge) | — | — |
| `crates/xvision-engine/src/eval/trader_output.rs` | (none — released by `q15-agent-max-tokens-from-model` PR #185 merge) | — | — |
| `crates/xvision-dashboard/src/routes/eval/mod.rs` | (none — released by #187 / #188 / #184 merges) | — | — |
| `crates/xvision-cli/src/commands/eval/mod.rs` | (none — released by #187 / #188 merges) | — | — |
| `frontend/web/src/routes/eval-runs-detail.tsx` | (none — released by #190 / #187 / #184 merges) | — | — |
| `crates/xvision-cli/src/json/object_shapes.rs` | (none — released by #187 / #189 merges) | — | — |
| `crates/xvision-dashboard/src/server.rs` | (none — `q15-tailscale-serve-api-reachability` deferred 2026-05-16) | — | — |
| `crates/xvision-dashboard/src/lib.rs` | (none — `q15-tailscale-serve-api-reachability` deferred 2026-05-16) | — | — |
| `frontend/web/vite.config.ts` | (none — `q15-tailscale-serve-api-reachability` deferred 2026-05-16) | — | — |
| `frontend/web/src/api/client.ts` | (none — `q15-tailscale-serve-api-reachability` deferred 2026-05-16) | — | — |
| `crates/xvision-cli/src/commands/mod.rs` | `v2a-example-artifacts` | v2a | `xvn example` registered |
| `team/MANIFEST.md` | conductor | — | Always conductor-only |
| `team/OWNERSHIP.md` | conductor | — | Always conductor-only |
| `team/CONFLICT_ZONES.md` | conductor | — | Always conductor-only |

## Rules

- "Current claim" rows are reset to `(none)` once the owning contract is
  marked `merged`.
- A second contract that needs a zone currently claimed must:
  1. Wait until the claim releases, or
  2. Push a contract update declaring `stacking: declared:<current-owner>`
     and base its branch on the current owner's branch (not `main`).
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
