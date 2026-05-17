# Dashboard auth runbook

Lands with `qa-dashboard-auth-hardening` (2026-05-17). This document covers
the dashboard's HTTP auth gate, the CLI-jobs allowlist, and the danger-op
typed-phrase verification.

## When does auth apply?

The dashboard process inspects its bind address at startup:

| Bind | Auth posture |
|---|---|
| `127.0.0.1:<port>` (or `::1`) | Loopback-only. No token required. |
| `0.0.0.0:<port>`, `::`, public IPs | Non-loopback. `XVN_DASHBOARD_TOKEN` env var **must** be set; otherwise the process refuses to start. |

Loopback connections from the local machine bypass the gate even on a
non-loopback bind — so SSH tunneling stays frictionless. Every other
peer must present the configured shared secret.

## Configuring the shared secret

```bash
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788
```

If the env var is missing on a non-loopback bind:

```
Error: XVN_DASHBOARD_TOKEN must be set to a non-empty secret when the
dashboard binds to a non-loopback address (0.0.0.0:8788). See
docs/runbook/dashboard-auth.md for details.
```

Treat the secret as a credential — generate via `openssl rand -hex 32`
or your preferred CSPRNG. Don't commit it. Distribute via the same
channels you'd use for an API key.

## Presenting the secret

Three channels are accepted (constant-time compared, equivalent
priority — first match wins):

1. **Authorization header:** `Authorization: Bearer <token>`
2. **Dedicated header:** `X-Xvision-Token: <token>`
3. **Query parameter:** `?token=<token>` (URL-encoded). Useful for SSE
   subscriptions and download links where attaching a header is awkward.

Failed authentication returns HTTP 401 with a JSON body:

```json
{ "code": "unauthorized", "message": "missing or invalid dashboard auth token" }
```

## CLI jobs allowlist

`POST /api/cli/jobs` runs `xvn` subcommands on the dashboard host. The
new allowlist defaults to a small set of safe templates — today just
`bars fetch` (the per-scenario "fetch missing bars" panel).

To opt back into the legacy permissive behavior for local development:

```bash
export XVN_DASHBOARD_CLI_DEVMODE=1
```

A few subcommands (`dashboard`, `mcp`, `fire-trade`) remain rejected
**even in devmode** — they have no business running over an HTTP
surface regardless of configuration.

The devmode flag is **not** a substitute for the auth gate above.
Non-loopback binds still require `XVN_DASHBOARD_TOKEN` regardless of
CLI devmode.

## Danger-op typed phrases

The Settings → Danger Zone routes (`/api/settings/danger/wipe-db`,
`/api/settings/danger/factory-reset`, `/api/settings/danger/regen-identity`)
no longer accept a static `"yes-i-am-sure"` token. Each route requires
the operator to type the matching phrase verbatim. The frontend renders
each phrase next to its input but does not auto-fill the payload.

| Route | Required phrase |
|---|---|
| `/api/settings/danger/wipe-db` | `WIPE DATABASE` |
| `/api/settings/danger/factory-reset` | `FACTORY RESET` |
| `/api/settings/danger/regen-identity` | `REGEN IDENTITY` |

Distinct phrases defend against a single typed string accidentally
firing the wrong destructive op. The engine rejects anything that
doesn't match its per-route expectation with a `Validation` error
naming the expected phrase.

## Out of scope

This wave does not ship:

- TLS termination — run the dashboard behind a reverse proxy (nginx,
  caddy, traefik) that terminates TLS and forwards to the
  dashboard's plain HTTP port.
- Per-user identities / RBAC — the shared secret authenticates the
  caller but doesn't identify them. Richer auth lands in V2B+.
- Session cookies, OAuth, SSO. The `Authorization: Bearer` channel is
  available for integration with token-issuing systems if needed; the
  dashboard itself doesn't issue tokens.

## Quick smoke test

```bash
# Loopback bind: works without a token
xvn dashboard serve --bind 127.0.0.1:8788 &
curl -sS http://127.0.0.1:8788/api/health

# Public bind: refused without token
xvn dashboard serve --bind 0.0.0.0:8788
# Error: XVN_DASHBOARD_TOKEN must be set ...

# Public bind: with token
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788 &
curl -sS http://<host>:8788/api/health        # → 401
curl -sS -H "Authorization: Bearer $XVN_DASHBOARD_TOKEN" http://<host>:8788/api/health
```
