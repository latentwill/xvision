# Dashboard Auth Boundary — Engineering Notes

## Two-layer model

```
Client request
    │
    ▼
┌─────────────────────────────────────────────────────┐
│  Layer 1: auth_middleware (gate.rs)                 │
│  • Non-loopback binds only                          │
│  • Checks XVN_DASHBOARD_TOKEN shared secret         │
│  • Bearer header / x-xvision-token / cookie / ?token│
│  • Loopback clients pass through unconditionally    │
└──────────────────────────┬──────────────────────────┘
                           │
                           ▼
┌─────────────────────────────────────────────────────┐
│  Layer 2: require_auth_middleware (require_auth.rs) │
│  • Mutating routes only (POST / PUT / PATCH / DELETE)│
│  • Checks dashboard_sessions token (24h TTL)        │
│  • Loopback clients pass through; audit row written │
│  • Writes auth_audit row for every call             │
└──────────────────────────┬──────────────────────────┘
                           │
                           ▼
              Route handler (with AuthContext)
```

## Tailscale remote-CLI surface — exemption rationale

The Tailscale remote-CLI path (`/api/cli/jobs/*`) is **exempt from
Layer 2 session-token auth** under the following conditions:

1. **Network-layer gate**: The dashboard process is reachable from the
   Tailscale tailnet only when bound to a non-loopback address with
   `XVN_DASHBOARD_TOKEN` set. Tailscale's ACL rules are the primary
   network gate; only explicitly enrolled nodes can reach the dashboard.

2. **Application-layer audit**: Every CLI job creation and mutating
   operation writes an `auth_audit` row. The `source_ip` field records
   the Tailscale node's IP, and the `session_token_hash` field records
   `"tailscale:<node>"` when the `X-Tailscale-Node` header is present.

3. **Operator opt-in for strict mode**: An operator who wants to require
   session tokens even from Tailscale nodes can set
   `XVN_DASHBOARD_STRICT_SESSION=1`. This environment variable is
   reserved for a future enforcement pass and is not yet implemented.
   The `auth_audit` table was designed to support retroactive analysis
   when strict mode is enabled.

### Why not full session tokens for Tailscale CLI calls?

The xvn CLI submits jobs via `POST /api/cli/jobs` immediately after
boot — there is no interactive browser session to carry a cookie, and
no reliable way to inject a session token into the CLI process without
either (a) storing the token in the operator's env/config file
(security footgun) or (b) requiring the operator to authenticate via a
browser before any CLI command can run (unworkable for automated scripts
and cron jobs).

The Tailscale ACL is the appropriate gate for network-level access
control in this setup. The application layer records who submitted each
job (Tailscale node identity) in `auth_audit` so the audit trail is
complete even without a session token.

### Migration path to strict mode

When a future V2B+ track implements per-user identity (OIDC or API
keys), the Tailscale path will gain a service-credential auth flow:

1. The `xvn` CLI fetches a short-lived service token from a token
   endpoint using the Tailscale node credential.
2. That token is used as the `Authorization: Bearer` header on
   subsequent API calls.
3. The `XVN_DASHBOARD_STRICT_SESSION=1` flag can then be enabled.

Until then, Tailscale ACL + `auth_audit` is the accepted posture.
