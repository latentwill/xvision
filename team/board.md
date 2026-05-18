# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-18 conductor cleanup. Round-2 QA wave decomposed
> from `team/intake/2026-05-18-qa-operator-round-2.md`. Round-1 QA
> tracks (2026-05-17 wave) archived. Current conductor pass claimed by
> this session until commit/push completes.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### Agent-run Observability Follow-ups

- [agent-run-observability-blob-fetch-route](contracts/agent-run-observability-blob-fetch-route.md) - leaf - claimed - add authenticated blob fetch route plus lazy SpanInspector preview for retained prompt/response payload refs.
- [eval-inspector-header-polish](contracts/eval-inspector-header-polish.md) - leaf - ready - uniform action-button widths, drop redundant strategy/scenario id strip, add a stable per-run disambiguator visible in list + detail header.
- [trace-fullscreen-redesign](contracts/trace-fullscreen-redesign.md) - leaf - claimed - replace the pop-out `/agent-runs/:runId` rail+timeline pair with a Logfire-style waterfall column; drop the redundant rail tree.

### Post-Q15 Paper Trading

- [alpaca-paper-crypto-submit](contracts/alpaca-paper-crypto-submit.md) - integration - ready - make Alpaca crypto paper orders non-fatal where the broker rejects bracket/short semantics, and improve broker failure classification.

### QA Operator Round 2 (2026-05-18)

- [qa-eval-action-lifecycle](contracts/qa-eval-action-lifecycle.md) - leaf - ready - fix cancelled-run timer / capsule bleed across routes / retry on cancelled / add delete in inspector. Stacks on eval-inspector-header-polish.
- [qa-retention-prompt-storage-bug](contracts/qa-retention-prompt-storage-bug.md) - leaf - ready - prompts still redacted despite full_debug while responses appear. Root-cause and fix the asymmetry. Depends on observability-retention-default-full-debug.
- [qa-review-agent-provider-config](contracts/qa-review-agent-provider-config.md) - leaf - ready - research-agent hardcodes anthropic provider; degrade gracefully when unconfigured.
- [qa-decisions-30day-count](contracts/qa-decisions-30day-count.md) - integration - ready - 30-bar scenario produces only 29 decisions (off-by-one). Root-cause and pin with parameterized test.
- [qa-trace-broker-spans](contracts/qa-trace-broker-spans.md) - integration - ready - emit Buy/Sell/Close/Short broker calls as trace spans; fixes missing short-sale fill. Stacks on alpaca-paper-crypto-submit.
- [qa-decisions-position-pnl](contracts/qa-decisions-position-pnl.md) - integration - ready - add per-row open-positions cell + realized-PnL fill on close decisions.
- [qa-budget-cost-precision](contracts/qa-budget-cost-precision.md) - leaf - ready - cheap-model per-call costs show $0.0000; add smart formatter + validate prices flow.
- [qa-trace-dock-resizable](contracts/qa-trace-dock-resizable.md) - leaf - ready - drop redundant "Full" button, add resizable dock with persisted height. Stacks on trace-dock-ux-polish.
- [qa-ui-polish-round2](contracts/qa-ui-polish-round2.md) - leaf - ready - bundle: latest-run chart eval name, agents archived delete, dup streaming icon, remove retention warning, restore TradingView chart titles.

### V2A Onboarding

- [v2a-driver-tour](contracts/v2a-driver-tour.md) - leaf - ready - first-run Driver.js tour plus restart affordance.
- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) - leaf - ready - dashboard `/docs` route backed by packaged in-repo docs.

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) - integration - deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding it to `Active`.

## Reserved

No reserved tracks at this time. New work should enter through an intake doc
or an explicit conductor contract update.

## Recently Closed

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
