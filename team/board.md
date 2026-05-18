# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-18 conductor sweep — rounds 1/2/3 QA merge wave
> archived to `team/archive/2026-05-18-qa-rounds/`. 19 tracks closed
> via PRs #244, #249, #250, #251, #252, #253, #255, #256, #257, #258,
> #259, #260, #261, #264, #267, #268, #269, #270, #271, #272.
> Open PR: `wizard-strategy-template-optional` (#275). Still ready:
> `qa-retention-prompt-storage-bug` (dep `observability-retention-default-full-debug`
> now merged → unblocked), `qa-trace-broker-spans`,
> `qa-decisions-position-pnl`, `agent-error-feedback-self-healing`,
> `chat-history-auto-title`, `v2a-in-app-docs`. New Agent CI/CD Phase-1
> wave (5 contracts) live.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### QA Operator Round 2/3 — remaining

- [wizard-strategy-template-optional](contracts/wizard-strategy-template-optional.md) - integration - pr-open - P1 — wizard `create_strategy` schema now optional `template`, defaults to blank `custom` draft. PR #275 awaiting merge.
- [qa-retention-prompt-storage-bug](contracts/qa-retention-prompt-storage-bug.md) - leaf - ready - P1 — prompts still redacted despite full_debug while responses appear. Dep `observability-retention-default-full-debug` merged via #252, unblocked.
- [qa-trace-broker-spans](contracts/qa-trace-broker-spans.md) - integration - ready - P2 — emit Buy/Sell/Close/Short broker calls as trace spans. Dep `alpaca-paper-crypto-submit` (#191) merged, unblocked.
- [qa-decisions-position-pnl](contracts/qa-decisions-position-pnl.md) - integration - ready - P2 — add per-row open-positions cell + realized-PnL fill on close decisions.
- [agent-error-feedback-self-healing](contracts/agent-error-feedback-self-healing.md) - integration - ready - P1 — recoverable broker errors (`insufficient_funds`, `rate_limited`, …) must round-trip back to the agent as tool-results, not kill the run. Stacks on qa-trace-broker-spans.
- [chat-history-auto-title](contracts/chat-history-auto-title.md) - leaf - ready - P3 — conversation-history list shows only the timestamp. Cheap-model summarize-once on the first response, ChatGPT-style.

### V2A Onboarding

- [v2a-in-app-docs](contracts/v2a-in-app-docs.md) - leaf - ready - dashboard `/docs` route backed by packaged in-repo docs.

### Harness Observability Audit

- [harness-prompt-hash-real-digest](contracts/harness-prompt-hash-real-digest.md) - leaf - claimed - replace eval-only `eval:<run>:<span>` placeholder with a real provider-payload digest; branch `task/harness-prompt-hash-real-digest` exists on origin (no PR yet).

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
