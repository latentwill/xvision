# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-21 — V2F decomposition (#406): six tracks
> authored under a new V2F phase (strategies folder + template
> refactor); see V2F active block below. Plus the prior 2026-05-21
> work: final intake-queue reconciliation (every intake now shipped,
> contracted, or Reserved with a stated reason); CLI agent research
> workbench fully shipped (waves A–E); operator follow-on intake
> (`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`)
> opened with the `cli-operator-safety-p0` bundled contract; Docs
> wiki intake (`team/intake/2026-05-20-docs-user-and-agent-wiki.md`)
> reconciled with two new contracts (`docs-agentd-surface-page` P1,
> `docs-freshness-staleness-guard` P3); Clawpatch-blockers wave
> opened (3 bundled contracts covering B-1 through B-11);
> Docs / lists / metric polish wave opened (5 contracts from the
> 2026-05-21 intake); QA Round 4 decomposition followups still open.
> Lists v1 phase 2 fully complete — 2a (#399), 2b (#400), 2c (#403)
> all merged; the `<ListPagination>` JSX primitive is gone and
> #401 cleared the 5 carry-over test failures (suite at 638/638).
> Previous sweep: 2026-05-20 conductor sweep — Lists v1 phase 1 +
> QA Round 7 cleanup.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

- **V2F — strategy authoring & user knowledge** (six tracks; spec at
  `docs/superpowers/plans/2026-05-21-v2f-strategies-folder-and-template-refactor.md`).
  Detailed entries live on `team/board-v2.md`. Wave 1 (independent —
  parallel-safe; all in PR):
  - [strategies-folder-surface](contracts/strategies-folder-surface.md) — foundation · pr-open #414 · gates wave 2 + 3
  - [agent-pipeline-template-library-expansion](contracts/agent-pipeline-template-library-expansion.md) — leaf · pr-open #409
  - [wizard-prompt-strategy-folder-and-templates](contracts/wizard-prompt-strategy-folder-and-templates.md) — leaf · pr-open #408
  Wave 2 (after foundation): `strategies-folder-prepopulation` + `strategies-folder-import` (parallel).
  Wave 3: `strategy-ideas-tool-surface` (after prepopulation).

- **CLI operator safety — 2026-05-20** (P0 bundle merged 2026-05-21 via
  3 slices: #425 (`xvn eval cancel`), #428 (engine hard limits + flags),
  #429 (`xvn experiment run` scope guardrails). P0 closed. P1 #12
  (`remote-cli-safe-eval-allowlist`) folded into
  `team/contracts/v2b-remote-cli-job-safety.md` (V2B). Remaining P1
  (#4–#11) and P2 (#13–#15) items from
  `team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md`
  remain Reserved.

- **Docs user+agent wiki — outstanding gaps** (2 tracks; 14 of 16
  intake tracks already shipped — see
  `team/intake/2026-05-20-docs-user-and-agent-wiki.md` §"Status
  reconciliation — 2026-05-21"):
  - [docs-agentd-surface-page](contracts/docs-agentd-surface-page.md) — leaf · ready · P1 — write `crates/xvision-dashboard/wiki/agentd.md` covering the TypeScript UDS daemon's externally-observable surface (NDJSON event schema, tool-shim registry, session lifecycle). Daemon itself is forbidden — docs-only. May need a spec round-trip if the surface isn't stable.
  - [docs-freshness-staleness-guard](contracts/docs-freshness-staleness-guard.md) — leaf · ready · P3 — CI lint at `scripts/docs-freshness-lint.sh` + workflow that fails when any wiki page exceeds 90 days since `last_reviewed`, or when a new top-level `xvn` verb lands without a same-PR `cli-reference.md` edit.

  Sequencing: parallel. Independent surfaces.

- **Clawpatch blockers — 2026-05-21** (3 bundled tracks, decomposed
  from `team/intake/2026-05-19-clawpatch-blockers.md`; 11 B-findings
  the autonomous loop couldn't close):
  - [clawpatch-engine-test-helpers](contracts/clawpatch-engine-test-helpers.md) — leaf · ready · medium — covers B-1, B-2, B-3 (SQLite in-memory pool single-connection sweep — only `api_eval.rs` still has the naked `SqlitePool::connect(":memory:")` per 2026-05-21 recon) and B-4 (janitor `max_bytes_evicts_oldest_until_under_cap` staggered mtimes).
  - [clawpatch-cli-test-assert](contracts/clawpatch-cli-test-assert.md) — leaf · ready · low — covers B-5 (add one `stdout.is_empty()` assertion in `crates/xvision-cli/tests/eval_export_cli.rs`). 15-minute job.
  - [clawpatch-frontend-components](contracts/clawpatch-frontend-components.md) — leaf · ready · low/medium — covers B-6 (HealthPill test), B-7 (CacheStatusBadge test), B-8 (AgentForm.duplicateSlot `max_tokens: null`), B-9 (WizardPreviewChart `useMemo`), B-10 (SlotForm provider-change model clear), and B-11 (**MobileDrawer focus management — escalation-gated**: clawpatch's recommended `role="dialog"`/`aria-modal` fix conflicts with the CLAUDE.md no-popups rule; the contract requires the worker to either refactor MobileDrawer into a no-focus-trap inline drawer OR get an operator exemption before implementing the focus-trapping fix).

  Sequencing: parallel. All three tracks are independent. The frontend
  bundle could split per-component if a worker prefers; the contract
  allows that via a contract-update PR.

- **Docs / lists / metric polish — 2026-05-21** (5 tracks, decomposed
  from `team/intake/2026-05-21-docs-lists-metric-polish.md`):
  - [docs-ui-prototype-alignment](contracts/docs-ui-prototype-alignment.md) — leaf · ready · P1 — restyle `/docs` to the folio-dark prototype visual language. Behavior preserved (deep links, sidebar filtering, loading/empty/error states); presentation only. Forbidden from touching docs content (owned by `2026-05-20-docs-user-and-agent-wiki.md`).
  - [list-search-filter-completion-audit](contracts/list-search-filter-completion-audit.md) — foundation · ready · P1 — single-deliverable audit doc at `docs/superpowers/audits/2026-05-21-list-surfaces-audit.md` inventorying every list-like surface in the SPA with its current search/filter/sort state. Blocks `list-search-filter-missing-surfaces`.
  - [list-search-filter-missing-surfaces](contracts/list-search-filter-missing-surfaces.md) — integration · blocked (on audit) · P1 — migrates every list surface the audit flags as missing search/filter/sort to the phase-1 list component stack.
  - [max-drawdown-danger-tone](contracts/max-drawdown-danger-tone.md) — leaf · ready · P1 — rewrite `drawdownToneClass` so any non-zero magnitude max DD renders red/danger across eval-runs list, run detail (desktop + mobile), compare table, and home (if applicable). Extract to a shared module and add tests.
  - [docs-search-list-component-adoption](contracts/docs-search-list-component-adoption.md) — leaf · deferred · P2 — optional follow-up to adopt the standard list component search/chip idiom for the docs sidebar. Stays deferred until `docs-ui-prototype-alignment` lands AND the audit confirms docs nav qualifies.

  Sequencing: `list-search-filter-completion-audit` first (foundation; ~1 day).
  `docs-ui-prototype-alignment` and `max-drawdown-danger-tone` are
  parallel-safe with each other and with the audit. The migration
  track (`list-search-filter-missing-surfaces`) flips to `ready` when
  the audit lands. The docs-list-adoption follow-up activates only on
  conductor flip after the prototype alignment merges and the audit
  recommendation supports it.

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
- **`team/intake/2026-05-20-strategies-folder-and-template-refactor.md`**
  — V2F phase seed: user-curated `strategies/` folder, pre-seeded
  agent-pipeline templates, template-optional refactor follow-on.
  Needs operator decision on V2F adoption + spec.
- **`team/intake/2026-05-20-canonical-template-needs-trader.md`** —
  P2, explicitly gated on the V2 capability-first agent-model spec
  per the intake itself; resolves as part of that refactor.

Intakes with **contracts already laid down** in `team/contracts/`
that haven't yet entered the Active block:

- **`team/intake/2026-05-19-eval-accuracy-and-trace-surface.md`** (V2E)
  — 9 contracts under `team/contracts/eval-*` covering trace-surface
  foundation, candle integrity + manifest, per-bar cost arrays,
  volume-share slippage, intra-bar fill ordering, look-ahead prober,
  broker-rule findings, net-of-inference-cost metric, and trace-surface
  prober. See `team/board-v2.md` for V2E sequencing.

P1 (#4–#12) and P2 (#13–#15) tracks from
`team/intake/2026-05-20-cli-operator-safety-and-model-bakeoff.md` are
also Reserved until P0 (the bundled `cli-operator-safety-p0` contract
above) lands.

## Recently Closed

Merged 2026-05-20 → 2026-05-21 (not yet archived):

- **Lists v1 phase 2** — fully landed. `list-migrate-eval-runs`
  (#399, 2a), `list-migrate-strategies` (#400, 2b),
  `list-migrate-decisions-and-tail` (#403, 2c). The transitional
  `<ListPagination>` JSX primitive was deleted in 2c;
  `useServerPagination` hook lifted to its own file. Phase 3
  (`list-component-density-toggle`) remains deferred. Ready to
  archive on the next conductor sweep.
- **Pre-existing test failures cleanup** — `chore: fix 5 pre-existing
  test failures carried across #386/#387/#397/#399` (#401). Three
  were stale test assertions; the other two found a real production
  duplicate-`data-testid` bug on `strategies-detail.tsx` from commit
  `7c7c55a`. Full frontend suite now 638/638.

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
