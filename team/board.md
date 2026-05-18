# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-18 conductor sync — drift cleanup after PRs
> #275, #280, #281 merged. Open PRs now: #277 (gated, harness), #278
> (agent-cicd-board-schema), #282 (qa-retention), #283 (qa-trace-
> broker-spans). Active WIP (uncommitted, no PR): `qa-decisions-
> position-pnl`. Truly unblocked + unclaimed: zero — every ready
> downstream stacks on an in-flight PR or is gated.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B-V4 roadmap) also has its own board:
`team/board-v2.md`.

## Active

### QA Operator Round 2/3 — remaining

- [qa-retention-prompt-storage-bug](contracts/qa-retention-prompt-storage-bug.md) - leaf - pr-open #282 - P1 — retention-aware prompt/response placeholder copy (no more "hash-only" lie under full_debug). Awaiting review.
- [qa-trace-broker-spans](contracts/qa-trace-broker-spans.md) - integration - pr-open #283 - P2 — emit `broker.call` spans for Buy/Sell/Close/Short submits. Unblocks `agent-error-feedback-self-healing` when it lands.
- [qa-decisions-position-pnl](contracts/qa-decisions-position-pnl.md) - integration - in-progress (worktree WIP, no PR) - P2 — per-row open-positions cell + realized-PnL fill. Worker has uncommitted client-side derivation in `.worktrees/qa-decisions-position-pnl`; status file pending.
- [agent-error-feedback-self-healing](contracts/agent-error-feedback-self-healing.md) - integration - blocked-on #283 - P1 — recoverable broker errors must round-trip to agent as tool-results. Cannot start until `qa-trace-broker-spans` (#283) merges.

### Harness Observability Audit — GATED on image build

All seven findings (F-1..F-7) from `team/intake/2026-05-18-harness-observability-audit.md` are held until operator ships an image build of pre-harness state. PR #277 (F-1) is open + green but **must not merge**. F-7 is additionally gated on F-2 + F-4 once the wave unfreezes.

- [harness-prompt-hash-real-digest](contracts/harness-prompt-hash-real-digest.md) - leaf - blocked (PR #277 held) - F-1 — real SHA-256 prompt_hash + response_hash on model_call spans. Re-open by flipping status back to `pr-open` once the image deploys.

### Agent CI/CD Phase 1 (2026-05-18)

Implements `docs/superpowers/specs/2026-05-18-agent-cicd-control-plane.md`.
Phase-1 closes the worktree + PR-open gap; review routing and deploy are
Phase 2/3 (not contracted yet).

- [agent-cicd-board-schema](contracts/agent-cicd-board-schema.md) - foundation - pr-open #278 - JSON Schema 2020-12 for the task object + GitHub Project v2 setup doc. Blocks the other three.
- [agent-cicd-migrate-board](contracts/agent-cicd-migrate-board.md) - integration - blocked-on #278 - one-time idempotent script: parse `team/board.md` + `team/board-v2.md`, enrich from contracts, create Issues + Project items. Depends on board-schema.
- [agent-cicd-daemon-skeleton](contracts/agent-cicd-daemon-skeleton.md) - foundation - blocked-on #278 - Node/TS daemon at `tools/agent-conductor/` with `start|stop|pause|resume|status|watch|cancel` CLI, three-layer status surface (CLI + state.json + digest), instance identity for multi-repo Hermes, zero-host-repo-references boundary. Phase-1 transitions only. Depends on board-schema.
- [agent-cicd-shadow-run](contracts/agent-cicd-shadow-run.md) - integration - blocked-on the-other-three - run daemon in shadow against a real 3-5 leaf cohort; ≥90% agreement gate; archived report unblocks live flip. Depends on the other three.
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
- **QA operator round 3** — `wizard-scenario-create-tool-repair` (#272), `trader-output-action-case-insensitive` (#268), `chat-rail-strategy-list-refresh` (#270), `ui-scrollbars-always-visible` (#271), `scenario-bars-estimate-ui` (#269), plus the related `fix-streaming-legacy-fallback` (#267), and `wizard-strategy-template-optional` (#275).
- **V2A onboarding** — `v2a-driver-tour` (#258), `v2a-in-app-docs` (#281). V2A wave closed.
- **QA Round 3 chat polish** — `chat-history-auto-title` (#280).
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
