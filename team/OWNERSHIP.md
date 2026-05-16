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
| `crates/xvision-dashboard/src/routes/eval/mod.rs` | `eval-review-api-cli` | Route registration only; no business logic. One PR at a time. |
| `crates/xvision-cli/src/commands/eval/mod.rs` | `eval-review-api-cli` | Subcommand registration only. One PR at a time. |
| `crates/xvision-cli/src/commands/mod.rs` | `v2a-example-artifacts` | Subcommand registration only. One PR at a time. |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | Mount points only; no refactor. |

## Out of scope this wave

Paths with no listed owner are not blocked, but a new track touching them
must add an ownership row in the same PR as the contract.
