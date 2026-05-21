# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-22 — conductor pass. The 2026-05-21
> `qa-chat-rail-2026-05-21` wave fully merged (all six tracks landed;
> see archive below). The harness observability tail wave's phase 1
> (`harness-recovery-state-machine`, PR #499) also merged, unblocking
> the two parallel phase-2 contracts (`harness-recovery-malformed-json`,
> `harness-recovery-context-overflow`); `harness-recovery-schema-missing-field`
> remains sequentially blocked behind `malformed-json`. The
> alpaca-live-eval and filter-v1 waves also fully merged. 13 contracts
> + 17 stale status files archived under
> `team/archive/2026-05-22-conductor-pass/`.

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
  decomposition.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** —
  P2, explicitly gated on the V2 capability-first agent-model spec
  per the intake itself; resolves as part of that refactor.
- **`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`** —
  P0 bundle shipped via #425, #428, #429. P1 (#4–#11) and P2 (#13–#15)
  tracks Reserved pending operator confirmation. P1 #12
  (`remote-cli-safe-eval-allowlist`) folded into the now-merged
  `v2b-remote-cli-job-safety` (#447).
- **`docs-search-list-component-adoption`** — P2 optional follow-up
  contract (`team/contracts/docs-search-list-component-adoption.md`)
  remains `deferred`; only opens if a docs-sidebar audit confirms it
  qualifies as a "list" worth the component migration.

## Recently Closed

### Merged 2026-05-22 conductor pass — archived under `team/archive/2026-05-22-conductor-pass/`

13 merged or superseded contracts archived (+ 17 orphan status files
cleaned up):

- **qa-chat-rail-2026-05-21 wave (6 contracts, fully merged)** —
  `templates-elimination` (#481), `strategy-template-registry-removal`
  (#486), `chat-messages-insert-failing` (#480),
  `wizard-folder-recall-honesty` (#488),
  `strategies-folder-into-view-toggle` (#479),
  `memory-into-agents-section` (#478).
- **harness-observability-tail phase 1** —
  `harness-recovery-state-machine` (#499).
- **alpaca-live-eval-2026-05-21 wave** — `executor-trait-extraction`
  (#487), `live-bar-source-alpaca` (#489),
  `live-eval-launch-and-freeze` (#497); `executor-refactor`
  superseded contract archived.
- **filter-v1 wave** — `filter-v1` umbrella + all five stages
  (#485, #491, #495, #496, #492, #493).
- **v2d-followup tail** — `v2d-memory-cli-and-api` (#460).
- **Loose ends** — `container-config-path-papercut` (#464),
  `seed-scaffolding-cleanup` (#463).
- **Orphan status files** — 17 stale status files from earlier waves
  (2026-05-18/19 QA tail + `clawpatch-v2e-deslop-followup` from
  #445) archived without contract counterparts.

### Merged 2026-05-21 sweep — archived under `team/archive/2026-05-21-conductor-sweep/`

Implementation sweep (28 contracts archived this pass):

- **Eval honesty wave** — `eval-honesty-smell-tests` (#448),
  `eval-guardrail-log-collapse` (#449), `eval-provider-attestation`
  (#450), `eval-provider-preflight` (#452 + follow-up #468).
- **V2B (security hardening) — foundation complete** —
  `v2b-dashboard-auth-boundary` (#465), `v2b-remote-cli-job-safety`
  (#447), `v2b-broker-wallet-kill-switch` (#466). All three V2B
  tracks landed in the same wave.
- **V2D (agent memory)** — single-contract wave shipped as commit
  `81007d1` (agent memory + cortex tier split F+L+T). Follow-ups
  still open as #453 / #455 / #460 / #458 (D5 kills, D2 cross-symbol).
- **V2E foundation tail** — `eval-trace-surface-foundation` (#422)
  rolled out the determinism receipts + cycle_features parquet (V2E
  item 17). Remaining V2E rows already archived 2026-05-21 on
  `team/board-v2.md`.
- **V2F (strategy authoring) — fully complete (six tracks)** —
  `strategies-folder-surface` (#414), `agent-pipeline-template-library-expansion`
  (#409), `wizard-prompt-strategy-folder-and-templates` (#408),
  `strategies-folder-prepopulation` (#419), `strategies-folder-import`
  (#420), `strategy-ideas-tool-surface` (#421).
- **CLI operator safety P0** — `cli-operator-safety-p0` (#425, #428,
  #429): `xvn eval cancel`, engine hard limits, `xvn experiment run`
  scope guardrails.
- **Docs / lists / metric polish — 2026-05-21 intake closed** —
  `max-drawdown-danger-tone` (#424), `list-search-filter-completion-audit`
  (#430), `docs-ui-prototype-alignment` (#441 + #454),
  `list-search-filter-missing-surfaces` (#433, #439, #442, #457 —
  four slices: skills, providers, eval-compare sort, scenarios-detail
  Runs tab). `docs-search-list-component-adoption` stays deferred
  (deferred lane).
- **Docs / agent wiki tail** — `docs-freshness-staleness-guard` (#434),
  `docs-agentd-surface-page` (#432).
- **Clawpatch blockers** — `clawpatch-engine-test-helpers` (#431 —
  B-1 through B-4), `clawpatch-frontend-components` (#438 — B-6
  through B-10, B-11 closed by the MobileDrawer no-popups refactor
  in #451), `clawpatch-cli-test-assert` (B-5 was already satisfied
  by #423; contract closed without code change).
- **QA Round 4 outstanding tail** — `paper-eval-inspector-parity`
  (#440 — recon doc; no source-visible gap),
  `scenario-clone-form-structural-fields` (#437),
  `strategy-require-at-least-one-agent-fixture-migration` (#443
  worked example + #467 fallback removal + 3 fixture migrations).

### Archived earlier (still on disk)

- **2026-05-20 sweep** — Lists v1 phase 1, backend pagination follow-up
  (#397), QA operator round 7, QA Round 6, skills refresh. See
  `team/archive/2026-05-20-lists-v1-phase-1/`.

## Stale-info hygiene

The cumulative "Recently Closed" section above is a window, not a log.
Lookups for closed work go to `team/archive/<sweep>/` first. After two
conductor sweeps, an entry rolls off this section.
