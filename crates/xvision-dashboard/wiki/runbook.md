# Operator Runbook

## Dashboard authentication

Shipped with the `qa-dashboard-auth-hardening` wave. This section covers the
HTTP auth gate, the CLI-jobs allowlist, and the danger-op typed-phrase
verification.

### When auth applies

The dashboard inspects its bind address at startup:

| Bind address | Auth posture |
|---|---|
| `127.0.0.1:<port>` or `::1` | Loopback only. No token required. |
| `0.0.0.0:<port>`, `::`, public IPs | Non-loopback. `XVN_DASHBOARD_TOKEN` must be set; the process refuses to start without it. |

Loopback connections from the local machine bypass the gate even on a
non-loopback bind, so SSH tunneling stays frictionless.

### Configuring the shared secret

```bash
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788
```

If the env var is missing on a non-loopback bind:

```
Error: XVN_DASHBOARD_TOKEN must be set to a non-empty secret when the
dashboard binds to a non-loopback address (0.0.0.0:8788).
```

Generate the secret with `openssl rand -hex 32` or equivalent CSPRNG. Do not
commit it. Distribute it the same way you'd distribute an API key.

Tailscale deployments still count as non-loopback binds — remote `.env` files
must include `XVN_DASHBOARD_TOKEN`. Local development can avoid the token by
binding to `127.0.0.1`.

### Presenting the secret

Four channels are accepted, first match wins:

1. `Authorization: Bearer <token>` header
2. `X-Xvision-Token: <token>` header
3. `xvn_dashboard_token=<token>` cookie (set automatically after a valid header
   or query-token request, so subsequent browser page loads authenticate
   through the cookie without re-appending the token)
4. `?token=<token>` query parameter — useful for SSE subscriptions and download
   links

Failed authentication returns HTTP 401:

```json
{ "code": "unauthorized", "message": "missing or invalid dashboard auth token" }
```

### CLI jobs allowlist

`POST /api/cli/jobs` runs `xvn` subcommands on the dashboard host. By default
the allowlist is a broad read/eval/research set: all read-only verbs plus the
scoped, hard-capped, cancellable ones (`eval run`, `eval compare/watch`,
`experiment run`, `model bakeoff`, `bars fetch`) and low-risk strategy draft
creation (`strategy create` / `strategy new`). Unscoped writes
(`agent create`, `scenario create`, …) and categorically dangerous heads
(`fire-trade`, `close-position`, `migrate`, `dashboard`, `mcp`) are denied. The authoritative list is
`crates/xvision-dashboard/src/cli_jobs/allowlist.rs`.

To turn the policy into a **full bypass** on a trusted dev node:

```bash
export XVN_DASHBOARD_CLI_DEVMODE=1   # accepts 1 or true
```

In full-bypass mode **every** argv is accepted — including the live-trade
(`fire-trade`, `close-position`) and host-admin (`migrate`, `dashboard`,
`mcp`) verbs. Only set this on a dev node that (a) has no live broker
credentials and (b) is reachable solely from a trusted tailnet. It does NOT
replace the auth gate — non-loopback binds still require `XVN_DASHBOARD_TOKEN`
regardless of CLI devmode.

### Danger-op typed phrases

The Settings Danger Zone routes require typing the matching phrase verbatim.
The frontend renders each phrase but does not auto-fill the payload.

| Route | Required phrase |
|---|---|
| `/api/settings/danger/wipe-db` | `WIPE DATABASE` |
| `/api/settings/danger/factory-reset` | `FACTORY RESET` |
| `/api/settings/danger/regen-identity` | `REGEN IDENTITY` |

Distinct phrases prevent a single typed string from accidentally firing the
wrong destructive operation. The engine rejects anything that doesn't match the
per-route expectation with a `Validation` error naming the expected phrase.

### Quick smoke test

```bash
# Loopback bind: works without a token
xvn dashboard serve --bind 127.0.0.1:8788 &
curl -sS http://127.0.0.1:8788/api/health

# Non-loopback: refused without token
xvn dashboard serve --bind 0.0.0.0:8788
# Error: XVN_DASHBOARD_TOKEN must be set ...

# Non-loopback: with token
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
xvn dashboard serve --bind 0.0.0.0:8788 &
curl -sS http://<host>:8788/api/health                            # → 401
curl -sS -H "Authorization: Bearer $XVN_DASHBOARD_TOKEN" http://<host>:8788/api/health
curl -i "http://<host>:8788/?token=$XVN_DASHBOARD_TOKEN"          # → Set-Cookie: xvn_dashboard_token=...
```

### Out of scope

