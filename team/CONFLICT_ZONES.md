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
| `frontend/web/package.json` | (none) | - | Released 2026-05-21 — `v2a-driver-tour` merged earlier |
| `frontend/web/src/routes/index.tsx` | (none) | - | Released 2026-05-21 — `v2a-driver-tour` merged earlier |
| `crates/xvision-dashboard/src/server.rs` | (none) | - | Released 2026-05-21 — observability-blob-fetch-route + q15 both closed |
| `crates/xvision-dashboard/src/state.rs` | (none) | - | Released 2026-05-21 |
| `crates/xvision-observability/src/lib.rs` | (none) | - | Released 2026-05-21 |
| `crates/xvision-dashboard/src/routes/agent_runs.rs` | (none) | - | Released 2026-05-21 |
| `frontend/web/src/features/agent-runs/SpanInspector.tsx` | (none) | - | Released 2026-05-21 |
| `crates/xvision-engine/src/eval/executor/paper.rs` | (none) | - | Released 2026-05-21 — `alpaca-paper-crypto-submit` closed |
| `crates/xvision-dashboard/src/wizard_loop.rs` | `templates-elimination` | qa-chat-rail-2026-05-21 | Released when `templates-elimination` merges; `wizard-folder-recall-honesty` then claims it (sequential, NOT stacked). |
| `crates/xvision-dashboard/prompts/wizard.md` | `templates-elimination` | qa-chat-rail-2026-05-21 | Same as above — sequential handoff. |
| `crates/xvision-engine/src/authoring.rs` | `templates-elimination` (wizard-only); then `strategy-template-registry-removal` (engine refactor; deferred) | qa-chat-rail-2026-05-21 | Sequential — narrow scope first, full refactor after. |
| `crates/xvision-engine/src/api/strategy.rs` | `strategy-template-registry-removal` (deferred) | qa-chat-rail-2026-05-21 | Engine follow-up after `templates-elimination` merges. |
| `crates/xvision-engine/src/strategies/manifest.rs` | `strategy-template-registry-removal` (deferred) | qa-chat-rail-2026-05-21 | Engine follow-up. |
| `crates/xvision-engine/src/strategies/mechanical.rs` | `strategy-template-registry-removal` (deferred) | qa-chat-rail-2026-05-21 | `MechanicalParams::from_value` typed-dispatch refactor. |
| `crates/xvision-engine/src/templates/**` | `strategy-template-registry-removal` (deletion; deferred) | qa-chat-rail-2026-05-21 | Directory deleted by follow-up contract. |
| `crates/xvision-engine/src/agents/templates.rs` | **NOT touched** — AgentTemplate (agent-picker), distinct from strategy templates | qa-chat-rail-2026-05-21 | Stays in both contracts. |
| `frontend/web/src/routes.tsx` | shared: `strategies-folder-into-view-toggle` + `memory-into-agents-section` | qa-chat-rail-2026-05-21 | Disjoint blocks (strategies/folder rows vs. memory row). Coordinate via `team/queue/`; later claimant rebases. Released when both contracts merge. |

## Rules

- Reset a row to `(none)` once the owning contract is marked `merged`.
- A second contract that needs a claimed zone must either wait or update its
  contract with `stacking: declared:<current-owner>` and base on that branch.
- Generated/re-export registries (`index.ts`, `mod.rs`, `lib.rs` re-exports)
  are zones by default even if not listed here. Touch only as registration,
  not refactor.
