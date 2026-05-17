# xvision execution board

> One line per active track. Click into the contract for scope, paths,
> verification, and acceptance. This file is conductor-owned; see
> `team/CONDUCTOR.md`.
>
> Last updated: 2026-05-17.
> Previous board: `team/archive/2026-05-16-migration/execution-board-2026-05-13.md`.

V2 work (V2A onboarding + docs, V2B–V4 roadmap) lives on its own board:
**`team/board-v2.md`**.

## Active

- [ux-polish-eval-list-and-snapshot](contracts/ux-polish-eval-list-and-snapshot.md) — leaf · ready · chart snapshot title + eval-list friendly labels + scroll indicator

### qa-2026-05-17 — comprehensive codebase review

Decomposition: `team/intake/2026-05-17-qa-comprehensive-codebase-review.md`.
Source: `qa/2026-05-17-comprehensive-codebase-review.md` (3×P1, 3×P2, 4×P3).

P1 (claim first):

- [qa-execute-slot-cap](contracts/qa-execute-slot-cap.md) — foundation · ready · bound `execute_slot` tool-use loop with iteration cap + typed error
- [qa-agentd-budget-enforcement](contracts/qa-agentd-budget-enforcement.md) — leaf · ready · enforce `budget_limits.max_wall_ms` + token caps in `xvision-agentd`
- [qa-dashboard-auth-hardening](contracts/qa-dashboard-auth-hardening.md) — integration · ready · gate `/api/cli/jobs` + danger routes; argv allowlist; server-side challenge

P2:

- [qa-role-normalization](contracts/qa-role-normalization.md) — leaf · ready · canonicalize `AgentRef.role` at mutation; fix trader-case + whitespace drift (combines findings 5+6+7)

P3:

- [qa-strategy-id-path-safety](contracts/qa-strategy-id-path-safety.md) — leaf · ready · path-safe strategy ID validation in `FilesystemStore`
- [qa-eval-retry-params-override](contracts/qa-eval-retry-params-override.md) — leaf · ready · include `params_override` in retry idempotency predicate
- [qa-chart-hold-marker-zero](contracts/qa-chart-hold-marker-zero.md) — leaf · ready · stop emitting hold markers at price 0.0 on bar-lookup miss

