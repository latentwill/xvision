# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-19 conductor sweep — harness F-2 (#294), F-6
> (#302), QA Round 5 F-1/F-2/F-3/F-5 (bundled in #316), and QA Round 5
> F-4 (`risk-preset-balanced-min-order-sanity`) all closed. Agent CI/CD
> Phase-1 parked into a handoff doc at
> `docs/superpowers/handoffs/2026-05-19-agent-cicd-phase-1-handoff.md`
> (5 contracts archived). Deferred `q15-tailscale-serve-api-reachability`
> retired. **Active section empty** — next wave should come through intake.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

No active tracks. The QA Round 5 wave is fully closed (F-1/F-2/F-3/F-5
in PR #316, F-4 resolved 2026-05-19). New work enters through an intake
doc.

## Reserved

- **QA operator round 7** — 9 findings in `team/intake/2026-05-20-qa-operator-round-7.md`.
  Clusters:
  - **List wave** — F-2 (search/filter never landed; revisit
    `list-component-spec` direction) + F-3 (recency-default sort) +
    F-4 (page size).
  - **Eval-inspector wave** — F-1 (strategy/scenario/agents top bar)
    + F-6 (capsule short-title click-through) + F-8 (total cost on
    summary) + F-9 (cost-decimal precision audit).
  - **Trace wave** — F-5 (PAYLOAD REF → human-readable labels) +
    F-7 (remove Super button, add Trade button, investigate why
    trade events are missing from the trace).

  F-2 needs an operator direction call (unified component vs. per-page
  quick fix on the eval-runs list) before the conductor decomposes.
- **Skills refresh for new xvn verbs** — planning intake at
  `team/intake/2026-05-20-skills-update-for-new-xvn-verbs.md`. Refresh
  `.claude/skills/xvision-cli`, `xvision-cli-qa`, `xvision-dev`, and
  `cycle-migration` to cover the experiment orchestrator, hypothesis
  flags, scenario regime labels, baseline compare column, 6 new MCP
  peers, and deploy-image preflight + cleanup. Immediate-board work,
  not V2.
- **QA operator round 6** — 3 tracks in `team/intake/2026-05-19-qa-operator-round-6.md`:
  `scenario-form-calendar-whitespace` (P2), `scenario-runs-tab-show-eval-name` (P2),
  `agent-usage-panel-wire-deployed-and-runs` (P1).
- **Standard list component (design handoff)** — planning intake at
  `team/intake/2026-05-19-list-component-design-intake.md`. Needs a spec under
  `docs/superpowers/specs/` before contracts open. Phase-0 track:
  `list-component-spec`.

## Recently Closed

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
