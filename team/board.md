# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 (sweep 6 + sweep-5 rebase) —
> `indicator-tool-wiring` (#521), Orderly multi-asset expansion (#540),
> CLI test fixture tail (#541), eval model override (#538), and model
> bakeoff (#537) merged in the cascade. Sweep 5 archives
> `eval-token-efficiency-tail` (#528) and consolidates the
> `multi-asset-alpaca-unlock` (#533/#536) archive. Agent-graph Phase B-E
> contracts are authored and deferred behind Phase A (#527). Remaining
> live owned PRs: #527 and #523.

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

### cli-operator-safety-wave-b-2026-05-22 — model-bakeoff cluster

Wave A (#530/#531/#532) and Wave B #5/#6 (#538/#537) merged
2026-05-22. The clone helper remains as the one unshipped Wave B leaf.

- [cli-strategy-clone-model-override](contracts/cli-strategy-clone-model-override.md) — **P1 leaf** · ready · `xvn strategy clone <id> --provider X --model Y --name N`.

### charts-section-2026-05-23 — Track B of chart-rework spec (4 canvases + foundation)

Track B of `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md`,
folding in the 2026-05-22 Claude design handoff under
`docs/design/trading-charts/XVN.zip`. Adds a `Charts` left-nav section
(between Scenarios and Eval) with four dashboard canvases. Chart 04
(Liquidation Heatmap) is parked as F-CHART-LIQHEAT — not in this wave.

- [charts-section-b0](contracts/charts-section-b0.md) — **P0 foundation** · ready · sidebar entry, `/charts` route topology, `Strategy.color` migration 034, theme-token superset, three fixtures, `/api/v2/charts/dashboards/overview` stub, `/chart-lab/dashboards` tab.
- B1 (`charts-section-b1`) — Dark Minimal Strategy Dashboard → `/charts/overview`. **Contract authored when B0 lands.** Plan: `docs/superpowers/plans/2026-05-23-charts-section-b1-overview-dashboard.md`.
- B2 (`charts-section-b2`) — Comparison AB Scalable → `/charts/compare`. Plan: `docs/superpowers/plans/2026-05-23-charts-section-b2-comparison-ab.md`. **Parallel-safe with B3 after B1 lands** (B2 reuses B1's primitives; B3 doesn't).
- B3 (`charts-section-b3`) — AI Annotation Chart → `/charts/annotated`. Plan: `docs/superpowers/plans/2026-05-23-charts-section-b3-ai-annotation.md`. **Parallel-safe with B1+B2 after B0 lands** (no shared primitives).
- B4 (`charts-section-b4`) — Gradient Warm Hero → `/charts/hero`. Plan: `docs/superpowers/plans/2026-05-23-charts-section-b4-gradient-hero.md`. Depends on B0 + B1.
- B5 — Review checkpoint (no code commitment) — decide whether `/` should redirect/alias to `/charts/hero` after all four canvases are in production. Fires after B4 lands.
- B-rollout — drop the `xvn.chartv2=1` cookie gate for the Charts sidebar entry.
- F-CHART-LIQHEAT — followup, Chart 04 Liquidation Heatmap. Not in this wave; tokens + payload type land in B0 so the followup picks up rework-free.

## Open PRs (in-flight, not yet merged)

- **#527** — Phase A schema (`agent-graph-capability-schema`).
- **#523** — `memory-provenance-in-decisions-trace` (wave-2 dispatch).
- **#512** — `[codex] streamline strategy creation and docs layout` — CLEAN, external.
- **#498** — `fix(trace-dock): hide state.transition stub in Advanced view` — CLEAN, older.

## Reserved (need spec authoring)

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 product asks for AB-compare. Gated by F33 chart rework.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** — folded into capability-first refactor; closes in Phase E.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** — P0 + Wave A shipped; Wave B in flight; P2 (#13–#15) Reserved indefinitely.

## Deferred — operator-gated

- [docs-search-list-component-adoption](contracts/docs-search-list-component-adoption.md) — only opens if docs-sidebar audit confirms list-component fit.

## Recently Closed

### Merged 2026-05-22 (post sweep-6 cascade)

3 owned tracks closed after sweep 6:

- `cli-test-fixture-completion-tail` (#541 — migrated the remaining 9 CLI fixtures; full `cargo test -p xvision-cli` clean)
- `cli-eval-model-override` (#538 — per-launch provider/model override + provider-diagnostics receipt)
- `cli-model-bakeoff` (#537 — `xvn model bakeoff`, migration 035, compare/status support)

### Merged 2026-05-22 (sweep 6) — archived under `team/archive/2026-05-22-conductor-pass-6/`

1 contract archived this pass + 4 contract-less merges noted:

- `indicator-tool-wiring` (#521 — wires `indicator_panel` tool through to the trader slot's LLM dispatch; F41 sub-track)
- **#520** `fix/unbreak-main-provider-catalogs` — build fix, no contract
- **#522** `fix/sqlite-busy-wal-busy-timeout` — DB pool WAL + busy_timeout, no contract
- **#525** `chore/gha-node-24-bump` — F26 GHA Node 24 cutover, no contract
- **#539** `plan: Orderly multi-asset expansion` — plan-only doc, no contract
- **#540** `feat(orderly): multi-asset expansion` — implemented directly from #539's plan, no contract; FOLLOWUPS F44 filed for the market-info refresh follow-on

### Merged 2026-05-22 (sweep 5) — archived under `team/archive/2026-05-22-conductor-pass-5/`

2 contracts archived/consolidated this pass:

- `eval-token-efficiency-tail` (#528 — per-provider `max_tokens` defaults + delta-briefing mode)
- `multi-asset-alpaca-unlock` (#533, F18 — complete cascade: `TraderDecision.asset` required, risk drops asset param, `BacktestConfig::instrument` removed, Alpaca/Orderly route per decision)

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
