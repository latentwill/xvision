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
| `crates/xvision-engine/src/agent/recovery.rs` | `harness-recovery-malformed-json` (ready) + `harness-recovery-context-overflow` (ready) — disjoint match arms; `harness-recovery-schema-missing-field` (deferred) stacks behind malformed-json | harness-observability-tail-2026-05-21 | Released when all three phase-2 contracts merge. |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `harness-recovery-malformed-json` (ready); then `harness-recovery-schema-missing-field` (deferred) | harness-observability-tail-2026-05-21 | Sequential — each phase-2 trader-output recovery wires through paper + backtest uniformly. |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | `harness-recovery-malformed-json` (ready); then `harness-recovery-schema-missing-field` (deferred) | harness-observability-tail-2026-05-21 | Same dual-executor pattern — repair logic applies uniformly. |
| `crates/xvision-engine/src/eval/executor/trader_output.rs` | `harness-recovery-schema-missing-field` (deferred) | harness-observability-tail-2026-05-21 | Adds `TraderOutputError::problem_fields()` helper for targeted-patch flow. |
| `crates/xvision-engine/src/agent/execute.rs` | `harness-recovery-context-overflow` (ready) | harness-observability-tail-2026-05-21 | Phase-2c adds the summarize-retry loop into the dispatcher-error path. |
| `crates/xvision-engine/src/agent/llm.rs` | `harness-recovery-context-overflow` (ready) | harness-observability-tail-2026-05-21 | May add a typed `ContextOverflow` variant to `OpenAiCompatError`. |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
