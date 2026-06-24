# Operator Runbook

Shipped with the `qa-dashboard-auth-hardening` wave. The dashboard uses a
**two-layer** auth model:

1. **Outer gate** (`gate.rs` / `auth_middleware`) — coarse non-loopback
   barrier. On a non-loopback bind the server requires
   `XVN_DASHBOARD_TOKEN` and refuses to start without it. Loopback binds
   (`127.0.0.1`, `::1`) pass through unchanged so local dev stays
   frictionless.

2. **Inner layer** (`require_auth.rs` / `require_auth_middleware`) —
   per-route session validation on mutating endpoints. If no operator
   password has been set in the `dashboard_auth` table, all requests pass
   through. Once a password IS configured (via the Settings UI), mutating
   routes require the password presented as `Authorization: Bearer
   <password>`, the `x-xvision-token` header, or the
   `xvn_dashboard_password` cookie. Read-only GET routes are exempt from
   the inner layer but still pass through the outer gate.
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

### Inner auth layer (operator password)

The inner `require_auth` layer gates mutating routes (create, update, delete,
settings) behind an operator-chosen password. Configuration:

1. **No password set** (default) — the `dashboard_auth` table has no hash row.
   All mutating requests pass through. The login endpoint returns
   `password_set: false` so the frontend skips the auth prompt.

2. **Password configured** — set via the Settings → Auth tab in the dashboard
   UI, or directly in SQLite:
   ```sql
   INSERT OR REPLACE INTO dashboard_auth (key, value)
   VALUES ('password_hash', '<sha256-hex-of-password>');
   ```
   Once set, every mutating request must present the password via one of the
   four channels (same ones as the outer gate, but using the
   `xvn_dashboard_password` cookie instead of `xvn_dashboard_token`).

On successful authentication the inner layer sets a 24-hour
`xvn_dashboard_password` cookie so the browser doesn't re-prompt on every
request. Failed authentication returns HTTP 401 with
`{"error": "unauthenticated"}`. Loopback connections bypass the inner layer
just as they bypass the outer gate.

Read-only GET routes (`/api/agents`, `/api/eval-runs`, `/api/health`, etc.)
are **exempt** from the inner layer — they only pass through the outer gate.

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
| `/api/settings/danger/reset-workspace` | `RESET WORKSPACE` |
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
### Out of scope

- TLS termination: run the dashboard behind a reverse proxy (nginx, caddy,
  traefik) that terminates TLS and forwards to the plain HTTP port.
- Per-user identities, RBAC, OAuth, SSO: the outer gate authenticates the
  caller via a shared secret; the inner layer adds an operator password on
  mutating routes. Neither identifies individual users.

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

## Filter timeline

The filter timeline is a per-bar visual strip rendered on the eval run detail
page for **FilterGated** strategies. It shows every bar where the filter
runtime was evaluated, color-coded by outcome. For EveryBar strategies the
timeline self-hides (no filter = no events).

### States

| Color | State | Meaning |
|---|---|---|
| Gold | Triggered | Filter fired — the LLM was woken up on this bar. |
| Amber | In-position suppressed | Filter would have fired but the strategy was already in a position. |
| Blue | Cooldown suppressed | Filter fired recently and the cooldown window hasn't elapsed. |
| Red | Daily-cap suppressed | Filter would have fired but the daily call cap has been reached. |
| Neutral | Not triggered | Filter evaluated, no conditions met. |

### Interacting with the timeline

The `FilterEventTimeline` component in the Run Detail page renders each bar as
a clickable color-coded tick.

- **Hover** a tick to see a preview strip: bar timestamp, trigger state, and
  conditions-passed / conditions-failed counts.
