# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-19 conductor sweep — harness F-2 (#294), F-6
> (#302), and QA Round 5 F-1/F-2/F-3/F-5 (bundled in #316) merged and
> archived to `team/archive/2026-05-19-sweep/`. Agent CI/CD Phase-1
> parked into a handoff doc at
> `docs/superpowers/handoffs/2026-05-19-agent-cicd-phase-1-handoff.md`
> (5 contracts archived). Deferred `q15-tailscale-serve-api-reachability`
> retired (contract archived). Remaining active: QA Round 5 F-4
> (`risk-preset-balanced-min-order-sanity`, still in intake — needs
> broker-config investigation).
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### QA Operator Round 5 (2026-05-19)

Source: [`team/intake/2026-05-19-qa-validate-draft-cadence-false-positive.md`](intake/2026-05-19-qa-validate-draft-cadence-false-positive.md).
Operator session got stuck in a silent `validate_draft` retry loop on
strategy `01KRZ0ZWER9HE2CTYNWT83ESYQ`. F-1/F-2/F-3/F-5 bundled into PR
#316 (merged 2026-05-19). F-4 is the only remaining leaf and still
needs broker-config investigation before a contract is written.

- `risk-preset-balanced-min-order-sanity` - integration - intake - P2 — F-4: balanced preset triggers 44+ `broker_min_order_size` warnings on ETH paper. Needs broker-config investigation before a contract can be written.

## Reserved

No reserved tracks at this time. New work should enter through an intake doc
or an explicit conductor contract update.

## Recently Closed

Archived 2026-05-19 (conductor sweep — see `team/archive/2026-05-19-sweep/`):

- **Harness observability audit (F-2, F-6)** — `harness-span-attrs-populate` (#294, merged 2026-05-18) and `harness-typed-mechanical-params` (#302, merged 2026-05-18). F-6 added typed `MechanicalParams` enum keyed on `manifest.template` + `deny_unknown_fields` on briefing/decision/risk structs; single pre-persist validate seam in `StrategyStore::save`.
- **QA Round 5 F-1/F-2/F-3/F-5** — bundled in PR #316 (`qa-round-5: validate_draft false positive + silent retry loop fixes`, merged 2026-05-19). Cadence parser is unit-token-strict (F-1); chat-rail surfaces `validate_draft` errors inline with no popup (F-2); wizard loop force-ends after 2 same-error retries with a stuck card (F-3); `findings_model_for_provider` picks the right Haiku id per provider kind (F-5).
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
