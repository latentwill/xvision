# Conflict zones - single-writer files

Files where exactly one active contract may write at a time, regardless of
ownership. A contract whose `allowed_paths` glob covers a row here must check
`Current claim` before editing.

Conductor: see `team/CONDUCTOR.md`.

## Zones

| Path | Current claim | Wave | Released when |
|---|---|---|---|
| `team/MANIFEST.md` | conductor | process | Always conductor-only (operator may amend migration registry rows) |
| `team/board.md` | conductor | process | Always conductor-only |
| `team/OWNERSHIP.md` | conductor | process | Always conductor-only |
| `team/CONFLICT_ZONES.md` | conductor | process | Always conductor-only |
| `crates/xvision-engine/migrations/**` | migration 033 reserved for `agent-graph-capability-schema` | agent-graph-2026-05-22 | Released after Phase A merges; next number 034 for whichever wave-2 contract claims first |
| `Cargo.toml` (workspace) | (none) | - | Crate add/remove proposed via a foundation contract |
| `crates/xvision-engine/src/agents/model.rs` | `agent-graph-capability-schema` (additive `capabilities` field); held: `eval-token-efficiency-tail` (disjoint region) | agent-graph-2026-05-22 | Released after Phase A merges. |
| `crates/xvision-engine/src/strategies/agent_ref.rs` | `agent-graph-capability-schema` (additive `activates` + `EdgePredicate` + `PipelineEdge.condition`) | agent-graph-2026-05-22 | Released after Phase A merges. |
| `crates/xvision-engine/src/agent/execute.rs` | held: 4 wave-2 contracts (memory-provenance, indicator-tool-wiring, eval-token-efficiency-tail, trace-dock-emitters) | multiple waves | Open for parallel dispatch — disjoint regions. Whichever lands first holds; others rebase. |
| `crates/xvision-engine/src/agent/llm.rs` | held: indicator-tool-wiring, eval-token-efficiency-tail | multiple waves | Disjoint regions. |
| `crates/xvision-engine/src/agent/pipeline.rs` | held: indicator-tool-wiring (tool-call dispatch), trace-dock-emitters (tool_calls emit) | multiple waves | Disjoint regions. |
| `crates/xvision-engine/src/eval/executor/paper.rs` | held: trace-dock-emitters | trace-dock-emitters-2026-05-22 | Single claim; safe to dispatch. |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | held: trace-dock-emitters | trace-dock-emitters-2026-05-22 | Single claim; safe to dispatch. |
| `crates/xvision-observability/src/{sqlite,events,types,lib}.rs` | held: trace-dock-emitters (broad), memory-provenance-in-decisions-trace (narrow — `decision_id` on memory_recall payload) | multiple waves | trace-dock-emitters lands first preferred; provenance rebases. |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