- **Click** a tick to open the detail panel showing the full indicator
  snapshot (each indicator's value at that bar) and the suppression reason if
  applicable.
- A **legend** below the timeline strip decodes the five color states.

The `FilterSummaryPanel` below the timeline shows per-filter aggregate
rollups: bars scanned, wakeups, suppression breakdown by reason
(in-position / cooldown / daily-cap), and estimated LLM calls + tokens saved.

### Interpreting delayed decisions

In Live (forward-test) runs the engine tracks three counters on the
`eval_decisions` table, surfaced in `RunSummary`:

| Counter | Meaning |
|---|---|
| `skipped_dispatches` | Bars where the live executor skipped dispatch entirely (e.g. bar arrived too late to act). |
| `delayed_decisions` | Decisions that fired but on stale data — the bar age exceeded the configured cadence. |
| `forced_cancels` | Decisions that were in-flight when a stop-policy limit triggered, forcibly cancelled. |

In the **DecisionsTable** on the Run Detail page, delayed decisions are marked
with a `· delayed` badge. The CLI verbose output (`xvn eval run --verbose`)
flags delayed rows with a `(delayed)` marker in the decision log.

High `delayed_decisions` or `skipped_dispatches` counts indicate the live
data feed is lagging behind the strategy cadence. Check network latency to the
broker, bar aggregation delay, and the `--stale-data-max-age-ms` CLI flag (if
set below the bar interval, every bar may appear stale).

---

## Live / forward-test

Live mode is a **bounded paper-trading dress rehearsal** that streams live
market data through a strategy without real money. It terminates under a
configurable `StopPolicy` and leaves a backtestable artifact behind.
Internally the code refers to this as "Live" mode; "forward-test" and
"paper-trading" are synonyms.

### How to launch

Via CLI:

```bash
xvn eval run --mode live \
  --live-asset BTC/USD \
  --live-capital 10000 \
  --live-bar-limit 500 \
  --live-broker-creds-ref alpaca
```

Via dashboard: the Launch wizard accepts a `"mode": "live"` schema with the
same fields. The wizard validates the `LiveConfig` before submitting.

Alpaca credentials must be stored in the credential store (`xvn provider
add alpaca …` or the Provider settings tab). The `--live-broker-creds-ref`
flag selects which stored credential set to use. The engine probes
`GET /v2/account` at launch time to confirm reachability.

### Stop policy

The run terminates when the **first** limit is reached:

| Limit | Flag | Description |
|---|---|---|
| Bar limit | `--live-bar-limit` | Total bars consumed from the live stream. |
| Decision limit | `--live-decision-limit` | LLM dispatch count. |
| Time limit | `--live-time-limit-secs` | Wall-clock seconds since run start (capped at 30 days). |
| Trade limit | *(future)* | Completed filled-trade count. |

At least one limit must be set. All set limits must be > 0.

### Validation rules

`LiveConfig::validate()` enforces these rules at launch time (engine-side):

| # | Rule | Error variant |
|---|---|---|
| 1 | At least one asset specified. | `NoAssets` |
| 2 | Each asset is on the Alpaca crypto whitelist. | `AssetNotWhitelisted` |
| 3 | At least one stop-policy limit set. | `NoStopLimit` |
| 4 | `venue_label` is NOT `Live` (v1 rejects real money). | `VenueLabelMustBePaper` |
| 5 | Initial capital > 0. | `CapitalNotPositive` |
| 6 | Broker creds ref non-empty. | `BrokerCredsRefEmpty` |
| 7 | Display name non-empty. | `DisplayNameEmpty` |
| 8 | Time limit ≤ 30 days when set. | `TimeLimitExceedsMax` |
| 9 | Each stop-policy limit > 0 when set. | `StopLimitNotPositive` |

The broker-creds reachability check (`GET /v2/account`) is a **runtime** probe
at launch, not a `validate()` rule — it requires HTTP I/O. If it fails the
error is `BrokerCredsUnreachable`.

### Troubleshooting

**"BrokerCredsUnreachable" at launch.** The Alpaca endpoint
(`https://paper-api.alpaca.markets`) is unreachable from the dashboard host.
Check:
1. Network egress from the host — can it reach `api.alpaca.markets`?
2. API key / secret stored correctly? Run `xvn provider list` and verify the
   `alpaca` entry.
3. Alpaca paper account still active? Log into the Alpaca dashboard and
   confirm the paper account hasn't been closed or rate-limited.

**"AssetNotWhitelisted" for a supported asset.** The whitelist is in
`crates/xvision-engine/src/eval/live_config.rs`. Add the symbol and rebuild.
Alpaca crypto pairs use the `XXX/USD` format (e.g. `BTC/USD`, `ETH/USD`).

**Run starts but no bars consumed.** The live bar stream may be waiting for
the next bar boundary. Check the `--live-warmup-bars` flag — if set too high
the executor waits to accumulate warmup bars before dispatching. Also verify
the bar granularity (default: 1-minute) matches what Alpaca is streaming.

**Run terminates immediately.** The stop policy fired on the first bar. Check
that at least one limit is set to a reasonable value (> 1 bar, > 0 seconds).
A `--live-bar-limit 0` or `--live-time-limit-secs 0` terminates instantly.

**High delayed/skipped counts.** See [Interpreting delayed
decisions](#interpreting-delayed-decisions) in the Filter timeline section
above. The same counters apply — high values suggest the data feed is lagging.

**Live run left orphaned after disconnect.** Live runs started from the
dashboard persist across page reloads. Check the Eval Runs list for any run
stuck in `Running` status. Use `xvn eval cancel <run-id>` to terminate an
orphaned run, or let its stop policy fire naturally.

**VenueLabelMustBeLive** — you're trying to use real-money credentials with
the v1 live executor, which only supports Alpaca paper. This is a safety
lock. Real-money live execution is a future plan track.
---

## See also

- [Operator Manual](/docs?slug=operator-manual) — env-var setup and live-node remote control.
- [CLI Reference](/docs?slug=cli-reference) — full `xvn` command surface.
