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
| `crates/xvision-engine/src/eval/executor/paper.rs` | sequential: `harness-recovery-malformed-json` (ready) → `harness-recovery-schema-missing-field` (deferred); then `trader-noop-skip` (ready) and `trace-dock-emitters` (ready) | multiple waves | Released when all four contracts merge. Disjoint regions; smaller diff lands first. |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | Same as paper.rs (dual-executor pattern) | multiple waves | Same rule. |
| `crates/xvision-engine/src/eval/executor/trader_output.rs` | `harness-recovery-schema-missing-field` (deferred) | harness-observability-tail-2026-05-21 | Adds `TraderOutputError::problem_fields()` helper. |
| `crates/xvision-engine/src/agent/execute.rs` | shared across 6 ready/deferred tracks (`harness-recovery-context-overflow`, `memory-provenance-in-decisions-trace`, `risk-sees-conviction`, `eval-token-efficiency-tail`, `trace-dock-emitters`, `indicator-tool-wiring`) — disjoint regions | multiple waves | Released when all six merge. `trace-dock-emitters` is the broadest; coordinate via team/queue/ if it claims first. |
| `crates/xvision-engine/src/agent/llm.rs` | `harness-recovery-context-overflow` + `indicator-tool-wiring` + `eval-token-efficiency-tail` — disjoint regions | multiple waves | Released when all three merge. |
| `crates/xvision-engine/src/strategies/{manifest,slot,validate,store}.rs` | sequential: `strategy-model-attestation-only` → `strategy-slot-prompt-resolution` | eval-honesty-tail-2026-05-22 | attestation-only lands first (rename is cleaner); slot-prompt resolves on rebase. |
| `crates/xvision-engine/src/authoring.rs` | Same as strategies/* above | eval-honesty-tail-2026-05-22 | Same rule. |
| `crates/xvision-observability/src/{sqlite,events,types,lib}.rs` | `trace-dock-emitters` (ready, broad) + `memory-provenance-in-decisions-trace` (ready, narrow — `decision_id` on memory_recall payload only) | multiple waves | trace-dock-emitters lands first preferred (broader change); provenance rebases. |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
