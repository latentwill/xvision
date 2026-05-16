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
| `frontend/web/src/routes/eval-runs-detail.tsx` | `eval-review-run-detail-ui` | eval-review | Review panel landed |
| `crates/xvision-engine/src/eval/store.rs` | `eval-review-agent-engine` | eval-review | Review insert helpers landed |
| `crates/xvision-dashboard/src/routes/eval/mod.rs` | `eval-review-api-cli` | eval-review | Review routes registered |
| `crates/xvision-cli/src/commands/eval/mod.rs` | `eval-review-api-cli` | eval-review | `xvn eval review` registered |
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
