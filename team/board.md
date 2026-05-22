# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 (sweep 5b) — reconciles with #535 (Wave B
> promotion) and #536 (multi-asset archive). #528 (eval-token-efficiency)
> merged and archived this pass. #520 (build-fix) + #522 (WAL pool) +
> #525 (Node 24) + #532 (Wave A #11) merged in the cascade.
> `cli-test-fixture-completion-tail` is now unblocked (#520 cleared the
> build error). Phase B of agent-graph (`agent-graph-capability-dispatch`)
> contract authored as deferred behind Phase A (#527).

V2 work (V2A onboarding + docs, V2B-V4 roadmap) has its own board:
`team/board-v2.md`.

## Active (ready to dispatch — no file conflicts)

### agent-graph-2026-05-22 — capability-first refactor cascade

- [agent-graph-capability-schema](contracts/agent-graph-capability-schema.md) — **P1 foundation** · in flight (PR #527) · `Capability` enum + `AgentSlot.capabilities` + `AgentRef.activates` + `PipelineEdge.condition` + migration 033.
- [agent-graph-capability-dispatch](contracts/agent-graph-capability-dispatch.md) — **P1 seam** · deferred behind #527 · `dispatch_capability` seam, `AgentOutput` typed sum, `EdgePredicate` evaluator. Lifts both eval executors onto unified seam. Router shipped in v1.
- [agent-graph-filter-capability](contracts/agent-graph-filter-capability.md) — **P1 filter runtime** · deferred behind Phase A + Phase B · LLM Filter dispatcher + `FilterGranularity` runtime (Bar/Minute/Decision) + in-memory signal cache + DSL filter bridge + multi-Filter cardinality knob (`multi_fire_bar_threshold_minutes`, default 30; operator Q3 resolution).
- [agent-graph-unified-recorder](contracts/agent-graph-unified-recorder.md) — **P1 recorder** · deferred behind Phase B · `Recorder` trait + `HarnessRecorder` + `EvalRecorder`; closes F-11(f) structurally so eval-driven runs produce non-empty rows in all 7 recorder tables.
- [agent-graph-template-capabilities](contracts/agent-graph-template-capabilities.md) — **P2 leaf** · deferred behind Phase A · explicit `capabilities` + `activates` on every starter template; flips `validate_draft_succeeds_for_fresh_template` from expected-fail to expected-pass (closes the longest-standing QA carryover, red since 2026-05-20).

### memory-safety-and-observability-2026-05-22 — V2D follow-up

- [memory-aware-eval-findings](contracts/memory-aware-eval-findings.md) — **P2 leaf** · deferred (depends on `memory-provenance-in-decisions-trace`) · per-decision finding extractor.

### cli-test-tech-debt-2026-05-22

- [cli-test-fixture-completion-tail](contracts/cli-test-fixture-completion-tail.md) — **P2 leaf** · **ready (unblocked 2026-05-22 by #520 merge)** · migrate 9 failing CLI test fixtures to post-template-registry, post-strategy-fixture-migration shape.

### cli-operator-safety-wave-b-2026-05-22 — model-bakeoff cluster

Wave A (#530/#531/#532) merged 2026-05-22. Wave B promoted via #535.
Leaves #4 + #5 are in flight; #6 depends on both.

- [cli-eval-model-override](contracts/cli-eval-model-override.md) — **P1 leaf** · in flight (PR #538) · `xvn eval run --provider X --model Y`.
- [cli-strategy-clone-model-override](contracts/cli-strategy-clone-model-override.md) — **P1 leaf** · ready · `xvn strategy clone <id> --provider X --model Y --name N`.
- [cli-model-bakeoff](contracts/cli-model-bakeoff.md) — **P1 integration** · in flight (PR #537) · the headline verb; absorbs intake #7. New migration 035.

## Open PRs (in-flight, not yet merged)

- **#540** — `task/orderly-multi-asset-expansion` — F18 follow-on; drops BTC-only guard, routes per `td.asset`.
- **#538** — `cli-eval-model-override` (Wave B #5).
- **#537** — `cli-model-bakeoff` (Wave B #6, absorbs #7).
- **#534** — sweep-5 (this branch — Phase B contract + #528/#533 archives). Rebased to clear conflict with #536.
- **#527** — Phase A schema (`agent-graph-capability-schema`).
- **#523** — `memory-provenance-in-decisions-trace` (wave-2 dispatch).
- **#521** — `indicator-tool-wiring` (wave-2 dispatch).
- **#512** — `[codex] streamline strategy creation and docs layout` — draft, external.

## Reserved (need spec authoring)

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 product asks for AB-compare. Gated by F33 chart rework.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** — folded into capability-first refactor; closes in Phase E.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** — P0 + Wave A shipped; Wave B in flight; P2 (#13–#15) Reserved indefinitely.

## Deferred — operator-gated

- [docs-search-list-component-adoption](contracts/docs-search-list-component-adoption.md) — only opens if docs-sidebar audit confirms list-component fit.

## Recently Closed

### Merged 2026-05-22 (sweep 5b) — archived under `team/archive/2026-05-22-conductor-pass-5/`

2 contracts archived this pass:

- `eval-token-efficiency-tail` (#528 — per-provider `max_tokens` defaults + delta-briefing mode)
- `multi-asset-alpaca-unlock` (#533, F18 — pre-archived by #536; consolidated here)

### Merged 2026-05-22 (post-cascade) — archived under `team/archive/2026-05-22-conductor-pass-4/`

2 contracts archived:

- `strategy-slot-prompt-resolution` (#515 — removed `LLMSlot.prompt`; agent-side `system_prompt` is the source of truth post-2026-05-12 refactor)
- `trace-dock-emitters` (#524, F43 — filled in `tool_calls`, `events`, `spans`, `supervisor_notes` emitters)

### Merged 2026-05-22 (~04:46–04:51 UTC cascade) — archived under `team/archive/2026-05-22-conductor-pass-3/`

3 contracts archived in sweep 3:

- `harness-recovery-context-overflow` (#513, F-5 phase 2c)
- `harness-recovery-schema-missing-field` (#516, F-5 phase 2b)
- `agent-graph-composition` (placeholder superseded by spec PR #518)

Also merged in the cascade with no contract attached:
- **#504** `conductor sweep + 3 new waves`
- **#509** `fix(build): activation_mode/filter for 6 Strategy literals`
- **#514** `fix(build): noop_skip on 3 cfg(test) AgentSlot literals`
- **#517** `conductor sweep-2 + cli-test-fixture-completion-tail contract`
- **#518** `spec: capability-first agent model + graph composition`
- **#520** `fix(build): provider_catalogs on PipelineInputs (run-inline)`
- **#522** `fix(api): xvn.db pool — WAL + busy_timeout`
- **#525** `chore(ci): GHA Node 20 → 24`
- **#532** `feat(eval): action counts + tokens + wall clock (Wave A #11)`
- **#535** `conductor(wave-b): promote Wave B model-bakeoff cluster`
- **#536** `team: sweep-5 — archive multi-asset-alpaca-unlock`
- **#539** `plan: Orderly multi-asset expansion`

### Earlier 2026-05-22 sweeps (still on disk)

- `team/archive/2026-05-22-conductor-pass-2/` — 7 contracts archived after the first merge cascade.
- `team/archive/2026-05-22-conductor-pass/` — 13 contracts + 17 status files archived at session start.

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

28 contracts archived (eval honesty wave, V2B/V2D/V2E/V2F tracks, clawpatch blockers, QA Round 4 tail).

## Stale-info hygiene

The "Recently Closed" section above is a window, not a log. Lookups for
closed work go to `team/archive/<sweep>/` first. After two conductor
sweeps, an entry rolls off this section.
