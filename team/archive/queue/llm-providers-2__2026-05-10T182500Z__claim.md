---
from: llm-providers-2
to: all
topic: claim
created_at: 2026-05-10T18:25:00Z
ack_required: false
---

# `llm-providers-2` track claimed (Phase 2 — SlotRef + Arm grammar)

Session 3 (continuing the LLM providers thread — Phase 1 merged via PR #14)
takes Phase 2. Worktree `.worktrees/llm-providers-2`, branch
`feature/llm-providers-phase-2`. Plan slice:
[Plan #7](../../docs/superpowers/plans/2026-05-10-llm-providers-and-per-arm-models-plan.md)
**Phase 2 — Tasks 5–8**.

## Scope

- T5 `SlotRef` newtype (`<provider>/<model>` parse + Display) in `xvision-core/src/slot.rs`
- T6 Convert `ArmKind::Trader` from unit variant to struct variant `{ intern: Option<SlotRef>, trader: Option<SlotRef> }` in `xvision-eval/src/ab_compare.rs`
- T7 Extend `parse_arm_spec` to accept `intern=` / `trader=` / `intern_model=` / `trader_model=` colon-key form; reject mutually-exclusive pairs and unknown keys
- T8 `auto_suffix_arm_names` so two `trader_arm` rows with distinct slots end up with distinct `BacktestResult` names; wire into `xvn ab-compare`

## Files this track touches

- `crates/xvision-core/src/slot.rs` (new)
- `crates/xvision-core/src/lib.rs` (add `pub mod slot;`)
- `crates/xvision-eval/src/ab_compare.rs` (modify ArmKind enum + parse_arm_spec; add `auto_suffix_arm_names`)
- `crates/xvision-cli/src/commands/ab_compare.rs` (one-line addition: call `auto_suffix_arm_names` after parse loop)
- `team/MANIFEST.md` + status + this queue file

Zero overlap with active sessions:
- `eval-3c-metrics` (session 1): touching `crates/xvision-engine/src/eval/`, NOT `crates/xvision-eval/src/`
- `frontend-2-home-and-health` (session 2, PR #13 open): `/api/health` probes in `crates/xvision-engine/src/api/health.rs` — different file

## Out of scope (deferred to Phase 3+)

- Phase 3 — `ProviderRegistry` (memoized backends) + `run_ab_compare` per-arm wiring
- Phase 4 — `xvn provider` CLI subcommand
- Phase 5 — UI design lock + migration note

## Why this slice

T6 is a breaking change to the `ArmKind` enum. Best landed as a focused PR
that updates all call sites (4 of them, all in `ab_compare.rs`) atomically.
After this lands, the type plumbing for "backtest one strategy against N
LLMs" is in place — Phase 3 wires the actual resolution.