- TLS termination: run the dashboard behind a reverse proxy (nginx, caddy,
  traefik) that terminates TLS and forwards to the plain HTTP port.
- Per-user identities, RBAC, OAuth, SSO: the shared secret authenticates the
  caller but does not identify them. The bootstrap cookie carries the shared
  secret for browser ergonomics only.

---

## Observability + OpenTelemetry

xvision writes a canonical SQLite ledger (`agent_runs`, `spans`, `model_calls`,
`tool_calls`, ...) for every run. An optional **OTel tee** mirrors each recorder
call as an OTel span and ships it to a configured OTLP collector (Jaeger, Tempo,
Honeycomb, etc.).

The tee is off by default. The `xvision:latest` production image does not
include OTel dependencies unless built with the `otel` cargo feature on
`xvision-observability`.

### What gets exported

| Data | SQLite | OTel |
|---|---|---|
| run id | yes | yes (`xvision.run.id`) |
| span hierarchy | yes | yes |
| span kind / status | yes | yes |
| token count | yes | yes (`xvision.model.*`) |
| cost | yes | no (SQLite only) |
| prompt / response hash | yes | yes (`*_hash` attributes) |
| tool input hash | yes | yes |
| tool exit code | yes | yes |
| approval flag | yes | yes |
| full prompt | gated | never |
| full tool payload | gated | never |
| replay checkpoint | yes | id only |
| supervisor note text | yes | role + severity only |

Full prompts, full tool inputs, and full tool outputs never leave the local
SQLite / blob store via OTel. This restriction is enforced at the type level in
`crates/xvision-observability/src/otel.rs`. For full-prompt visibility, use
`xvn observe run <run_id>` against the local SQLite ledger.

### Building with the OTel feature

```bash
cargo build -p xvision-observability --features otel
cargo build -p xvision-engine        --features otel
```

Both default-features and otel-feature test runs are required to pass:

```bash
cargo test -p xvision-observability --no-default-features
cargo test -p xvision-observability --features otel
```

### Environment variables

| Variable | Meaning | Default |
|---|---|---|
| `OTEL_EXPORTER_OTLP_ENDPOINT` | OTLP gRPC endpoint | `http://localhost:4317` |
| `OTEL_SERVICE_NAME` | `service.name` resource attribute | `xvision` |
| `OTEL_RESOURCE_ATTRIBUTES` | Extra `key=value` resource attributes | *(none)* |

Example:

```bash
export OTEL_EXPORTER_OTLP_ENDPOINT=http://collector.internal:4317
export OTEL_SERVICE_NAME=xvision-prod
export OTEL_RESOURCE_ATTRIBUTES=deployment.environment=prod,host.name=$(hostname)
xvn serve
```

### Correlating OTel traces with SQLite rows

When the OTel feature is enabled, `agent_runs.otel_trace_id` and
`spans.otel_trace_id` / `spans.otel_span_id` are populated on every recorder
write. Given a trace id from Jaeger or Tempo:

```sql
SELECT * FROM agent_runs WHERE otel_trace_id = '<trace-id>';
```

When the feature is off, those columns are NULL.

### Disabling the tee

Three options, in increasing order of permanence:

1. **Per-process:** leave the cargo feature on but set
   `observability.otel_enabled = false` in `$XVN_HOME/config/observability.toml`.
   The bus subscribes only the `SqliteRecorder`.
2. **Per-build:** drop `--features otel` and rebuild. The OTel crates are not
   linked; `xvision:latest` ships this way by default.
3. **Per-host:** point `OTEL_EXPORTER_OTLP_ENDPOINT` at a sink that drops
   everything. Not recommended — the pipeline will retry and log warnings.

### Troubleshooting

**No spans reach the collector.** `tracing::subscriber::set_global_default` must
run exactly once, early in process boot, before any agent-run work. A
per-thread `set_default` does not propagate to the bus consumer's Tokio task
and silently drops every exported span.

**Spans appear in SQLite but `otel_trace_id` columns are NULL.** The producer
is not stamping ids on `SpanStarted` events. Confirm the producer calls
`OtelIds::from_current()` from inside the active tracing span (after
`span.enter()`), not before entering it.

**CI build fails on the OTel feature.** Check the pinned dependency matrix in
`crates/xvision-observability/Cargo.toml`: `opentelemetry = 0.21`,
`opentelemetry-otlp = 0.14`, `tracing-opentelemetry = 0.22`. Version mismatches
are the most common cause of breakage.

## See also

- [Operator Manual](/docs?slug=operator-manual) — env-var setup and live-node remote control.
- [CLI Reference](/docs?slug=cli-reference) — full `xvn` command surface.
