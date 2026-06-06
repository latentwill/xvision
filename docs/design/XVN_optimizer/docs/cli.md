# xvn CLI reference

> Verbatim from `xvn --help` plus the high-traffic patterns. When in doubt, `xvn <subcommand> --help`.

**Binary** · `xvn` · **Crate** · `xvision-cli` · **Argv-only** — no shell text · **Updated** 2026-05-20.

## Context for AI agents

- **route**: `/docs/cli`
- **summary**: Reference for the xvn binary. 16 verbs. Argv-only contract — no shell text, no caller cwd, no caller env in v1. Strategy authoring writes ULID bundles. Eval runs persist to SQLite. Remote CLI runs through Tailscale-served dashboard, not arbitrary SSH.
- **key terms**: `ab-compare`, `eval run`, strategy bundle, ULID, `$XVN_HOME`, provider, backtest, RunMode, Tailscale
- **do not**: run `dashboard` or `mcp` via the remote-CLI job API (always rejected) · run `fire-trade` via remote CLI · run `cargo` on deploy hosts

## Top-level verbs

| Verb | Purpose |
|---|---|
| `ab-compare` | Run an N-arm backtest A/B comparison; emits `BacktestResult` JSON. |
| `metrics` | Pre-committed metrics (treatment vs baseline), JSON to stdout. |
| `gate` | Anti-overfit gate verdict for treatment vs baseline. |
| `report` | Headline Markdown report for a backtest run. |
| `show-metrics` | Render a `BacktestResult` JSON's headline numbers per arm. |
| `show-decision` | Pretty-print a cached `TraderDecision` by `cycle_id`. |
| `show-briefing` | Pretty-print a cached `InternBriefing` by `cycle_id`. |
| `run-setup` | Run a single setup through Intern → Risk slice. |
| `intern` / `trader` / `risk` | Stage in isolation (preview prompt or run a backend call). |
| `strategy` | Strategy authoring — `create`, `validate`, `ls`, `show`, `templates`, `run`. |
| `provider` | Manage registered LLM providers in `$XVN_HOME/config/default.toml`. |
| `store` | SQLite flight-recorder — `migrate`, `stats` on `$XVN_HOME/xvn.db`. |
| `indicator` | Compute one technical indicator from a JSON price/HLC series. |
| `dashboard` | Run the embedded web dashboard (axum + Vite SPA). |
| `eod` | End-of-day operator report (markdown to stdout). |
| `eval` | Launch, browse, compare, inspect eval runs. |
| `portfolio` | Read live portfolio state from a venue. |
| `fire-trade` | Manual single-trade smoke test against a live venue. |
| `close-position` | Close any open position in `--asset` at the given venue. |

## A/B compare · the headline call

```bash
xvn ab-compare \
  --cycles  path/to/cycles.json \
  --arm-a   baseline-config.toml \
  --arm-b   treatment-config.toml \
  --out     runs/headline-2026-05-11.json

# Then:
xvn show-metrics runs/headline-2026-05-11.json
xvn gate         runs/headline-2026-05-11.json
xvn report       runs/headline-2026-05-11.json > reports/headline_2026-05-11.md
```

**Rename heads-up.** The flag used to be `--setups`; it is `--cycles` now.

## Strategy authoring

Bundles persist at `$XVN_HOME/strategies/<agent_id>.json` where `agent_id` is a ULID. Bundles compose `AgentRefs` with a `PipelineDef` and a risk-config block; legacy fixed slots still parse.

```bash
xvn strategy templates                  # list bundled templates
xvn strategy templates --json           # machine-readable
xvn strategy new --name funding-fader --template mean_reversion
xvn strategy validate <ulid>
xvn strategy ls
xvn strategy show <ulid>
```

Skill authoring under `xvn skill …` was removed in ADR 0012 — the Agents page (`/agents`, `engine::agents`) is the canonical authoring path now.

## Eval & reports

```bash
xvn eval scenarios
xvn eval run --strategy <id> \
             --scenario crypto-bull-q1-2025 \
             --mode     backtest                # or `paper`
xvn eval list
xvn eval show     <run_id>
xvn eval compare  <run_id_a> <run_id_b>
xvn eval get      <run_id>                       # JSON detail
```

