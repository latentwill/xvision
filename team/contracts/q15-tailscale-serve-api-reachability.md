---
track: q15-tailscale-serve-api-reachability
lane: integration
wave: q15
worktree: .worktrees/q15-tailscale-serve-api-reachability
branch: task/q15-tailscale-serve-api-reachability
base: origin/main
status: deferred
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/vite.config.ts
  - frontend/web/src/api/client.ts
  - frontend/web/MOBILE.md
  - crates/xvision-dashboard/src/server.rs
  - crates/xvision-dashboard/src/lib.rs
  - crates/xvision-dashboard/src/state.rs           # if Origin/Host allowlist lives there
  - scripts/serve-tailscale.sh                     # new helper, optional
  - docs/runbook/tailscale-serve.md                # new short runbook
forbidden_paths:
  - crates/xvision-engine/**
  - crates/xvision-engine/migrations/**
  - frontend/web/src/features/**                   # symptom is universal; fix is in transport, not features
  - frontend/web/src/routes/**
interfaces_used:
  - apiFetch                                       # same-origin /api/* client
  - Axum Router / TcpListener::bind
  - Vite dev proxy
  - tailscale serve / tailscale funnel
parallel_safe: false
parallel_conflicts:
  - q15-agent-max-tokens-from-model               # do not edit dashboard/state.rs concurrently
  - any active dashboard route track              # do not refactor server.rs while routes change
verification:
  - Local repro: `tailscale serve --bg http://127.0.0.1:5180` (dev) and `tailscale serve --bg http://127.0.0.1:8788` (deploy image); load `/`, `/strategies`, `/agents`, `/eval-runs`, `/settings` and a chat rail message over the `*.ts.net` URL on a phone.
  - corepack pnpm --dir frontend/web typecheck
  - corepack pnpm --dir frontend/web test -- api-client
  - cargo test -p xvision-dashboard server::bind
  - Manual SSE smoke: open a long-running eval and confirm the stream survives ≥ 60s over the `*.ts.net` URL.
acceptance:
  - Tailscale Serve URL renders the dashboard and every page (Strategies, Agents, Eval Runs, Settings, Chat rail) can issue `/api/*` calls without HTTP errors.
  - Chat rail completes at least one streamed message end-to-end over `*.ts.net`.
  - One eval run's SSE stream stays open ≥ 60s over `*.ts.net` without disconnects (or, if Tailscale Serve cannot proxy SSE, the runbook documents the fallback).
  - A short runbook at `docs/runbook/tailscale-serve.md` documents the supported invocation (dev vs deploy image), the correct port to expose, any DNS/MagicDNS prerequisites, and the known SSE/WebSocket limitations.
  - Regression test on `crates/xvision-dashboard/src/server.rs::bind` covers `0.0.0.0` binding and any Origin/Host check changes.
---

# Scope

Diagnose and fix the "HTTP error in chat rail / strategies / agents / eval /
settings" failure mode when the xvision dashboard is served via Tailscale
Serve (`*.ts.net` hostname). Symptom is universal across pages, so the
failure is at the transport layer (host bind, Origin/Host check, proxy
mapping, or SSE forwarding) — not in any single feature.

# Diagnostic order (do not skip)

Before changing code, capture which layer is failing:

1. **Curl the dashboard directly** (`curl -i http://127.0.0.1:8788/api/health`) — confirm it answers.
2. **Curl the same path via the Tailscale Serve URL** (`curl -i https://<machine>.ts.net/api/health`) — confirm what error appears (4xx, 5xx, timeout, connection refused).
3. **Browser devtools network tab on the `*.ts.net` page** — capture the failing request: method, URL, response code, response body. The user's "couldn't reach the engine" wording suggests a specific application error, not a transport error; confirm.
4. **Check `tailscale serve status`** — confirm which local port the serve is mapped to, and that it matches a process actually listening on `0.0.0.0` (not `127.0.0.1` only).
5. **SSE check**: `curl -N https://<machine>.ts.net/api/eval/runs/<id>/stream` — Tailscale Serve historically has limitations on long-lived HTTP responses; this is the most likely root cause for chat rail and eval failures specifically.

Choose the fix path based on the diagnostic output. Do not pre-commit to a
specific fix in this contract.

# Likely fix surfaces

In rough order of likelihood:

- **Tailscale Serve mapped to the wrong port.** The deploy image serves both
  the SPA and API on `:8788` (same origin). The dev setup serves the SPA on
  `:5180` and proxies `/api` to `:8788`. If Serve is pointed at `:8788` in
  dev mode, API works but the SPA doesn't load — and vice versa. The
  runbook should make the correct invocation explicit per mode.
- **Dashboard bound to `127.0.0.1` instead of `0.0.0.0`.** If the dashboard
  is only reachable via loopback, Tailscale Serve cannot proxy it from the
  tailnet edge. Confirm `server.rs::serve` accepts the configured `SocketAddr`
  and the caller passes `0.0.0.0:8788`.
- **Origin/Host allowlist on the dashboard.** If the axum server rejects
  requests whose `Host` header is `*.ts.net`, every API call 4xx's. Lift
  the allowlist to include `*.ts.net` and `*.local` (mirroring the Vite
  `allowedHosts` change from PR #181).
- **SSE / WebSocket forwarding through Tailscale Serve.** If the chat rail
  and eval stream rely on SSE and Serve drops long-lived responses, the
  runbook must document Tailscale Funnel or a tunnel alternative as the
  supported path for streaming features. Code-side fallback: a polling
  mode toggle is out of scope for this track.
- **Mixed-content / scheme mismatch.** `*.ts.net` is HTTPS; if any
  frontend code hardcodes `http://` for API or asset URLs, the browser
  blocks. `apiFetch` already uses relative paths, but verify chart asset
  URLs and any `new URL(..., window.location)` constructions.

# Out of scope

- Authentication on the dashboard API (F35 — separate future track).
- PWA install path (F41 follow-up; depends on F35 + stable HTTPS origin).
- Multi-user surface or per-user identity.
- Production-grade reverse proxy (Caddy/Cloudflare Tunnel/Funnel beyond a
  one-line runbook mention).
- Changing the embedded-SPA / dev-proxy architecture.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git worktree add .worktrees/q15-tailscale-serve-api-reachability -b task/q15-tailscale-serve-api-reachability origin/main
```

# Notes

- This sits next to FOLLOWUPS F41 (mobile beyond the tailnet). F41 is the
  long-horizon story; this contract is "make the tailnet path itself
  reliable today" so dev/QA work over a phone is unblocked.
- PR #181 already made the Vite dev server accept `.ts.net` hostnames at
  the HTTP layer; this track is about everything downstream of that
  (API reachability, streaming, bind address, dashboard-side checks).
- The runbook should sit at `docs/runbook/tailscale-serve.md` (new
  directory). Worker may create that directory.

- **Deferred 2026-05-16** (acting conductor decision). Mobile/QA over the
  tailnet is parked, not archived. Revive by flipping `status:` back to
  `ready` and re-adding the row to the Q15 wave block on `team/board.md`.
  CONFLICT_ZONES claims released; OWNERSHIP rows annotated.
