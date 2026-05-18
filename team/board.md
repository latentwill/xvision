# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-18 conductor sweep #2 — QA round 2/3 tail
> (#275, #280, #282, #283, #284, #286), V2A docs (PR for
> `v2a-in-app-docs`), and harness `prompt_hash`/`response_hash` real
> SHA-256 digest (#277) all merged and archived to
> `team/archive/2026-05-18-sweep-2/`. Harness operator gate cleared
> (pre-harness image deployed). Remaining active: Agent CI/CD Phase-1
> wave (4 ready + 1 deferred).
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### Harness Observability Audit — F-6

F-1/F-2/F-3 from `team/intake/2026-05-18-harness-observability-audit.md`
landed (PRs #277, #294, #296). F-4 (#297) and F-5 (#298) are in PR
review on the conductor branch. F-7 is gated on F-4 merging. F-6 is
the last unclaimed leaf in the wave; it is parallel-safe with the
F-4/F-5 PRs (disjoint files).

- [harness-typed-mechanical-params](contracts/harness-typed-mechanical-params.md) - integration - claimed - F-6 — typed `MechanicalParams` enum keyed on `manifest.template` (one variant per canonical template + `Custom(Value)` fallback). Adds `#[serde(deny_unknown_fields)]` to `InternBriefing`, `TraderDecision`, `RiskDecision`, `RiskConfig`/`Limits`/`Stops`, `RiskCaps`. Single pre-persist validate seam in `StrategyStore::save`. No migration; wire format unchanged. Parallel-safe with F-4 PR #297 and F-5 PR #298.

### Agent CI/CD Phase 1 (2026-05-18)

Implements `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`.
Phase-1 closes the worktree + PR-open gap; review routing and deploy are
Phase 2/3 (not contracted yet).

- [agent-cicd-board-schema](contracts/agent-cicd-board-schema.md) - foundation - ready - JSON Schema 2020-12 for the task object + GitHub Project v2 setup doc. Blocks the other three.
- [agent-cicd-migrate-board](contracts/agent-cicd-migrate-board.md) - integration - ready - one-time idempotent script: parse `team/board.md` + `team/board-v2.md`, enrich from contracts, create Issues + Project items. Depends on board-schema.
- [agent-cicd-daemon-skeleton](contracts/agent-cicd-daemon-skeleton.md) - foundation - ready - Node/TS daemon at `tools/agent-conductor/` with `start|stop|pause|resume|status|watch|cancel` CLI, three-layer status surface (CLI + state.json + digest), instance identity for multi-repo Hermes, zero-host-repo-references boundary. Phase-1 transitions only. Depends on board-schema.
- [agent-cicd-shadow-run](contracts/agent-cicd-shadow-run.md) - integration - ready - run daemon in shadow against a real 3-5 leaf cohort; ≥90% agreement gate; archived report unblocks live flip. Depends on the other three.
- [agent-cicd-extract-package](contracts/agent-cicd-extract-package.md) - integration - deferred - Phase-2 work: extract `tools/agent-conductor/` to standalone npm package + `npx agent-conductor init` scaffolder. Deferred until Phase-1 is live and Phase-2 review-routing has merged.

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) - integration - deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding it to `Active`.

## Reserved

No reserved tracks at this time. New work should enter through an intake doc
or an explicit conductor contract update.

## Recently Closed

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
