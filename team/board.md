# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 — conductor pass + intake-followups
> sweep. Three new waves decomposed from open intakes:
> `memory-safety-and-observability` (3 contracts), `eval-honesty-tail`
> (8 contracts, 1 deferred behind a spec), `trace-dock-emitters`
> (1 contract). Eval-traces audit intake fully archived — all 11
> F-items shipped during prior waves (see archive note). Stale
> worktrees swept (18 inside `.worktrees/`, 8 external).

V2 work (V2A onboarding + docs, V2B-V4 roadmap) has its own board:
`team/board-v2.md`.

## Active

### harness-observability-tail-2026-05-21 — F-5 recovery state machine, phase 2

Phase 1 (`harness-recovery-state-machine`, typed dispatcher + repeated-tool
block list) merged 2026-05-21 as PR #499. The three phase-2 contracts
implement the audit's per-class recovery policies on top of that scaffold.
Both unblocked phase-2 tracks (`malformed-json`, `context-overflow`) can
proceed in parallel; `schema-missing-field` is sequentially blocked behind
`malformed-json` because both edit `eval/executor/{paper,backtest}.rs` and
`trader_output.rs`.

- [harness-recovery-malformed-json](contracts/harness-recovery-malformed-json.md) — **P2 integration** · ready · phase 2a: repair-prompt retry on TraderInvalidJson / TraderTruncated (paper + backtest seam)
- [harness-recovery-context-overflow](contracts/harness-recovery-context-overflow.md) — **P2 integration** · ready · phase 2c: cheap-model history summarize + retry; adds `FailureClass::ContextOverflow` variant and `agent/summarize.rs` module
- [harness-recovery-schema-missing-field](contracts/harness-recovery-schema-missing-field.md) — **P2 integration** · deferred (depends on `harness-recovery-malformed-json`) · phase 2b: targeted-patch retry on TraderMissingField / TraderInvalidField, with merge-and-reparse

### memory-safety-and-observability-2026-05-22 — V2D follow-up safety net + provenance

Decomposed 2026-05-22 from
`team/intake/2026-05-21-memory-safety-and-observability.md`. Three
tracks promoted from V2D's deferred list. V2D foundation already
shipped (commit `81007d1`); follow-ups land on top.

- [memory-forget-undo-snapshot](contracts/memory-forget-undo-snapshot.md) — **P1 leaf** · ready · soft-delete + 14-day grace + `xvn memory undo-forget` (no migration; `xvision-memory` crate owns its own schema)
- [memory-provenance-in-decisions-trace](contracts/memory-provenance-in-decisions-trace.md) — **P1 foundation** · ready · thread `decision_id` through `MemoryRecorder::recall`; events table carries `(run_id, decision_id, memory_item_id)`
- [memory-aware-eval-findings](contracts/memory-aware-eval-findings.md) — **P2 leaf** · deferred (depends on `memory-provenance-in-decisions-trace`) · per-decision finding extractor that names memory items behind bad/good outcomes

### eval-honesty-tail-2026-05-22 — F41 sub-tracks (8) from the eval-honesty intake

