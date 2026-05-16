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
| `crates/xvision-cli/src/commands/scenario/**` | `q15-object-json-output` (get only) | q15 |
| `crates/xvision-dashboard/src/routes/scenarios/**` | `q15-object-json-output` (get only) | q15 |
| `frontend/web/src/api/types.gen/**` | (regenerated; touched by any track that edits a ts-export Rust type) | — |
| `crates/xvision-core/src/providers/**` | `q15-agent-max-tokens-from-model` | q15 |
| `crates/xvision-core/src/models.rs` | `q15-agent-max-tokens-from-model` | q15 |
| `crates/xvision-engine/src/agents/**` | `q15-agent-max-tokens-from-model` | q15 |
| `crates/xvision-engine/src/eval/dispatcher.rs` | `q15-agent-max-tokens-from-model` | q15 |
| `crates/xvision-engine/src/eval/trader_output.rs` | `q15-agent-max-tokens-from-model` (truncation hint surface) | q15 |
| `frontend/web/src/features/agents/**` | `q15-agent-max-tokens-from-model` | q15 |
| `crates/xvision-engine/src/eval/export.rs` | `q15-eval-json-export` | q15 |
| `crates/xvision-dashboard/src/routes/eval/export.rs` | `q15-eval-json-export` | q15 |
| `crates/xvision-dashboard/src/routes/eval/retry.rs` | `q15-eval-retry-button` | q15 |
| `crates/xvision-cli/src/commands/eval/export.rs` | `q15-eval-json-export` | q15 |
| `crates/xvision-cli/src/json/object_shapes.rs` | `q15-eval-json-export` (defines), `q15-object-json-output` (consumes) | q15 |
| `crates/xvision-cli/src/commands/strategy/get.rs` | `q15-object-json-output` | q15 |
| `crates/xvision-cli/src/commands/agent/get.rs` | `q15-object-json-output` | q15 |
| `crates/xvision-dashboard/src/routes/strategies/get.rs` | `q15-object-json-output` | q15 |
| `crates/xvision-dashboard/src/routes/agents/get.rs` | `q15-object-json-output` | q15 |
| `frontend/web/src/features/eval-runs/export/**` | `q15-eval-json-export` | q15 |
| `frontend/web/src/features/eval-runs/retry-button.tsx` | `q15-eval-retry-button` | q15 |
| `frontend/web/vite.config.ts` | `q15-tailscale-serve-api-reachability` | q15 |
| `frontend/web/src/api/client.ts` | `q15-tailscale-serve-api-reachability` | q15 |
| `frontend/web/MOBILE.md` | `q15-tailscale-serve-api-reachability` | q15 |
| `crates/xvision-dashboard/src/server.rs` | `q15-tailscale-serve-api-reachability` | q15 |
| `crates/xvision-dashboard/src/lib.rs` | `q15-tailscale-serve-api-reachability` | q15 |
| `crates/xvision-dashboard/src/state.rs` | `q15-tailscale-serve-api-reachability` (if Host/Origin allowlist lives here) | q15 |
| `scripts/serve-tailscale.sh` | `q15-tailscale-serve-api-reachability` (new, optional) | q15 |
| `docs/runbook/tailscale-serve.md` | `q15-tailscale-serve-api-reachability` (new file) | q15 |
| `crates/xvision-engine/migrations/**` | (none — frozen until a new migration is reserved in `v1-shipping-plan.md`) | — |
| `crates/xvision-engine/src/eval/review/**` | `eval-review-agent-engine` | eval-review |
| `crates/xvision-engine/src/eval/store.rs` | `eval-review-agent-engine` (review helpers only) | eval-review |
| `crates/xvision-core/src/agent_profiles.rs` | `eval-review-agent-engine` | eval-review |
| `crates/xvision-dashboard/src/routes/eval/review.rs` | `eval-review-api-cli` | eval-review |
| `crates/xvision-dashboard/src/routes/docs/**` | `v2a-in-app-docs` | v2a |
| `crates/xvision-cli/src/commands/eval/review.rs` | `eval-review-api-cli` | eval-review |
| `crates/xvision-cli/src/commands/example/**` | `v2a-example-artifacts` | v2a |
| `crates/xvision-engine/src/strategies/templates.rs` | `v2a-example-artifacts` (template-id additions only) | v2a |
| `data/examples/**` | `v2a-example-artifacts` | v2a |
| `frontend/web/src/routes/eval-runs-detail.tsx` | `eval-review-run-detail-ui` | eval-review |
| `frontend/web/src/features/eval-runs/review/**` | `eval-review-run-detail-ui` | eval-review |
| `frontend/web/src/api/eval-review.ts` | `eval-review-run-detail-ui` | eval-review |
| `frontend/web/src/features/onboarding/**` | `v2a-driver-tour` | v2a |
| `frontend/web/src/routes/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/features/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/api/docs.ts` | `v2a-in-app-docs` | v2a |
| `frontend/web/package.json` | `v2a-driver-tour` (adds `driver.js` only) | v2a |
| `frontend/web/src/themes/**` | (closed-out wave: `color-themes-light-dark`) — request specific token additions through a contract update | — |

## Multi-owner exemptions

Rows that may be edited by more than one active contract, with a coordination rule:

| Path | Owners | Coordination rule |
|---|---|---|
| `crates/xvision-dashboard/src/routes/eval/mod.rs` | `eval-review-api-cli`, `q15-eval-json-export`, `q15-eval-retry-button` | Route registration only; no business logic. One PR at a time. |
| `crates/xvision-cli/src/commands/eval/mod.rs` | `eval-review-api-cli`, `q15-eval-json-export` | Subcommand registration only. One PR at a time. |
| `crates/xvision-cli/src/commands/mod.rs` | `v2a-example-artifacts` | Subcommand registration only. One PR at a time. |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | Mount points only; no refactor. |
| `frontend/web/src/routes/eval-runs-detail.tsx` | `eval-review-run-detail-ui`, `q15-eval-json-export` (Download JSON button), `q15-eval-retry-button` | Single-writer; serialize PRs through the conflict-zone registry. |
| `frontend/web/src/components/scenario/ScenarioForm.tsx` | `q15-scenario-warmup-bars` (adds Context bars field), `q15-scenario-granularity-dropdown` (replaces datalist with native select) | Independent regions of the form; merge in either order. |
| `crates/xvision-engine/src/eval/store.rs` | `eval-review-agent-engine` (review helpers), `q15-eval-json-export` (read-only load helpers) | Append-only additions; do not refactor existing fns. |
| `crates/xvision-cli/src/json/object_shapes.rs` | `q15-eval-json-export` (defines), `q15-object-json-output` (consumes) | Definer lands first; consumer stacks if needed. |

## Out of scope this wave

Paths with no listed owner are not blocked, but a new track touching them
must add an ownership row in the same PR as the contract.
