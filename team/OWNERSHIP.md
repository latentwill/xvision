# File ownership — active wave

Inverse of each active contract's `allowed_paths`. The conductor maintains this
file. A change here is part of the contract review, not a side-edit.

Conductor: see `team/CONDUCTOR.md`.
Active contracts: see `team/board.md` and `team/contracts/`.

## Rule

If your contract's `allowed_paths` glob includes a row's `Path`, you are an
owner of that row this wave. A non-owner touching that path is a scope
violation (PR closed, contract revised, code resubmitted).

Multi-owner rows are explicit. Listed owners are expected to coordinate
through their contracts' `parallel_conflicts`.

## Map

| Path | Owning track(s) | Wave |
|---|---|---|
| `frontend/web/src/api/types.gen/**` | (regenerated; touched by any track that edits a ts-export Rust type) | — |
| `frontend/web/vite.config.ts` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) | q15 |
| `frontend/web/src/api/client.ts` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) | q15 |
| `frontend/web/MOBILE.md` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) | q15 |
| `crates/xvision-dashboard/src/server.rs` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) | q15 |
| `crates/xvision-dashboard/src/lib.rs` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) | q15 |
| `crates/xvision-dashboard/src/state.rs` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) (if Host/Origin allowlist lives here) | q15 |
| `scripts/serve-tailscale.sh` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) (new, optional) | q15 |
| `docs/runbook/tailscale-serve.md` | `q15-tailscale-serve-api-reachability` (deferred 2026-05-16) (new file) | q15 |
| `crates/xvision-engine/migrations/**` | (none — frozen until a new migration is reserved in `v1-shipping-plan.md`) | — |
| `crates/xvision-dashboard/src/routes/docs/**` | `v2a-in-app-docs` | v2a |
| `crates/xvision-cli/src/commands/example/**` | (closed-out: `v2a-example-artifacts`, merged #205) | v2a |
| `crates/xvision-engine/src/strategies/templates.rs` | (closed-out: `v2a-example-artifacts`, merged #205) | v2a |
| `data/examples/**` | (closed-out: `v2a-example-artifacts`, merged #205) | v2a |
| `frontend/web/src/features/onboarding/**` | `v2a-driver-tour` | v2a |
| `frontend/web/src/routes/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/features/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/api/docs.ts` | `v2a-in-app-docs` | v2a |
| `frontend/web/package.json` | `v2a-driver-tour` (adds `driver.js` only) | v2a |
| `frontend/web/src/themes/**` | (closed-out wave: `color-themes-light-dark`) — request specific token additions through a contract update | — |
| `frontend/web/src/components/primitives/Pill.tsx` | (closed-out: `eval-running-animation`) | ux-polish |
| `frontend/web/src/routes/eval-runs.tsx` | `ux-polish-eval-list-and-snapshot` | ux-polish |
| `frontend/web/src/routes/eval-runs.test.tsx` | `ux-polish-eval-list-and-snapshot` | ux-polish |
| `frontend/web/src/routes/eval-runs-detail.tsx` | (closed-out: `eval-running-animation`) | ux-polish |
| `frontend/web/src/routes/eval-compare.tsx` | (closed-out: `eval-running-animation`) | ux-polish |
| `frontend/web/src/routes/home.tsx` | `ux-polish-eval-list-and-snapshot` | ux-polish |
| `frontend/web/src/routes/home.test.tsx` | `ux-polish-eval-list-and-snapshot` | ux-polish |
| `frontend/web/tailwind.config.ts` | (closed-out: `eval-running-animation`) | ux-polish |
| `frontend/web/src/styles/globals.css` | (closed-out: `eval-running-animation`) | ux-polish |
| `docs/superpowers/plans/2026-05-17-agent-run-observability-plan.md` | (closed-out: `agent-run-observability-foundation`, merged #197) | agent-run-observability |
| `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md` | (closed-out: `agent-run-observability-foundation`) | agent-run-observability |
| `team/intake/2026-05-17-agent-run-observability.md` | (closed-out: `agent-run-observability-foundation`) | agent-run-observability |
| `crates/xvision-observability/src/bus.rs` | (closed-out: `observability-review-fixes`, merged #207) | agent-run-observability |
| `crates/xvision-observability/tests/event_bus_drop_oldest.rs` | (closed-out: `observability-review-fixes`, merged #207) | agent-run-observability |
| `crates/xvision-observability/tests/event_bus_synthetic.rs` | (closed-out: `observability-review-fixes`, merged #207) | agent-run-observability |
| `crates/xvision-observability/**` | (closed-out Phase A. Next claimant: a Phase B contract from the Reserved list on `team/board.md` once decomposed.) | agent-run-observability |
| `crates/xvision-engine/migrations/018_agent_run_observability.sql` | (closed-out: `agent-run-observability-schema`) | agent-run-observability |
| `crates/xvision-engine/migrations/018_agent_run_observability.down.sql` | (closed-out: `agent-run-observability-schema`) | agent-run-observability |
| `crates/xvision-cli/src/commands/obs/**` | (closed-out: `agent-run-observability-retention-cli`) | agent-run-observability |
| `xvision-agentd/**` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `crates/xvision-agent-client/**` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `docs/superpowers/specs/2026-05-17-cline-sdk-agent-replacement-design.md` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave1.md` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `docs/superpowers/plans/2026-05-17-cline-sdk-agent-replacement-wave2.md` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `docs/superpowers/research/2026-05-17-cline-sdk-license-audit.md` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `Dockerfile.deploy` | (closed-out: `cline-sdk-wave1-2`, merged #208) | cline-sdk-agent-replacement |
| `crates/xvision-engine/src/agent/execute.rs` | `qa-execute-slot-cap` | qa-2026-05-17 |
| `crates/xvision-engine/src/agent/pipeline.rs` | `qa-role-normalization` | qa-2026-05-17 |
| `crates/xvision-engine/src/strategies/validate.rs` | `qa-role-normalization` | qa-2026-05-17 |
| `crates/xvision-engine/src/strategies/agent_ref.rs` | `qa-role-normalization` | qa-2026-05-17 |
| `crates/xvision-engine/src/eval/executor/backtest.rs` | `qa-role-normalization` | qa-2026-05-17 |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `qa-role-normalization` | qa-2026-05-17 |
| `crates/xvision-engine/src/strategies/store.rs` | `qa-strategy-id-path-safety` | qa-2026-05-17 |
| `crates/xvision-engine/src/strategies/id.rs` | `qa-strategy-id-path-safety` (new file) | qa-2026-05-17 |
| `crates/xvision-engine/src/authoring.rs` | `qa-strategy-id-path-safety` | qa-2026-05-17 |
| `crates/xvision-engine/src/api/strategy.rs` | `qa-strategy-id-path-safety` | qa-2026-05-17 |
| `crates/xvision-engine/src/api/eval.rs` | `qa-eval-retry-params-override` | qa-2026-05-17 |
| `crates/xvision-engine/src/api/chart.rs` | `qa-chart-hold-marker-zero` | qa-2026-05-17 |
| `crates/xvision-dashboard/src/server.rs` | `qa-dashboard-auth-hardening` (re-claimed from deferred q15-tailscale) | qa-2026-05-17 |
| `crates/xvision-dashboard/src/lib.rs` | `qa-dashboard-auth-hardening` (re-claimed from deferred q15-tailscale) | qa-2026-05-17 |
| `crates/xvision-dashboard/src/auth.rs` | `qa-dashboard-auth-hardening` (new file) | qa-2026-05-17 |
| `crates/xvision-dashboard/src/routes/cli.rs` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `crates/xvision-dashboard/src/cli_jobs/runner.rs` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `crates/xvision-dashboard/src/cli_jobs/allowlist.rs` | `qa-dashboard-auth-hardening` (new file) | qa-2026-05-17 |
| `crates/xvision-dashboard/src/routes/settings/danger.rs` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `crates/xvision-engine/src/api/settings/danger.rs` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `frontend/web/src/api/settings.ts` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `frontend/web/src/routes/settings/danger.tsx` | `qa-dashboard-auth-hardening` | qa-2026-05-17 |
| `docs/runbook/dashboard-auth.md` | `qa-dashboard-auth-hardening` (new file) | qa-2026-05-17 |
| `xvision-agentd/src/methods/session.ts` | `qa-agentd-budget-enforcement` | qa-2026-05-17 |
| `xvision-agentd/src/session/store.ts` | `qa-agentd-budget-enforcement` | qa-2026-05-17 |
| `xvision-agentd/src/session/build-agent.ts` | `qa-agentd-budget-enforcement` | qa-2026-05-17 |
| `xvision-agentd/src/session/budget.ts` | `qa-agentd-budget-enforcement` (new file) | qa-2026-05-17 |
| `crates/xvision-agent-client/src/protocol.rs` | `qa-agentd-budget-enforcement` | qa-2026-05-17 |

## Multi-owner exemptions

Rows that may be edited by more than one active contract, with a coordination rule:

| Path | Owners | Coordination rule |
|---|---|---|
| `crates/xvision-cli/src/commands/mod.rs` | (closed-out: `v2a-example-artifacts`, `agent-run-observability-retention-cli`) | Subcommand registration only. One PR at a time. |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | Mount points only; no refactor. |

## Out of scope this wave

Paths with no listed owner are not blocked, but a new track touching them
must add an ownership row in the same PR as the contract.
