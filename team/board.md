# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 (sweep 4) — `strategy-slot-prompt-resolution`
> (#515) merged 04:57 UTC and is archived this pass. Wave-2 dispatch is
> in flight: `memory-provenance-in-decisions-trace` (#523) and
> `indicator-tool-wiring` (#521) opened as PRs; `eval-token-efficiency-tail`
> and `trace-dock-emitters` still ready but unclaimed. F-5 harness
> recovery state machine, capability-first spec (#518), and the Phase A
> schema contract remain in the picture from sweep 3.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) has its own board:
`team/board-v2.md`.

## Active (ready to dispatch — no file conflicts)

### agent-graph-2026-05-22 — Phase A of capability-first refactor

- [agent-graph-capability-schema](contracts/agent-graph-capability-schema.md) — **P1 foundation** · ready · adds `Capability` enum + `AgentSlot.capabilities` + `AgentRef.activates` + `PipelineEdge.condition` + migration 033. Pure schema/storage — no dispatch logic. Unblocks Phases B–F.

### memory-safety-and-observability-2026-05-22 — V2D follow-up

- [memory-aware-eval-findings](contracts/memory-aware-eval-findings.md) — **P2 leaf** · deferred (depends on `memory-provenance-in-decisions-trace`) · per-decision finding extractor.

### eval-honesty-tail-2026-05-22 — F41 remaining sub-tracks

- [eval-token-efficiency-tail](contracts/eval-token-efficiency-tail.md) — **P2 leaf** · ready · per-provider `max_tokens` defaults + optional delta-briefing mode

### trace-dock-emitters-2026-05-22 — F43

- [trace-dock-emitters](contracts/trace-dock-emitters.md) — **P2 integration** · ready · 5 sub-items: tool_calls emitters, events writer + lifecycle events, supervisor_notes broadening, per-decision spans, design call on checkpoints/approvals/sandbox_results

### cli-test-tech-debt-2026-05-22

- [cli-test-fixture-completion-tail](contracts/cli-test-fixture-completion-tail.md) — **P2 leaf** · ready · migrate 9 failing CLI test fixtures to post-template-registry, post-strategy-fixture-migration shape

## Open PRs (in-flight, not yet merged)

- **#523** — `memory-provenance-in-decisions-trace` — wave-2 dispatch; threads `decision_id` through `MemoryRecorder::recall`. Contract still in `team/contracts/`; archive on merge.
- **#521** — `indicator-tool-wiring` — wave-2 dispatch; wires `indicator_panel` tool through to trader slot. Contract still in `team/contracts/`; archive on merge.
- **#525** — `chore/gha-node-24-bump` — F26: GHA Node 20 → 24 ahead of 2026-06-02 cutover. No contract (CI housekeeping).
- **#520** — `fix/unbreak-main-provider-catalogs` — build fix for `xvn strategy run-inline`. No contract.
- **#522** — `fix/sqlite-busy-wal-busy-timeout` — `xvn.db` pool WAL + busy_timeout. No contract.
- **#512** — `[codex] streamline strategy creation and docs layout` — CLEAN, external.
- **#498** — `fix(trace-dock): hide state.transition stub in Advanced view` — CLEAN, older.

## Reserved (need spec authoring)

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 product asks for AB-compare. Gated by F33 chart rework.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** — folded into capability-first refactor; closes in Phase E.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** — P0 shipped; P1/P2 tracks Reserved pending operator confirmation.

## Deferred — operator-gated

- [docs-search-list-component-adoption](contracts/docs-search-list-component-adoption.md) — only opens if docs-sidebar audit confirms list-component fit.

## Recently Closed

### Merged 2026-05-22 (post-cascade) — archived under `team/archive/2026-05-22-conductor-pass-4/`

1 contract archived this pass:

- `strategy-slot-prompt-resolution` (#515 — removed `LLMSlot.prompt`; agent-side `system_prompt` is the source of truth post-2026-05-12 refactor)

### Merged 2026-05-22 (~04:46–04:51 UTC cascade) — archived under `team/archive/2026-05-22-conductor-pass-3/`

3 contracts archived in sweep 3:

- `harness-recovery-context-overflow` (#513, F-5 phase 2c — context_length_exceeded → summarize-and-retry)
- `harness-recovery-schema-missing-field` (#516, F-5 phase 2b — targeted-patch retry on missing/invalid trader fields)
- `agent-graph-composition` (placeholder superseded by spec PR #518; archived contract points at the spec + 5 phase successors)

Also merged in the cascade but no contract attached (operator/conductor maintenance):
- **#504** `conductor sweep + 3 new waves`
- **#509** `fix(build): activation_mode/filter for 6 Strategy literals`
- **#514** `fix(build): noop_skip on 3 cfg(test) AgentSlot literals`
- **#517** `conductor sweep-2 + cli-test-fixture-completion-tail contract`
- **#518** `spec: capability-first agent model + graph composition`

### Earlier 2026-05-22 sweeps (still on disk)

- `team/archive/2026-05-22-conductor-pass-2/` — 7 contracts archived after the first merge cascade (#505, #508, #510, #511, #506-external, #507-external, live-bar-source-alpaca leftover)
- `team/archive/2026-05-22-conductor-pass/` — 13 contracts + 17 status files archived at the start of the session

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

28 contracts archived (eval honesty wave, V2B/V2D/V2E/V2F tracks, clawpatch blockers, QA Round 4 tail).

## Stale-info hygiene

The "Recently Closed" section above is a window, not a log. Lookups for
closed work go to `team/archive/<sweep>/` first. After two conductor
sweeps, an entry rolls off this section.