```bash
xvn report --run runs/headline-2026-05-11.json > reports/headline_2026-05-11.md
xvn eod > reports/eod-2026-05-11.md
```

Headline reports land in `reports/headline_<quant>/<date>.{json,md}` by convention.

## Providers

Providers are LLM endpoints in `$XVN_HOME/config/default.toml`. Secrets live separately under `$XVN_HOME/secrets/` — never committed.

```bash
xvn provider add \
  --name          claude \
  --kind          anthropic \
  --base-url      https://api.anthropic.com \
  --api-key-env   ANTHROPIC_API_KEY
xvn provider ls
xvn provider show   --name claude
xvn provider check  --name claude
xvn provider remove --name claude
```

| Field | Value |
|---|---|
| `--kind` | `openai-compat` · `anthropic` · `local` (candle, opt-in) |
| `--api-key-env` | name of an env var holding the secret; never the secret value |
| `--base-url` | full HTTP base, e.g. `https://openrouter.ai/api/v1` |
| config file | `$XVN_HOME/config/default.toml` |
| secrets dir | `$XVN_HOME/secrets/` · 0700 |

## Dashboard

```bash
xvn dashboard serve --bind 127.0.0.1:8788   # loopback: no token needed
xvn dashboard serve --bind 0.0.0.0:8788     # non-loopback: token required

# For non-loopback binds:
export XVN_DASHBOARD_TOKEN="$(openssl rand -hex 32)"
```

See [manual · dashboard auth](manual.md#dashboard-auth) for the full posture and token presentation channels.

## Remote CLI · Tailscale

Live-node command execution runs through the dashboard's typed remote CLI job API — not arbitrary SSH.

```bash
scripts/xvn-remote.py exec   eval run --strategy <id> \
                                       --scenario crypto-bull-q1-2025 \
                                       --mode backtest
scripts/xvn-remote.py submit eval list
scripts/xvn-remote.py status <job_id>
scripts/xvn-remote.py output <job_id>
scripts/xvn-remote.py cancel <job_id>
```

### Raw API contract

| Endpoint | Purpose |
|---|---|
| `POST /api/cli/jobs` | Submit a job with body `{ "argv": [...], "timeout_secs": 3600 }`. |
| `GET  /api/cli/jobs/:id` | Status: queued · running · ok · err · canceled. |
| `GET  /api/cli/jobs/:id/output` | Full stdout/stderr (tail-friendly). |
| `GET  /api/cli/jobs/:id/events` | SSE progress stream. |
| `POST /api/cli/jobs/:id/cancel` | Terminate a running job. |

### Rules

- **argv only** · no shell text, no string-cat command interpretation.
- **No caller-controlled cwd**.
- **No caller-controlled env** in v1.
- Subcommands `dashboard`, `mcp`, and `fire-trade` are **rejected even in devmode**.
- Default allowlist is small — currently just `bars fetch`. Set `XVN_DASHBOARD_CLI_DEVMODE=1` to opt back into permissive local-dev behavior.
- Trust boundary is Tailscale reachability for now.

**CLI devmode is not a substitute for auth.** Non-loopback binds still require `XVN_DASHBOARD_TOKEN` regardless of `XVN_DASHBOARD_CLI_DEVMODE`.

## Operator surfaces · live venues

```bash
xvn portfolio      --venue alpaca             # or orderly
xvn close-position --venue orderly --asset BTC
xvn fire-trade     --venue alpaca  --side buy --size-bps 100
```

## Exit codes

| Code | Meaning | When |
|---|---|---|
| `0` | ok | Command succeeded. |
| `1` | user | Bad argv, validation failure, missing required env. |
| `2` | notfound | Strategy ULID, scenario id, or run id not found. |
| `3` | provider | LLM provider returned non-2xx or unparseable response after retry. |
| `4` | venue | Broker rejected order; venue unreachable; portfolio query failed. |
| `5` | risk | Risk layer vetoed every candidate decision in the run. |
| `10` | internal | Bug. Capture the run, file an issue. |

---

Reconciled with `.claude/skills/xvision-cli/references/cli.md` at commit `a73b18f` on 2026-05-20.
