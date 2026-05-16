---
from: frontend-2-home-and-health
to: all
topic: pr-open
created_at: 2026-05-10T17:25:00Z
ack_required: false
---

# Frontend Plan 2 Task 1 — PR #13 open

PR: https://github.com/latentwill/xvision/pull/13
Branch: `feature/frontend-2-home-and-health`
Worktree: `.worktrees/frontend-foundation` (reused after Phase B merged)

## What landed

Plan 2 Task 1 (real `/api/health` probes) plus the matching frontend
reflection. Scope deliberately narrow — does not pull in Task 2 (Home
aggregator) which depends on Plan 2 Task 3 (`engine::api::eval::list_runs`).

Backend:
- `engine::api::health` — `HealthReport { status, probes }`, three local
  probes (`data_dir`, `db`, `bundles`), worst-status rollup, ts-rs
  derives on all three types.
- `dashboard::routes::health::health` is a thin wrapper over
  `engine::api::health::check`. Always 200; partial outages live in
  `probes[*]`.

Frontend:
- `api/health.ts` typed `getHealth()` + TanStack Query keys.
- `components/shell/HealthPill.tsx` replaces the static
  "paper · localhost" pill with a live one (15s poll, gold/warn/danger
  color, per-probe `title=` hover with detail+latency, "checking…"
  pending, "offline" on engine-down).

## Tested

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard --test http` — 5/5 (probes,
  db latency, `/api/* → 404`, empty + seeded `/api/strategies`).
- `pnpm typecheck && pnpm build` green.
- Live smoke (port 8802): cold + warm runs both all-ok, db probe
  records 0ms latency.

## Pattern for downstream tracks

When adding a new external dep (alpaca paper, llm provider, eval
executor heartbeat), append a `Probe` to the `check()` result.
HealthStatus aggregates worst-state automatically. ts-rs picks up the
new fields via `cargo xtask gen-types`.

## Notes for parallel work

- This PR touches `engine::api::mod.rs` (one new `pub mod health;` line)
  and `dashboard::routes::mod.rs` indirectly via the existing
  `pub mod health;` export. No conflicts expected with eval-engine 3.B
  or strategy-2a-A/B/C tracks.
- The Topbar pill replaces a hardcoded "paper · localhost" with a live
  `HealthPill` — if Plan 2d's wizard or Plan 4's chat rail also wants to
  edit the Topbar, rebase will surface the rename.
