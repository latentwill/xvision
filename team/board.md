# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-19 conductor sync — round-4 QA wave decomposed
> + re-evaluated against fresh merges. 6 contracts (4 P1 + 2 P2).
> Highest-leverage P1: `harness-payload-blob-write` (unblocks trace
> dock prompt/response bodies on full_debug — unfinished handoff from
> PR #282 that #277 picked up the hash-half of but dropped the blob-
> write half). `eval-broker-error-circuit-breaker` stays P1 because
> the operator's 2026-05-19 02:33 UTC run still looped despite the
> post-#314 timestamp (deploy lag OR feedback ignored — safety net
> needed). `risk-gate-min-notional` revised P1→P2: #314 (Alpaca
> cost-basis classifier) + #286 (self-healing feedback) now handle
> the critical failure at the classifier+feedback layer; this contract
> is the proactive cleanup. Calendar-picker intake in `## Reserved`.
> Harness wave near closeout: F-1..F-6 all merged (F-6 via #302); F-7
> in review at PR #300. Agent CI/CD Phase-1 closed out (#315 merged).
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### Harness Observability Audit (intake `team/intake/2026-05-18-harness-observability-audit.md`)

F-1..F-6 merged (F-6 via PR #302 on 2026-05-19). F-7 in review at PR #300. Wave near closeout.

- [trace-dock-simple-advanced-toggle](contracts/trace-dock-simple-advanced-toggle.md) - leaf - pr-open #300 - F-7 — `Simple | Advanced` segmented toggle on both trace surfaces (TraceDock + /agent-runs/<id>). Simple (default) hides `tool.validate_input/output` + `state.transition` and collapses SpanInspector attributes to a one-liner. Recovery spans stay visible in both. Pure frontend.

### Agent CI/CD Phase 1 (2026-05-18, spec `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`)

Schema-board (#278), markdown→Project migrate (#290), daemon skeleton (#295), and shadow-run (#315) all merged. Phase-1 closed out — `agent-cicd-extract-package` remains deferred to Phase-2.

- [agent-cicd-extract-package](contracts/agent-cicd-extract-package.md) - integration - deferred - Phase-2 work: extract `tools/agent-conductor/` to standalone npm package + `npx agent-conductor init` scaffolder. Deferred until Phase-1 is live and Phase-2 review-routing has merged.

### QA Operator Round 4 (2026-05-19, intake `team/intake/2026-05-19-qa-operator-round-4.md`)

Six tracks decomposed from operator findings on 2026-05-19. Re-evaluated post-#314 (Alpaca min-notional classifier) + #302 (F-6 merged): `risk-gate-min-notional` dropped from P1→P2 because #314+#286 now handle the critical failure mode at the classifier+feedback layer; `eval-broker-error-circuit-breaker` stays P1 because the operator's 2026-05-19 02:33 UTC run still looped despite the post-#314 timestamp (deploy lag OR feedback-ignored — investigation out of scope for the safety-net track). Highest-leverage P1: `harness-payload-blob-write` (trace dock body capture — unfinished from #282 handoff).

- [harness-payload-blob-write](contracts/harness-payload-blob-write.md) - integration - ready - P1 — wire `BlobStore::write` into `emit_model_call_finished` so `full_debug` trace dock actually shows prompt/response bodies. Unfinished handoff from #282 — #277 shipped real hashes, but the blob-write half was dropped. `BlobStore::write` has zero production callers today.
- [eval-broker-error-circuit-breaker](contracts/eval-broker-error-circuit-breaker.md) - integration - ready - P1 safety net — abort eval run after N=3 consecutive identical `error_class` rejections. Catches the operator's 2026-05-19 loop even though #314+#286 should have prevented it; works for unknown future deterministic broker errors too. Different error classes don't accumulate; success resets the counter.
- [strategy-edit-top-level-fields](contracts/strategy-edit-top-level-fields.md) - integration - ready - P1 — `PATCH /api/strategy/:id` for `display_name` / `plain_summary` / `asset_universe`. Inline-edit UI on `/strategies/:id` per the no-popup rule. Operator can only fix typos by delete-and-recreate today.
- [eval-review-400-diagnose](contracts/eval-review-400-diagnose.md) - integration - ready - P1 investigation→fix — repro the silent 400 on the operator's `01KRXY73XAE2NR65YVKJZ28JBK` review request, identify which Validation branch is firing, then either add a remediation hook (per #256 pattern) or fix the frontend's error-body surfacing. Phase-1 status note required before any code lands.
- [risk-gate-min-notional](contracts/risk-gate-min-notional.md) - integration - ready - P2 (revised from P1) — proactive: new `MinNotional` risk rule vetoes pre-submit when notional < venue minimum, surfaces a clean `BelowVenueMinNotional` veto, and prevents the wasteful broker round-trip. #314+#286 already handle the critical failure mode at the classifier+feedback layer; this is the cleanup pass that also primes the risk crate for other deterministic broker constraints (tick size, lot size).
- [eval-rerun-from-completed](contracts/eval-rerun-from-completed.md) - integration - ready - P2 — widen the retry route from `failed | cancelled` to also accept `completed`. Frontend `canRetry` widens; button label adapts to "Rerun" vs "Retry" with a distinguishing tooltip.
- [stale-chunk-import-retry](contracts/stale-chunk-import-retry.md) - leaf - ready - P2 — catch Vite lazy-import chunk-fetch errors after deploy in an `AppErrorBoundary`; hard-reload once per session with a post-reload toast. Defers the proactive build-id polling approach as a follow-up.

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) - integration - deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding it to `Active`.
- agent-error-feedback-same-cycle-rerun - integration - deferred follow-up from PR #286 - Re-run the trader within the same decision cycle after a recoverable broker rejection, recording the retry/follow-up turn in the trace rather than waiting for the next bar.
- agent-error-feedback-real-broker-roundtrip-test - integration - deferred follow-up from PR #286 - Add a real-broker or high-fidelity broker-surface integration test for `recoverable_broker_error_round_trips_to_agent`, including the broker span, decision row, feedback injection, and continued run.
- ~~agent-error-feedback-non-broker-errors~~ — folded into `harness-recovery-state-machine` (F-5, Active) 2026-05-18. The recoverable/fatal split now extends to risk/model/data-fetch errors via the typed `FailureClass` dispatcher.

## Reserved

Awaiting conductor decomposition:

- `team/intake/2026-05-19-calendar-picker.md` — inline date-range picker
  + canonical-set calendar select on `ScenarioForm.tsx`, implementing the
  component design package at `docs/design/calendar-picker/`. Structural
  fix that lets us delete the Qwen-specific normalizer hacks in
  `wizard_loop.rs::normalize_create_scenario_input`. Component-only
  scope — no page-layout changes.

## Follow-ups / research needed

- **User-configurable review-agent profile** (raised 2026-05-18 from
  operator QA round 2; moved 2026-05-19 from `team/board-v2.md` —
  near-term Settings surface, not a V2-phase roadmap item). The current
  review/research agent profile hardcodes `anthropic` as its provider.
  `qa-review-agent-provider-config` shipped a runtime fallback so review
  still runs on dashboards without Anthropic configured, but the longer
  arc is a Settings → Review Agents UI where the operator picks the
  profile (system prompt, provider, model, memory mode) for the review
  pass. Output before contract: short design note under
  `docs/superpowers/notes/` scoping the Settings surface + which review
  passes are configurable (results review only, or also research /
  autoresearcher passes).

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
