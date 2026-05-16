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
| `crates/xvision-cli/src/commands/example/**` | `v2a-example-artifacts` | v2a |
| `crates/xvision-engine/src/strategies/templates.rs` | `v2a-example-artifacts` (template-id additions only) | v2a |
| `data/examples/**` | `v2a-example-artifacts` | v2a |
| `frontend/web/src/features/onboarding/**` | `v2a-driver-tour` | v2a |
| `frontend/web/src/routes/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/features/docs/**` | `v2a-in-app-docs` | v2a |
| `frontend/web/src/api/docs.ts` | `v2a-in-app-docs` | v2a |
| `frontend/web/package.json` | `v2a-driver-tour` (adds `driver.js` only) | v2a |
| `frontend/web/src/themes/**` | (closed-out wave: `color-themes-light-dark`) — request specific token additions through a contract update | — |
| `crates/xvision-execution/src/alpaca.rs` | `alpaca-paper-crypto-submit` | post-q15 |
| `crates/xvision-execution/src/broker_surface.rs` | `alpaca-paper-crypto-submit` | post-q15 |
| `crates/xvision-execution/tests/broker_surface.rs` | `alpaca-paper-crypto-submit` | post-q15 |
| `crates/xvision-execution/tests/broker_surface_alpaca_live.rs` | `alpaca-paper-crypto-submit` (adds one `--ignored` operator test) | post-q15 |
| `crates/xvision-engine/src/eval/executor/mod.rs` | `alpaca-paper-crypto-submit` (classifier + format_failure_reason) | post-q15 |
| `crates/xvision-engine/src/eval/executor/paper.rs` | `alpaca-paper-crypto-submit` (crypto short-open no-op branch) | post-q15 |
| `crates/xvision-engine/src/eval/executor/trader_output.rs` | `alpaca-paper-crypto-submit` (only if a new failure-class enum sibling is added) | post-q15 |

## Multi-owner exemptions

Rows that may be edited by more than one active contract, with a coordination rule:

| Path | Owners | Coordination rule |
|---|---|---|
| `crates/xvision-cli/src/commands/mod.rs` | `v2a-example-artifacts` | Subcommand registration only. One PR at a time. |
| `frontend/web/src/routes/index.tsx` | `v2a-driver-tour` | Mount points only; no refactor. |

## Out of scope this wave

Paths with no listed owner are not blocked, but a new track touching them
must add an ownership row in the same PR as the contract.
