---
from: frontend-2-settings
to: all
topic: pr-open
created_at: 2026-05-10T18:00:00Z
ack_required: false
---

# Frontend Plan 2 Settings sub-slice — PR #18 open

PR: https://github.com/latentwill/xvision/pull/18
Branch: `feature/frontend-2-settings`
Worktree: `.worktrees/frontend-2-settings`

## What landed

Plan 2 Tasks 6, 7, 8 + frontend half of 16 — read-only Settings tabs for
brokers, daemon, identity. Providers + danger explicitly out of scope.

Backend (`engine::api::settings/`):
- `brokers::get` — env-var presence (never values) for
  Alpaca (`APCA_API_KEY_ID`, `APCA_API_SECRET_KEY`) and Orderly
  (`ORDERLY_KEY`/`SECRET`/`ACCOUNT_ID`); `configured: bool` rollup;
  optional `base_url` override; per-broker note.
- `daemon::get` — v1 stub: `status: "not_applicable"` +
  `deferred_to_plan: "...-2c-scheduler-live-exec.md"`.
- `identity::get` — `feature_compiled_in: false` hardcoded for v1;
  `MANTLE_RPC_URL` + `XVN_WALLET_KEY` presence flags.
- ts-rs derives on all response structs; 7 new .ts files emitted.

Dashboard:
- Three handlers under `/api/settings/{brokers,daemon,identity}`.
- Registered before SPA fallback in `server.rs`.

Frontend:
- `api/settings.ts` typed fetchers + TanStack Query keys.
- `routes/settings/index.tsx` real cards with per-credential rows,
  configured/not pill, deferred-plan note for daemon, env flag table
  for identity. Reuses the strategies loading/error/empty pattern.
- Providers + danger become PlaceholderTabs pointing at owning plans.

## Tested

- `cargo build --workspace` green.
- `cargo test -p xvision-dashboard --test http` — 8/8 (existing 4 plus
  4 new). Env-touching tests serialize on a static ENV_LOCK mutex with
  RAII guards restoring prior values on drop.
- `pnpm typecheck && pnpm build` green.
- Live smoke (port 8803, `APCA_*` set): brokers shows alpaca configured=
  true (no value leaks), orderly configured=false. Daemon + identity
  return v1 stubs.

## Coordination notes

- **Independent of llm-providers Phase 2/3** — that track owns
  `xvision-core::config` schema + `[[providers]]` persistence; this PR
  only adds `engine::api::settings/` which is a separate module.
- **Independent of eval-3.B/3.C** — no overlap with `eval/` files.
- The `/api/settings/providers` route is intentionally NOT registered;
  llm-providers Phase 2's PR can claim it without rebasing through this
  PR's server.rs.
- The `routes/settings/index.tsx` Providers + Danger tabs are
  PlaceholderTabs — replacing those is the next natural Settings PR
  once llm-providers Phase 2's persistence lands.
