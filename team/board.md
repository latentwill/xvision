# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-18 conductor sync — third sweep. Harness
> observability wave 5/7 merged today: #277 (F-1), #294 (F-2),
> #296 (F-3, +fix #299), #297 (F-4), #298 (F-5). F-6
> (`harness-typed-mechanical-params`) ready to claim; F-7
> (`trace-dock-simple-advanced-toggle`) in review at PR #300.
> Operator's image-build gate lifted earlier today — no more
> "blocked-on-deploy" tracks in the harness section. Agent CI/CD
> Phase-1 schema-board / migrate / daemon all landed: #278, #290,
> #295; shadow-run remains as the only active phase-1 track.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### Harness Observability Audit (intake `team/intake/2026-05-18-harness-observability-audit.md`)

F-1..F-5 merged. F-6 ready (no dependencies, parallel-safe with F-7). F-7 in review.

- [harness-typed-mechanical-params](contracts/harness-typed-mechanical-params.md) - integration - ready - F-6 — typed `MechanicalParams` enum keyed on `manifest.template` (one variant per canonical template + `Custom(Value)` fallback). Adds `#[serde(deny_unknown_fields)]` to `InternBriefing`, `TraderDecision`, `RiskDecision`, `RiskConfig`/`Limits`/`Stops`, `RiskCaps`. Single pre-persist validate seam in `StrategyStore::save`. No migration; wire format unchanged. Parallel-safe with F-7.
- [trace-dock-simple-advanced-toggle](contracts/trace-dock-simple-advanced-toggle.md) - leaf - pr-open #300 - F-7 — `Simple | Advanced` segmented toggle on both trace surfaces (TraceDock + /agent-runs/<id>). Simple (default) hides `tool.validate_input/output` + `state.transition` and collapses SpanInspector attributes to a one-liner. Recovery spans stay visible in both. Pure frontend.

### Agent CI/CD Phase 1 (2026-05-18, spec `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`)

Schema-board (#278), markdown→Project migrate (#290), and daemon skeleton (#295) all merged. Shadow-run is the last Phase-1 track.

- [agent-cicd-shadow-run](contracts/agent-cicd-shadow-run.md) - integration - ready - run daemon in shadow against a real 3-5 leaf cohort; ≥90% agreement gate; archived report unblocks live flip. Depends on board-schema + migrate + daemon-skeleton (all merged).
- [agent-cicd-extract-package](contracts/agent-cicd-extract-package.md) - integration - deferred - Phase-2 work: extract `tools/agent-conductor/` to standalone npm package + `npx agent-conductor init` scaffolder. Deferred until Phase-1 is live and Phase-2 review-routing has merged.

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) - integration - deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding it to `Active`.
- agent-error-feedback-same-cycle-rerun - integration - deferred follow-up from PR #286 - Re-run the trader within the same decision cycle after a recoverable broker rejection, recording the retry/follow-up turn in the trace rather than waiting for the next bar.
- agent-error-feedback-real-broker-roundtrip-test - integration - deferred follow-up from PR #286 - Add a real-broker or high-fidelity broker-surface integration test for `recoverable_broker_error_round_trips_to_agent`, including the broker span, decision row, feedback injection, and continued run.
- ~~agent-error-feedback-non-broker-errors~~ — folded into `harness-recovery-state-machine` (F-5, Active) 2026-05-18. The recoverable/fatal split now extends to risk/model/data-fetch errors via the typed `FailureClass` dispatcher.

## Reserved

No reserved tracks at this time. New work should enter through an intake doc
or an explicit conductor contract update.

## Recently Closed

Archived 2026-05-18 (harness observability audit, third sweep — see
`team/archive/2026-05-18-harness/`):

- **Harness wave (5/7 merged)** — `harness-prompt-hash-real-digest` (F-1, #277), `harness-span-attrs-populate` (F-2, #294), `harness-prompt-version-field` (F-3, #296 + fix #299), `harness-span-taxonomy-extension` (F-4, #297), `harness-recovery-state-machine` (F-5, #298). F-6 (`harness-typed-mechanical-params`) remains active; F-7 (`trace-dock-simple-advanced-toggle`) in review at #300.

Archived 2026-05-18 (rounds 1/2/3 QA merge wave — see
`team/archive/2026-05-18-qa-rounds/`):

- **Self-healing broker errors** — `agent-error-feedback-self-healing` (#286). The deferred follow-ups under `## Deferred` below (same-cycle rerun, real-broker round-trip test) trace back to this PR.

- **Agent-run observability follow-ups** — `agent-run-observability-blob-fetch-route` (#244), `eval-inspector-header-polish` (#255), `trace-fullscreen-redesign` (#249).
- **Post-Q15 paper trading** — `alpaca-paper-crypto-submit` (#191, older merge, archived alongside the round-2 wave for traceability).
- **QA operator round 2** — `qa-eval-action-lifecycle` (#260), `qa-review-agent-provider-config` (#256), `qa-decisions-30day-count` (#259), `qa-trace-dock-resizable` (#261), `qa-ui-polish-round2` (#264), `qa-budget-cost-precision` (#257). Plus the supporting round-2 prereqs that landed in the same wave: `trace-dock-ux-polish` (#251), `observability-retention-default-full-debug` (#252), `model-call-streaming-text-passthrough` (#253), `settings-trace-retention` (#250).
- **QA operator round 3** — `wizard-scenario-create-tool-repair` (#272), `trader-output-action-case-insensitive` (#268), `chat-rail-strategy-list-refresh` (#270), `ui-scrollbars-always-visible` (#271), `scenario-bars-estimate-ui` (#269), plus the related `fix-streaming-legacy-fallback` (#267), and `wizard-strategy-template-optional` (#275).
- **V2A onboarding** — `v2a-driver-tour` (#258), `v2a-in-app-docs` (#281). V2A wave closed.
- **QA Round 3 chat polish** — `chat-history-auto-title` (#280).
- **QA Round 2/3 second-wave merge** — `qa-retention-prompt-storage-bug` (#282), `qa-trace-broker-spans` (#283), `qa-decisions-position-pnl` (#284).
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
