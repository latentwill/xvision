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

The current Tailscale deployment path is a testing convenience, not the
final user-facing auth model. It still counts as a non-loopback dashboard bind,
so test stacks that expose the dashboard through a Tailscale sidecar must set
`XVN_DASHBOARD_TOKEN` in their remote `.env`. Local development can avoid the
token by binding the dashboard to `127.0.0.1`.

## Presenting the secret

Four channels are accepted (constant-time compared, equivalent
priority — first match wins):

1. **Authorization header:** `Authorization: Bearer <token>`
2. **Dedicated header:** `X-Xvision-Token: <token>`
3. **Bootstrap cookie:** `xvn_dashboard_token=<token>`, scoped to `/`,
   `HttpOnly`, `SameSite=Lax`. The dashboard sets this automatically
   after a valid header or query-token request so browser page loads can
   fetch `/assets/*` and same-origin `/api/*` routes without appending the
   token to every request.
4. **Query parameter:** `?token=<token>` (URL-encoded). Useful for SSE
   subscriptions and download links where attaching a header is awkward.

For browser use on a non-loopback bind, open the dashboard once with
`?token=<token>` or arrive through a proxy that injects one of the
headers. The response sets the bootstrap cookie; subsequent HTML assets
and same-origin API calls authenticate through that cookie.

Failed authentication returns HTTP 401 with a JSON body:

```json
{ "code": "unauthorized", "message": "missing or invalid dashboard auth token" }
```

## CLI jobs policy

`POST /api/cli/jobs` runs `xvn` subcommands on the dashboard host. The
remote surface accepts typed argv only — no shell text, no caller-controlled
cwd, and no caller-controlled env.

Normal operator/eval/research commands are supported without a dev-mode
bypass, including `eval`, `strategy`, `scenario`, `experiment`, `doctor`,
`report`, `run`, and read-oriented provider/bars commands. Some subcommands
remain categorically rejected because they start servers or can directly
mutate live broker state:

- `dashboard`
- `mcp`
- `fire-trade`
- `close-position`

Some nested commands under otherwise-supported heads are also rejected where
they are destructive, mutate host configuration, or perform authoring/admin
writes that belong in typed dashboard/API flows. That includes `bars rm`,
`bars gc`, `provider add`, `provider remove`, `provider refresh-models`,
scenario authoring/deletion (`scenario create`, `clone`, `archive`, `rm`,
`classify`, `set-regime`), strategy authoring mutations (`strategy new`,
`add-agent`, `remove-agent`, `set-pipeline`, `migrate-agents`), experiment
ledger edits (`experiment new`, `create`, `update`), `example seed`, `store migrate`,
and observability admin writes (`obs retention set`, `obs retention clear`,
`obs janitor run`). Top-level `migrate` is also rejected.

This policy is not a substitute for the auth gate above. Non-loopback binds
still require `XVN_DASHBOARD_TOKEN`.

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
- Per-user session management, OAuth, SSO. The bootstrap cookie only
  carries the configured shared secret for browser ergonomics; it is not
  a per-user identity/session system.

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
curl -i "http://<host>:8788/?token=$XVN_DASHBOARD_TOKEN" # → Set-Cookie: xvn_dashboard_token=...
```
