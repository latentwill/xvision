---
track: v2b-dashboard-auth-boundary
lane: foundation
wave: v2b
worktree: .worktrees/v2b-dashboard-auth-boundary
branch: task/v2b-dashboard-auth-boundary
base: origin/main
status: ready
depends_on: []
blocks:
  - v2b-remote-cli-job-safety
  - v2b-broker-wallet-kill-switch
stacking: none
allowed_paths:
  - crates/xvision-dashboard/src/auth/**                 # NEW — session token issue + verify + middleware
  - crates/xvision-dashboard/src/redact.rs               # NEW — secret-pattern redactor
  - crates/xvision-dashboard/src/routes/**               # split mutating from read-only; attach require_auth
  - crates/xvision-dashboard/src/main.rs                 # bind defaults + bind-warning
  - crates/xvision-dashboard/src/bootstrap.rs            # if bind/init lives here instead
  - crates/xvision-dashboard/Cargo.toml                  # session-token deps (cookie crate, etc.)
  - crates/xvision-dashboard/migrations/**               # new `dashboard_sessions` migration
  - crates/xvision-dashboard/tests/**                    # auth gate + redaction + bind-warning tests
  - frontend/web/src/api/types.gen/**                    # ts-rs regen for any new types
  - frontend/web/src/auth/**                             # session token storage on the SPA side
  - frontend/web/src/api/client.ts                       # attach bearer to requests
forbidden_paths:
  - crates/xvision-execution/**                          # broker pause-gate is v2b-broker-wallet-kill-switch
  - crates/xvision-engine/src/api/safety/**              # ditto
  - crates/xvision-dashboard/src/cli_jobs/**             # v2b-remote-cli-job-safety owns the orphan-recovery surface
  - crates/xvision-engine/migrations/**                  # this track's migration lives under dashboard/
interfaces_used:
  - axum::Router
  - axum::middleware
  - axum::extract::State
  - sqlx::SqlitePool
parallel_safe: false
parallel_conflicts: []
verification:
  - cargo fmt --all -- --check
  - cargo clippy -p xvision-dashboard -- -D warnings
  - cargo test -p xvision-dashboard auth
  - cargo test -p xvision-dashboard redact
  - cargo test -p xvision-dashboard bind
  - pnpm --dir frontend/web typecheck
acceptance:
  - **Mutating routes require auth.** Audit every `POST`, `PUT`, `PATCH`, `DELETE`, and SSE-mutating route under `crates/xvision-dashboard/src/routes/`. Attach `require_auth` middleware. `curl POST` without a session token returns 401 with `{"error": "unauthenticated"}`. `curl POST` with a valid session token returns the route's normal response.
  - **Read-only routes stay open on loopback.** Routes that only read (status, list, get) remain accessible without a token on loopback binds. Document the read/write split with a comment block on each route file.
  - **Session token model.** `POST /api/auth/session` issues a new session: returns a UUID token, persists `{token_hash, created_at, expires_at, source_ip}` to a new `dashboard_sessions` table. `DELETE /api/auth/session` revokes. Token is verified via constant-time hash comparison. Default TTL: 24h, configurable via `dashboard.session_ttl` in the dashboard's config.
  - **Bind defaults.** `xvn dash` binds to `127.0.0.1:<port>` by default. Binding to a non-loopback address (`--bind 0.0.0.0` or any other interface) prints a loud single-line `WARNING: dashboard bound to <addr>; ensure firewall/Tailscale ACL restricts access` to stderr at startup. No popup, no extra UI — terminal warning only.
  - **Secret redaction at the API boundary.** New `crates/xvision-dashboard/src/redact.rs` defines a redactor that scans for and replaces:
    * Provider tokens: any string matching `sk-[A-Za-z0-9]{20,}` (OpenAI/Anthropic format), `OR-[A-Za-z0-9-]{20,}` (OpenRouter), `xai-[A-Za-z0-9]{20,}` (xAI), etc.
    * Broker tokens: Alpaca `PK[A-Z0-9]{16,}` (key) + bearer paired patterns; Coinbase, Binance API key prefixes if present.
    * Wallet seeds/mnemonics: any string of 12 or 24 BIP-39 words; private-key hex (`0x[0-9a-f]{64}`).
    * Tailscale auth keys: `tskey-[a-z0-9]{20,}`.
    Replaces matches with `[REDACTED:<kind>]`.
  - **Redaction is wired into:** (a) the API error response formatter, (b) the SSE event serializer, (c) the job-log writer, (d) any exported example/artifact. Tests assert that planted-secret payloads come out redacted from every channel.
  - **Auth middleware writes an audit row** for every mutating call: `auth_audit(timestamp, route, method, session_token_hash, source_ip, response_status)`. Schema persisted by the session migration.
  - **The Tailscale remote-CLI surface** either inherits a service-token auth context (preferred) OR is documented as exempt with a written rationale block in `crates/xvision-dashboard/src/auth/README.md` (the auth boundary is per-API; Tailscale-only routes can rely on Tailscale ACL for the network gate as long as the application records the authenticated user on every audit row).
  - **Frontend wires session token.** On dashboard load, fetch `GET /api/auth/session/current`; if 401, show a single-screen inline login (no popup). On successful auth, the bearer token is attached to every mutating API call via `crates/frontend/web/src/api/client.ts`.
  - **Tests:** 401 on each mutating route without token; 200 with token; redaction of planted secrets in each output channel; bind-warning printed on non-loopback bind; session expiry honoured.
  - **`cargo clippy -p xvision-dashboard -- -D warnings` clean.**
  - **No popups in the SPA** (`CLAUDE.md` rule).

---

# Scope

V2B foundation track. Defines the auth surface every other V2B track
audits against. Implements the action plan §V2B work packages 1
(dashboard and API auth) and 2 (secret handling) — these merge because
redaction lives at the same API boundary the auth middleware gates.

# Out of scope

- Broker pause/kill switch — `v2b-broker-wallet-kill-switch` owns it.
- CLI job orphan recovery + audit fields — `v2b-remote-cli-job-safety` owns them; this contract only lands the auth context they consume.
- OIDC / OAuth flow. The first cut is a session-token model; OIDC integration is a follow-up if/when the operator opts in.

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/v2b-dashboard-auth-boundary status
git -C .worktrees/v2b-dashboard-auth-boundary log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/v2b-dashboard-auth-boundary -b task/v2b-dashboard-auth-boundary origin/main
```

# Notes

- The dashboard already has an `xvision-dashboard` crate. The agent should grep the existing `routes/` directory to inventory mutating routes before authoring the middleware — the route list is long and the audit needs to be exhaustive.
- Session storage in SQLite (within the dashboard's existing pool) avoids adding a new dependency. Cookie-based session for the browser; bearer header for the CLI and HTTP API.
- Constant-time comparison for token verification: use the `subtle` crate (already a transitive dep) or `ring::constant_time::verify_slices_are_equal`.
- The redactor should be a `tracing` layer / serializer wrapper, not a string post-processor, so structured fields are redacted before formatting. Use `tracing_subscriber::fmt::format::JsonFields` or similar interception point.