`cline-sdk-wave1-2` (#208) and `observability-review-fixes` (#207) merged
2026-05-17; both archived under
`team/archive/2026-05-17-cline-sdk-merge/`. PR #199 (DRAFT spec) closed as
superseded.

The remaining v1 worker stream is the V2A onboarding leaves on
`team/board-v2.md` (`v2a-driver-tour`, `v2a-in-app-docs`;
`v2a-example-artifacts` merged via #205 on 2026-05-17).

## Immediate start set

Safe to claim right now (no unresolved Foundation dependency):

- `ux-polish-eval-list-and-snapshot` — three independent UI nits in one PR.
- All seven `qa-*` tracks above. P1 tracks (`qa-execute-slot-cap`,
  `qa-agentd-budget-enforcement`, `qa-dashboard-auth-hardening`) take
  priority. `qa-role-normalization` and `qa-execute-slot-cap` both touch
  `crates/xvision-engine/src/agent/` (different files) — coordinate
  rebases. `qa-dashboard-auth-hardening` re-claims the
  `dashboard/src/{server,lib}.rs` single-writer rows (registered below).
- Phase B agent-run-observability contracts — **now unblocked** by the
  #208 merge (Cline SDK migration step 3 complete; `xvision-agent-client`
  crate is on `main`). Contracts in the Reserved section can be promoted
  to Active by the next conductor pass.
- V2A leaves on `team/board-v2.md` (`v2a-driver-tour`, `v2a-in-app-docs`).
  Independent, parallel-safe.

## Reserved (ready to decompose)

Phase B of the agent-run-observability wave. The Cline SDK foundation
landed via #208, so these are unblocked — they just need contract files
written and worktrees created:

- `agent-run-observability-ipc-emission` (foundation) — wires Cline IPC
  events to the `RunEventBus`. **Is step 8 of the Cline migration plan.**
- `agent-run-observability-otel-bridge` (leaf) — `tracing-opentelemetry` +
  OTLP, gated by cargo feature `otel`.
- `agent-run-observability-export-cli` (leaf) — `xvn run inspect <id>`
  produces `xvn_run.json` + `xvn_report.md`; `GET /api/agent-runs/:id`
  routes.
- `agent-run-observability-ui` (leaf) — `/agent-runs/:id` route with agent
  timeline + streaming text. Implementation plan landed at
  `docs/superpowers/plans/2026-05-17-agent-run-observability-ui-implementation-plan.md`
  (design: `docs/superpowers/specs/2026-05-17-agent-run-observability-ui-design.md`).

## Deferred

- [q15-tailscale-serve-api-reachability](contracts/q15-tailscale-serve-api-reachability.md) — integration · deferred 2026-05-16. Mobile/QA over tailnet parked, not archived. Revive by flipping `status:` back to `ready` and re-adding to an Active wave block above.

## Recently closed waves

Archived 2026-05-16:

- **Q4** — all four tracks merged to `main`.
- **Q8** — board tracks landed via #124 / #162 / #164 combined PRs; individual `qa8-*` PRs closed unmerged on purpose.
- **Q9** — all `qa9-*` PRs merged (#131–#161).
- **Q10** — all `qa10-*` PRs merged (#166–#180); chat/runtime recovery via #169 and #170.
- **eval-review data-model** — merged via #176.
- **color-themes-light-dark** — merged via #135.
- **mobile-safari-load** — merged via #147; iPhone Safari follow-up via #181.
- **q15-scenario-granularity-dropdown** — merged via #182; archived under `team/archive/2026-05-16-q15/`.
- **q15-scenario-warmup-bars** — merged via #183; archived under `team/archive/2026-05-16-q15/`.
- **q15-agent-max-tokens-from-model** — merged via #185; archived under `team/archive/2026-05-16-q15/`.
- **q15-eval-json-export** — merged via #187; archived under `team/archive/2026-05-16-q15/`.
- **q15-object-json-output** — merged via #189; archived under `team/archive/2026-05-16-q15/`.
- **q15-eval-retry-button** — merged via #184; archived under `team/archive/2026-05-16-q15/`.
- **eval-review-agent-engine** — merged via #186; archived under `team/archive/2026-05-16-eval-review/`.
- **eval-review-api-cli** — merged via #188; archived under `team/archive/2026-05-16-eval-review/`.
- **eval-review-run-detail-ui** — merged via #190; archived under `team/archive/2026-05-16-eval-review/`.
- **alpaca-paper-crypto-submit** — merged via #191; contract retained at `team/contracts/alpaca-paper-crypto-submit.md` for regression context.
- **provider-models-selected-first** — merged via #192; archived under `team/archive/2026-05-16-ux-polish/`.
- **strategy-agent-card-collapse** — merged via #194; archived under `team/archive/2026-05-16-ux-polish/`.
- **strategy-agent-card-collapse-resync** — merged via #196 (fix-forward on #194); archived under `team/archive/2026-05-16-ux-polish/`.

Archived 2026-05-17:

- **cline-sdk-wave1-2** — merged via #208 on 2026-05-17; archived under `team/archive/2026-05-17-cline-sdk-merge/`. Sidecar adapter (`xvision-agentd`) + Rust client crate (`xvision-agent-client`) + session lifecycle (`start_run` / `step` / `end_run`) + tool callback round-trip. Mock-provider path for deterministic CI; production providers use normal SDK plumbing. Licensing baseline (LICENSE / NOTICE / cargo-deny / license-checker / CI workflow) explicitly deferred per direction — F1–F4 in `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md`. Deploy-image runtime check deferred to next Docker-host push. Draft spec PR #199 closed as superseded.
- **observability-review-fixes** — merged via #207 on 2026-05-17; archived under `team/archive/2026-05-17-cline-sdk-merge/`. Event bus now evicts OLDEST non-lifecycle event on overflow (not newest, as the buggy `tokio::sync::mpsc::try_send` had been doing). Lifecycle-critical events (RunStarted/RunFinished/RunInterrupted/SidecarError) never evicted. Two reviewer findings on retention.rs / janitor.rs confirmed already fixed on main pre-merge.
- **agent-run-observability-foundation** — merged via #197 on 2026-05-17; archived under `team/archive/2026-05-17-agent-run-observability/`. Plan at `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md`. Phase A leaves now open for claim; Phase B reserved pending Cline migration.
- **agent-run-observability-schema** — merged via #200 on 2026-05-16; archived under `team/archive/2026-05-17-agent-run-observability/`. New `xvision-observability` crate + migration 018 (10 tables).
- **agent-run-observability-event-bus** + **agent-run-observability-retention-cli** — combined and merged via #204 on 2026-05-17; archived under `team/archive/2026-05-17-agent-run-observability/`. Standalone PRs #202/#203 were closed in favor of the combined PR. Phase A is now feature-complete. Live fix-forward = `observability-review-fixes` (#207).
- **eval-running-animation** — merged via #193 on 2026-05-16; archived under `team/archive/2026-05-17-eval-running-animation/`. Animated "running" status pill across eval surfaces with `prefers-reduced-motion` guard.
- **v2a-example-artifacts** — merged via #205 on 2026-05-17; archived under `team/archive/2026-05-17-v2a/`.
- Stale 2026-05-11 status/queue carry-over (audit-health-tests #66, skill-cli-pp-followups #71) — moved to `team/archive/2026-05-17-stale-may11/`.

Q15 wave closed except the deferred integration track. Eval-review wave fully closed.

See `team/archive/2026-05-16-migration/` for the historical board snapshot.

## V2B+ intake (not yet decomposed)

`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` lists items 4–14
(auth boundary, kill switch, on-chain wallets, autoresearcher, audit). The
conductor decomposes one wave at a time. Do not freelance contracts from
that list without going through intake.
