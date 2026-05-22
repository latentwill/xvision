# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 (sweep 2) — `bar-history-limit-surface`,
> `harness-recovery-malformed-json`, `memory-forget-undo-snapshot`,
> `strategy-model-attestation-only`, `trader-noop-skip`,
> `risk-sees-conviction`, `live-bar-source-alpaca` all archived after
> merging. 4 PRs still open against contracts in this board
> (`harness-recovery-context-overflow` #513,
> `harness-recovery-schema-missing-field` #516,
> `strategy-slot-prompt-resolution` #515,
> plus the build-fix #514). Wave-2 (memory-provenance, indicator-tool-wiring,
> eval-token-efficiency-tail, trace-dock-emitters) still held — all
> conflict with `agent/execute.rs` that PR #513 is currently editing.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) has its own board:
`team/board-v2.md`.

## Active (open PRs)

### harness-observability-tail-2026-05-21 — F-5 recovery state machine

Wave complete in PR form — all three phase-2 contracts have PRs open
on top of phase 1 (#499, merged 2026-05-21). Phase 2a (#511) already
merged. Phase 2b + 2c are CLEAN and awaiting merge.

- [harness-recovery-context-overflow](contracts/harness-recovery-context-overflow.md) — **P2 integration** · pr-open **#513** · phase 2c: cheap-model history summarize + retry; adds `FailureClass::ContextOverflow` variant and `agent/summarize.rs` module
- [harness-recovery-schema-missing-field](contracts/harness-recovery-schema-missing-field.md) — **P2 integration** · pr-open **#516** · phase 2b: targeted-patch retry on TraderMissingField / TraderInvalidField with merge-and-reparse; disjoint families (no fall-through to MalformedJson)

### eval-honesty-tail-2026-05-22 — F41 tail (one PR open, rest held or deferred)

- [strategy-slot-prompt-resolution](contracts/strategy-slot-prompt-resolution.md) — **P2 leaf** · pr-open **#515** · removed `LLMSlot.prompt` (decision: REMOVE — single consumer in `validate.rs:200`; agent-side `system_prompt` is source of truth post-2026-05-12 refactor)

## Held (waiting for in-flight PRs to merge)

All conflict with `crates/xvision-engine/src/agent/execute.rs` that PR #513
(`harness-recovery-context-overflow`) is currently editing. Will dispatch
once #513 merges.

- [memory-provenance-in-decisions-trace](contracts/memory-provenance-in-decisions-trace.md) — **P1 foundation** · ready (held) · thread `decision_id` through `MemoryRecorder::recall`
- [memory-aware-eval-findings](contracts/memory-aware-eval-findings.md) — **P2 leaf** · deferred (depends on `memory-provenance`) · per-decision finding extractor over memory recall
- [indicator-tool-wiring](contracts/indicator-tool-wiring.md) — **P2 leaf** · ready (held) · actually wire `indicator_panel` tool to trader slot (`tools: []` today)
- [eval-token-efficiency-tail](contracts/eval-token-efficiency-tail.md) — **P2 leaf** · ready (held) · per-provider `max_tokens` defaults + optional delta-briefing mode
- [trace-dock-emitters](contracts/trace-dock-emitters.md) — **P2 integration** · ready (held) · F43: tool_calls + events writer + supervisor_notes broadening + per-decision spans

## Deferred — needs spec

- [agent-graph-composition](contracts/agent-graph-composition.md) — **P1 foundation** · deferred · per-kind I/O contracts, Filter granularity, graph short-circuit. Converges with board-v2's "Capability-first agent model" research-needed item. Spec author dispatched 2026-05-22.
- [docs-search-list-component-adoption](contracts/docs-search-list-component-adoption.md) — **P2 leaf** · deferred · only opens if a docs-sidebar audit confirms list-component fit

## Reserved

Intakes that exist in `team/intake/` but **need spec authoring first**
before contracts can open:

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 open-
  ended product asks for the AB-compare surface. Gated by FOLLOWUPS F33
  (chart rework — PR #501 base now in main).
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** —
  P2, explicitly gated on the capability-first agent-model spec.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** —
  P0 bundle shipped via #425, #428, #429. P1 (#4–#11) and P2 (#13–#15)
  tracks Reserved pending operator confirmation.

## Pre-existing CLI test failures — now contracted as `cli-test-fixture-completion-tail`

See `team/contracts/cli-test-fixture-completion-tail.md` — ready to dispatch, no dependencies, leaf scope.

### Original carryover note

Surfaced 2026-05-22 by the `strategy-slot-prompt-resolution` worker
during full-workspace verification. Predate all wave-1+ work this
session; flagged here so a follow-up contract can pick them up:

- `cargo test -p xvision-cli --test strategy_validate` — 4 failures
  from `--template` flag removal during the template-registry cleanup
  (PR #486 fallout).
- `cargo test -p xvision-cli --test eval_batch_run` — 5 failures from
  legacy `trader_slot`-only strategies being rejected at the eval
  boundary (`strategy-require-at-least-one-agent` fixture migration,
  PR #443/#467 — incomplete on the CLI side).
- `cargo test -p xvision-cli --test experiment_run` — same shape as
  `eval_batch_run`.

Contract authored 2026-05-22 — see `team/contracts/cli-test-fixture-completion-tail.md`.

## Recently Closed

### Merged 2026-05-22 (this session) — archived under `team/archive/2026-05-22-conductor-pass-2/`

7 contracts archived this pass:

- `bar-history-limit-surface` (#505) — surface `AgentSlot.bar_history_limit` in SlotForm
- `harness-recovery-malformed-json` (#511) — F-5 phase 2a, repair-prompt retry
- `memory-forget-undo-snapshot` (#510) — soft-delete + grace + `xvn memory undo-forget`
- `strategy-model-attestation-only` (#508) — demote `required_models`/`model_requirement` to `attested_with`
- `trader-noop-skip` (#506) — skip LLM call on zero-legal-actions (external)
- `risk-sees-conviction` (#507) — expose `TraderDecision.conviction` to risk gate (external)
- `live-bar-source-alpaca` — leftover from earlier wave; merged via PR #489 on 2026-05-21

### Merged 2026-05-22 conductor pass — archived under `team/archive/2026-05-22-conductor-pass/`

13 contracts + 17 orphan status files archived (see prior board snapshot
in archive for details).

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

28 contracts archived (eval honesty wave, V2B/V2D/V2E/V2F tracks,
clawpatch blockers, QA Round 4 tail).

## Stale-info hygiene

The "Recently Closed" section above is a window, not a log. Lookups for
closed work go to `team/archive/<sweep>/` first. After two conductor
sweeps, an entry rolls off this section.
