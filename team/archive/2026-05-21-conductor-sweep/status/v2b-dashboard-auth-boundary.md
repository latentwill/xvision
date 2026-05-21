# v2b-dashboard-auth-boundary — status

**Status:** complete · PR open for review

**PR:** https://github.com/latentwill/xvision/pull/465

**Branch:** task/v2b-dashboard-auth-boundary  
**Commit:** 6527691

## What shipped

### Backend (xvision-dashboard)

- `auth/` module split: `gate.rs` (existing coarse token gate), `context.rs`
  (canonical AuthContext), `session.rs` (session CRUD, SHA-256 hashing,
  constant-time compare, DB helpers), `require_auth.rs` (per-route middleware
  with loopback exemption + audit row writes), `mod.rs` (re-exports),
  `README.md` (Tailscale exemption rationale)
- `redact.rs`: pure-string redactor; patterns: sk-/sk-ant-/OR-/xai- provider
  tokens, PK broker keys, 0x64hex EVM private keys, BIP-39 12/24-word
  mnemonics, tskey- Tailscale keys
- `server.rs`: readonly_router / mutating_router (with require_auth layer) /
  auth_router; non-loopback bind warning to stderr
- `state.rs`: `run_dashboard_migrations()` via direct DDL (no sqlx Migrator,
  avoids `_sqlx_migrations` conflict)
- `Cargo.toml`: added `sha2 = "0.10"` and `getrandom = "0.2"`
- SQL schema in `migrations/1001_dashboard_sessions.sql` (kept as docs, applied
  via DDL in state.rs)

### Frontend (xvision-web)

- `stores/auth.ts`: sessionStorage-backed Zustand store with expiry check
- `api/auth.ts`: createSession / currentSession / deleteSession
- `api/client.ts`: auto-inject Bearer on POST/PUT/PATCH/DELETE; 401 with
  `error: "unauthenticated"` triggers redirect to `/login?next=…`
- `routes/login.tsx`: full-screen login route, no modal, redirects to ?next
- `routes.tsx`: /login registered outside Layout shell (full viewport)

### Tests — all passing

- `tests/auth_session.rs` — 8 integration tests (session lifecycle, 401 from
  non-loopback, loopback pass, session expiry)
- `tests/bind_warning.rs` — 7 unit tests (loopback detection, AuthState::from_env)
- `tests/redact_channels.rs` — 13 integration tests (all secret patterns)

## Out of scope / follow-up coordination

- `cli_jobs/auth_stub.rs` intentionally NOT deleted — follow-up PR to
  coordinate stub-swap with the #447 (`v2b-remote-cli-job-safety`) parallel track
- `AuthContext` is now canonical in `auth/context.rs` — stub-swap PR can import
  from there

## Verification commands

```bash
export CARGO_TARGET_DIR="$HOME/.cargo-target/xvision-v2b-dashboard-auth-boundary"
export PATH="$HOME/.cargo/bin:$PATH"
cd .worktrees/v2b-dashboard-auth-boundary

cargo test -p xvision-dashboard --test auth_session    # 8/8
cargo test -p xvision-dashboard --test bind_warning    # 7/7
cargo test -p xvision-dashboard --test redact_channels # 13/13
```
