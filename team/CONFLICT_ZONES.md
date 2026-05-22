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
| `crates/xvision-engine/src/agent/recovery.rs` | `harness-recovery-context-overflow` (#513) + `harness-recovery-schema-missing-field` (#516) — disjoint enum arms; phase 2a (#511) `MalformedJson` arm already merged | harness-observability-tail-2026-05-21 | Released when #513 + #516 merge. |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `harness-recovery-schema-missing-field` (#516) | harness-observability-tail-2026-05-21 | Released when #516 merges. Then `trace-dock-emitters` claims for the F43 event emitters. |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | Same as paper.rs (dual-executor pattern) | harness-observability-tail-2026-05-21 | Same rule. |
| `crates/xvision-engine/src/eval/executor/trader_output.rs` | `harness-recovery-schema-missing-field` (#516) | harness-observability-tail-2026-05-21 | Released when #516 merges. |
| `crates/xvision-engine/src/agent/execute.rs` | `harness-recovery-context-overflow` (#513) | harness-observability-tail-2026-05-21 | Released when #513 merges. Then 4 wave-2 contracts (`memory-provenance`, `indicator-tool-wiring`, `eval-token-efficiency-tail`, `trace-dock-emitters`) can claim disjoint regions in parallel. |
| `crates/xvision-engine/src/agent/llm.rs` | `harness-recovery-context-overflow` (#513) | harness-observability-tail-2026-05-21 | Released when #513 merges. Then `indicator-tool-wiring` + `eval-token-efficiency-tail` claim disjoint regions. |
| `crates/xvision-engine/src/strategies/{manifest,slot,validate,store}.rs` | `strategy-slot-prompt-resolution` (#515) | eval-honesty-tail-2026-05-22 | Released when #515 merges. |
| `crates/xvision-engine/src/authoring.rs` | Same as strategies/* | eval-honesty-tail-2026-05-22 | Same rule. |
| `crates/xvision-observability/src/{sqlite,events,types,lib}.rs` | held: `trace-dock-emitters` + `memory-provenance-in-decisions-trace` — disjoint event-kinds | multiple waves | Dispatch when #513 unblocks the execute.rs co-occupancy. |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