Tier-0 of the eval-honesty wave shipped 2026-05-21 (#448–#452, #463, #464).
This decomposition opens the remaining 8 sub-tracks. The seven leaf
contracts can proceed in parallel modulo file-share conflicts noted
on each contract; `agent-graph-composition` is the lone foundation
item and is **deferred** pending a capability-first spec (see notes
on the contract; converges with board-v2.md's research-needed item).

- [trader-noop-skip](contracts/trader-noop-skip.md) — **P2 leaf** · ready · skip LLM call when zero legal actions; per-slot opt-out; default ON
- [strategy-model-attestation-only](contracts/strategy-model-attestation-only.md) — **P2 leaf** · ready · demote `required_models` / `model_requirement` to informational `attested_with`
- [strategy-slot-prompt-resolution](contracts/strategy-slot-prompt-resolution.md) — **P2 leaf** · ready · resolve `trader_slot.prompt` (remove or formalize as override)
- [indicator-tool-wiring](contracts/indicator-tool-wiring.md) — **P2 leaf** · ready · wire `indicator_panel` tool through to trader slot (today `tools: []`)
- [bar-history-limit-surface](contracts/bar-history-limit-surface.md) — **P3 leaf** · ready · surface `AgentSlot.bar_history_limit` in agent editor (runner side shipped #372)
- [risk-sees-conviction](contracts/risk-sees-conviction.md) — **P2 leaf** · ready · add `TraderDecision.conviction: f32`; risk gate reads it; never enforces
- [eval-token-efficiency-tail](contracts/eval-token-efficiency-tail.md) — **P2 leaf** · ready · per-provider `max_tokens` defaults + optional delta-briefing mode (PR #372 covered Anthropic cache + bar_history_limit)
- [agent-graph-composition](contracts/agent-graph-composition.md) — **P1 foundation** · **deferred — needs spec** · per-kind I/O contracts, Filter granularity, graph short-circuit; converges with board-v2 "Capability-first agent model" item

### trace-dock-emitters-2026-05-22 — F43 fill in tool_calls / events / spans / supervisor_notes

Single integration contract decomposing the five sub-items from
F43 in `FOLLOWUPS.md`. Same emit sites and the unified observability
writer surface — bundling reduces coordination overhead.

- [trace-dock-emitters](contracts/trace-dock-emitters.md) — **P2 integration** · ready · 5 sub-items: tool_calls emitters, events writer + lifecycle events, supervisor_notes broadening, per-decision spans, checkpoints/approvals/sandbox_results design call

## Reserved

Intakes that exist in `team/intake/` but **need spec authoring first**
before contracts can open. Conductor will not freelance these into
contracts without an operator-approved spec:

- **`team/intake/2026-05-19-compare-ab-evaluations.md`** — 10 open-
  ended product asks for the AB-compare surface (live compare for
  in-flight runs, promote/demote arms, per-agent metrics, side-by-side
  traces, statistical confidence, templates, capsule→compare bridge,
  mobile view, shareable charts, strategy-name labels). Needs a
  product-design spec under `docs/superpowers/specs/` before
  decomposition. Gated by FOLLOWUPS F33 (chart rework).
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** —
  P2, explicitly gated on the V2 capability-first agent-model spec
  per the intake itself; resolves as part of that refactor (see
  also `agent-graph-composition` deferred contract).
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** —
  P0 bundle shipped via #425, #428, #429. P1 (#4–#11) and P2 (#13–#15)
  tracks Reserved pending operator confirmation. P1 #12
  (`remote-cli-safe-eval-allowlist`) folded into the now-merged
  `v2b-remote-cli-job-safety` (#447).
- **`docs-search-list-component-adoption`** — P2 optional follow-up
  contract remains `deferred`; only opens if a docs-sidebar audit
  confirms it qualifies as a "list" worth the component migration.

## Recently Closed

### Decomposed 2026-05-22 — new waves opened (this pass)

Intake archiving + new contract decomposition; no PRs yet.

- **eval-traces-end-to-end-audit (intake 2026-05-19)** archived to
  `team/intake/archive/` — verified all 11 F-items shipped during
  earlier waves (F-1 #361, F-2 #347, F-3 #345, F-4 `3d152f9` + #376,
  F-5 `839ebcb`, F-6 #354, F-7 #353, F-8 #372, F-9 `86336fe` + #462,
  F-10 #349, F-11(a)–(e) shipped; F-11(f) re-filed as F43).
- New waves: `memory-safety-and-observability` (3 contracts),
  `eval-honesty-tail` (8 contracts; 1 deferred), `trace-dock-emitters`
  (1 contract). 12 new contracts ready or deferred.

### Merged 2026-05-22 conductor pass — archived under `team/archive/2026-05-22-conductor-pass/`

13 merged or superseded contracts archived (+ 17 orphan status files):

- **qa-chat-rail-2026-05-21 wave (6 contracts, fully merged)** —
  templates-elimination (#481), strategy-template-registry-removal
  (#486), chat-messages-insert-failing (#480),
  wizard-folder-recall-honesty (#488),
  strategies-folder-into-view-toggle (#479),
  memory-into-agents-section (#478).
- **harness-observability-tail phase 1** —
  harness-recovery-state-machine (#499).
- **alpaca-live-eval-2026-05-21 wave** — executor-trait-extraction
  (#487), live-bar-source-alpaca (#489),
  live-eval-launch-and-freeze (#497); executor-refactor
  (superseded).
- **filter-v1 wave** — filter-v1 umbrella + all five stages
  (#485, #491, #495, #496, #492, #493).
- **v2d-followup tail** — v2d-memory-cli-and-api (#460).
- **Loose ends** — container-config-path-papercut (#464),
  seed-scaffolding-cleanup (#463).

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

Implementation sweep (28 contracts archived):

- **Eval honesty wave (tier-0)** — eval-honesty-smell-tests (#448),
  eval-guardrail-log-collapse (#449), eval-provider-attestation
  (#450), eval-provider-preflight (#452 + #468).
- **V2B (security hardening) — foundation complete** —
  v2b-dashboard-auth-boundary (#465), v2b-remote-cli-job-safety
  (#447), v2b-broker-wallet-kill-switch (#466).
- **V2D (agent memory)** — shipped as commit `81007d1`.
- **V2E foundation tail** — eval-trace-surface-foundation (#422).
- **V2F (strategy authoring) — fully complete (six tracks)** — #414,
  #409, #408, #419, #420, #421.
- **CLI operator safety P0** — cli-operator-safety-p0 (#425, #428,
  #429).
- **Docs / lists / metric polish** — multiple tracks (#424, #430,
  #441 + #454, #433/#439/#442/#457, #432, #434).
- **Clawpatch blockers** — clawpatch-engine-test-helpers (#431),
  clawpatch-frontend-components (#438), clawpatch-cli-test-assert
  (no code change).
- **QA Round 4 tail** — paper-eval-inspector-parity (#440),
  scenario-clone-form-structural-fields (#437),
  strategy-require-at-least-one-agent-fixture-migration (#443 +
  #467 + 3 fixture migrations).

### Archived earlier (still on disk)

- **2026-05-20 sweep** — Lists v1 phase 1, backend pagination (#397),
  QA Round 7, QA Round 6, skills refresh. See
  `team/archive/2026-05-20-lists-v1-phase-1/`.

## Stale-info hygiene

The cumulative "Recently Closed" section above is a window, not a log.
Lookups for closed work go to `team/archive/<sweep>/` first. After two
conductor sweeps, an entry rolls off this section.
