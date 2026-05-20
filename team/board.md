# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-21 — QA Round 4 decomposition: opened
> `paper-eval-inspector-parity` (P1 integration) and two carve-out
> followups (`strategy-require-at-least-one-agent-fixture-migration`
> P2 leaf; `scenario-clone-form-structural-fields` P2 integration).
> Marked `mcp-eval-run-job-bridge` shipped (commit `11959db`) and
> `trace-capsule-multi-eval-behavior` shipped via implementation
> (#339, the design-spike step was bypassed). Lists v1 phase 2a
> (`list-migrate-eval-runs`) merged via #399 on 2026-05-20; active
> Lists work is now 2b → 2c. Previous sweep: 2026-05-20 conductor
> sweep — Lists v1 phase 1 + QA Round 7 cleanup.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

- **Lists v1 — phase 2** (serial migration tracks; spec Decision 5,
  `docs/superpowers/specs/2026-05-20-standard-list-component.md`):
  - [list-migrate-strategies](contracts/list-migrate-strategies.md) — integration · ready · 2b — migrates `/strategies` to `<ResponsiveListCard>` + `useListState` + `useListUrlState`. Pattern lifts forward from 2a (#399).
  - [list-migrate-decisions-and-tail](contracts/list-migrate-decisions-and-tail.md) — integration · ready · 2c — `/scenarios` + `/agents`, plus final deletion of the transitional `<ListPagination>` JSX primitive. Depends on 2b.

  Sequencing: serial 2b → 2c per spec Decision 5. Phase 3
  (`list-component-density-toggle`) remains deferred.

- **QA Round 4 — outstanding tail** (decomposed from
  `team/intake/2026-05-19-qa-operator-round-4.md`; 8 of 11 original
  tracks shipped via #341, 2 more via #339 and commit `11959db`):
  - [paper-eval-inspector-parity](contracts/paper-eval-inspector-parity.md) — integration · ready · P1 — paper eval inspector lacks PnL summary + buy/sell order rendering; backtest parity is the target. Root-cause first (engine persistence vs frontend loader fork), then fix.
  - [strategy-require-at-least-one-agent-fixture-migration](contracts/strategy-require-at-least-one-agent-fixture-migration.md) — leaf · ready · P2 — followup to #341 commit `3849680`. Migrate ~13 engine fixtures off the legacy `trader_slot` fallback, then delete the fallback branch in `validate_eval_trader_source`.
  - [scenario-clone-form-structural-fields](contracts/scenario-clone-form-structural-fields.md) — integration · ready · P2 — followup to #341 commit `53f3e3f`. Mount the already-lifted `<ScenarioForm>` inside the inline clone accordion on `/scenarios/:id` so operators can override `time_window` / `asset` / `granularity` / `venue` / `warmup_bars` without leaving the page.

  Sequencing: parallel. `strategy-require-at-least-one-agent-fixture-migration`
  and `paper-eval-inspector-parity` share `crates/xvision-engine/src/api/eval.rs`
  in disjoint regions; fixture-migration is smaller and should land first
  to keep `cargo test --workspace` green for the parity track's CI.

## Reserved

_(empty — next decomposition wave should come through intake; see
the V2 board for V2A leaves and V2E contracts already laid down.)_

## Recently Closed

Merged 2026-05-20 (not yet archived):

- **Lists v1 phase 2a** — `list-migrate-eval-runs` (#399). Migrated
  `/eval-runs` to `<ResponsiveListCard>` + `useListState` +
  `useListUrlState`; landed F-2 (search/filter) from QA Round 7.
  Pattern is the reference for 2b/2c.

QA Round 4 status reconciled 2026-05-21 (intake table updated, no archive yet — three tracks still open as `paper-eval-inspector-parity` / `strategy-require-at-least-one-agent-fixture-migration` / `scenario-clone-form-structural-fields`):

- **`mcp-eval-run-job-bridge`** — shipped via commit `11959db`. Synthetic `eval_run_<ULID>` bridge in `crates/xvision-dashboard/src/cli_jobs/eval_run_bridge.rs` resolves to the `eval_runs` registry without dual-writes; `get_cli_job` / `get_cli_job_output` accept the prefix.
- **`trace-capsule-multi-eval-behavior`** — shipped via implementation (#339); the design-spike step was bypassed and the multi-eval capsule was built directly from the operator's HTML mock at `docs/design/Capsule · Multi-Eval.html`.

Archived 2026-05-20 (conductor sweep — see `team/archive/2026-05-20-lists-v1-phase-1/`):

- **Lists v1 phase 1** — `list-component-port-desktop` (#390),
  `list-component-port-mobile` (#395), `list-component-tokens-reconcile`
  (#396), all merged 2026-05-20. Foundation `<ListCard>` / `useListState`
  / `<ListToolbar>` / `<ListActiveChips>` (1a), mobile `<MListCard>` /
  `<MListRow>` / `<MListSheet>` + `CLAUDE.md` no-popups exemption (1b),
  `<ResponsiveListCard>` wrapper + token audit (1c). Backend-pagination
  follow-up `{items, total}` envelope across all four list endpoints
  shipped via #397.
- **QA operator round 7** — all 9 findings shipped without contract
  files (intake-direct PRs). Trace wave F-5/F-7 (#385), List wave
  F-3/F-4 (#386 — recency-first sort + `ListPagination` primitive
  wired across eval-runs/strategies/scenarios/agents), Eval-inspector
  wave F-1/F-6/F-8/F-9 (#387), `decision_idx` populate follow-up
  (#391). F-2 (search/filter) **rolls into phase 2 list migration**
  rather than a separate quick fix.
- **QA operator round 6** — `scenario-form-calendar-whitespace` (P2),
  `scenario-runs-tab-show-eval-name` (P2),
  `agent-usage-panel-wire-deployed-and-runs` (P1) bundled in #360
  (2026-05-19, but archived in this sweep for traceability).
- **Skills refresh** — `xvision-cli`, `xvision-cli-qa`, `xvision-dev`
  refreshed for new xvn verbs in #379. `cycle-migration` explicitly
  punted by operator (narrow migration-authoring skill, not in the
  usage/contribution orientation surface this wave covers). Drift
  prevention shipped as "skills owner" footers.

Archived 2026-05-19 (conductor sweep — see `team/archive/2026-05-19-sweep/`):

- **Harness observability audit (F-2, F-6)** — `harness-span-attrs-populate` (#294, merged 2026-05-18) and `harness-typed-mechanical-params` (#302, merged 2026-05-18). F-6 added typed `MechanicalParams` enum keyed on `manifest.template` + `deny_unknown_fields` on briefing/decision/risk structs; single pre-persist validate seam in `StrategyStore::save`.
- **QA Round 5 F-1/F-2/F-3/F-5** — bundled in PR #316 (`qa-round-5: validate_draft false positive + silent retry loop fixes`, merged 2026-05-19). Cadence parser is unit-token-strict (F-1); chat-rail surfaces `validate_draft` errors inline with no popup (F-2); wizard loop force-ends after 2 same-error retries with a stuck card (F-3); `findings_model_for_provider` picks the right Haiku id per provider kind (F-5).
- **QA Round 5 F-4** — `risk-preset-balanced-min-order-sanity` resolved 2026-05-19. The balanced-preset 44+ `broker_min_order_size` warnings on ETH paper were closed out; no follow-up contract needed.
- **Parked** — `q15-tailscale-serve-api-reachability` retired from the Deferred lane (no operator demand). Contract archived; revive by restoring from `team/archive/2026-05-19-sweep/contracts/`.
- **Parked** — Agent CI/CD Phase-1 (5 contracts) moved into a handoff doc at `docs/superpowers/handoffs/2026-05-19-agent-cicd-phase-1-handoff.md`. Shadow-run gate already passed (`team/archive/agent-cicd-phase-1-shadow/`, 17/17 = 100%); resume by following the handoff's "How to resume" section.

Archived 2026-05-18 (conductor sweep #2 — see
`team/archive/2026-05-18-sweep-2/`):

- **QA Round 2/3 tail** — `wizard-strategy-template-optional` (#275), `qa-retention-prompt-storage-bug` (#282), `qa-trace-broker-spans` (#283), `qa-decisions-position-pnl` (#284), `agent-error-feedback-self-healing` (#286), `chat-history-auto-title` (#280).
- **V2A onboarding** — `v2a-in-app-docs` merged (closing out the V2A onboarding wave).
- **Harness observability audit (F-1)** — `harness-prompt-hash-real-digest` (#277). Replaces synthetic `eval:<run>:<span>` `prompt_hash` with real SHA-256 digest of `(system_prompt, messages, tools)`; `response_hash` now populated. Operator gate cleared (pre-harness image deployed). Unblocks F-3 prompt-version inference.

Archived 2026-05-18 (rounds 1/2/3 QA merge wave — see
`team/archive/2026-05-18-qa-rounds/`):

- **Agent-run observability follow-ups** — `agent-run-observability-blob-fetch-route` (#244), `eval-inspector-header-polish` (#255), `trace-fullscreen-redesign` (#249).
- **Post-Q15 paper trading** — `alpaca-paper-crypto-submit` (#191, older merge, archived alongside the round-2 wave for traceability).
- **QA operator round 2** — `qa-eval-action-lifecycle` (#260), `qa-review-agent-provider-config` (#256), `qa-decisions-30day-count` (#259), `qa-trace-dock-resizable` (#261), `qa-ui-polish-round2` (#264), `qa-budget-cost-precision` (#257). Plus the supporting round-2 prereqs that landed in the same wave: `trace-dock-ux-polish` (#251), `observability-retention-default-full-debug` (#252), `model-call-streaming-text-passthrough` (#253), `settings-trace-retention` (#250).
- **QA operator round 3** — `wizard-scenario-create-tool-repair` (#272), `trader-output-action-case-insensitive` (#268), `chat-rail-strategy-list-refresh` (#270), `ui-scrollbars-always-visible` (#271), `scenario-bars-estimate-ui` (#269), plus the related `fix-streaming-legacy-fallback` (#267).
- **V2A onboarding** — `v2a-driver-tour` (#258). `v2a-in-app-docs` still ready.
- **Closed without merge** — `qa-eval-inspector-buttons-actually-uniform` PR #263 closed 2026-05-18; contract archived under `team/archive/2026-05-18-qa-rounds/contracts/` for reference. Revisit if operator confirms the button uniformity regression returns.

Archived 2026-05-17:

- **Phase B observability** - merged via #224, #225, #226, #227, #234, #235, and #243. Contracts, statuses, and resolved queue notes are under `team/archive/2026-05-17-phase-b/`.
- **QA codebase review wave** - P1/P2/P3 contracts merged and archived under `team/archive/2026-05-17-qa-codebase-review/`.
- **QA operator fix sprint** - merged operator tracks archived under `team/archive/2026-05-17-qa-operator/`, including `qa-eval-observability-wiring` via #242.
- **Mobile UX polish** - merged mobile/eval-list polish archived under `team/archive/2026-05-17-mobile-ux/`.
- **Cline SDK merge follow-ups** - `cline-sdk-wave1-2` (#208) and `observability-review-fixes` (#207) archived under `team/archive/2026-05-17-cline-sdk-merge/`.
- **Agent-run observability Phase A** - foundation/schema/event-bus/retention leaves archived under `team/archive/2026-05-17-agent-run-observability/`.
- **V2A example artifacts** - merged via #205 and archived under `team/archive/2026-05-17-v2a/`.
- **Stale 2026-05-11 carry-over** - moved to `team/archive/2026-05-17-stale-may11/`.

Archived 2026-05-16:

- **Q4, Q8, Q9, Q10, Q15 completed leaves, eval-review, color themes, mobile Safari, UX polish** - see `team/archive/2026-05-16-*` and `team/archive/status/`.

## V2B+ Intake

`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` lists items 4-14
(auth boundary, kill switch, on-chain wallets, autoresearcher, audit). The
conductor decomposes one wave at a time. Do not freelance contracts from that
list without going through intake.
