# xvn CLI reference

Verbatim from `xvn --help` plus the high-traffic patterns. When in doubt, `xvn <subcommand> --help`.

## Top-level

```
xvn <COMMAND>
```

| Verb | Purpose |
|---|---|
| `ab-compare` | Run an N-arm backtest A/B comparison; emits `BacktestResult` JSON |
| `metrics` | Pre-committed metrics (treatment vs baseline), JSON to stdout |
| `gate` | Anti-overfit gate verdict for treatment vs baseline |
| `report` | Headline Markdown report for a backtest run |
| `show-metrics` | Render a `BacktestResult` JSON's headline numbers per arm |
| `show-decision` | Pretty-print a cached `TraderDecision` by `cycle_id` |
| `show-briefing` | Pretty-print a cached `InternBriefing` by `cycle_id` |
| `run-setup` | Run a single setup through Intern → Risk slice |
| `intern` / `trader` / `risk` | Stage in isolation (preview prompt or run a backend call) |
| `strategy` | Strategy authoring (create / validate / ls / show / templates / run) |
| `provider` | Manage registered LLM providers in `$XVN_HOME/config/default.toml` |
| `store` | SQLite flight-recorder (migrate / stats) on `$XVN_HOME/xvn.db` |
| `indicator` | Compute one technical indicator from a JSON price/HLC series |
| `dashboard` | Run the embedded web dashboard (axum + Vite SPA) |
| `eod` | End-of-day operator report (markdown to stdout) |
| `eval` | Launch, browse, compare, and inspect eval runs |
| `portfolio` | Read live portfolio state from a venue |
| `fire-trade` | Manual single-trade smoke test against a live venue |
| `close-position` | Close any open position in `--asset` at the given venue |

## A/B compare — the headline call

```bash
xvn ab-compare \
  --cycles path/to/cycles.json \
  --arm-a baseline-config.toml \
  --arm-b treatment-config.toml \
  --out runs/headline-2026-05-11.json
```

Pre-rename heads-up: this used to be `--setups`; it's `--cycles` now.

## Strategy authoring

```bash
xvn strategy new --name funding-fader --template mean_reversion
xvn strategy validate <ulid>
xvn strategy ls
xvn strategy show <ulid>
```

Bundles persist at `$XVN_HOME/strategies/<agent_id>.json` (agent_id = ULID).

Reusable prompt authoring used to live under `xvn skill …` (Plan 2b). That surface was removed in ADR 0012 — the Agents page (`/agents`, `engine::agents`) is now the canonical authoring path. See `decisions/0012-deprecate-in-app-skills.md`.

## Dashboard

```bash
xvn dashboard serve --bind 0.0.0.0:8788
```

SPA baked into the binary via `rust-embed` from `crates/xvision-dashboard/static/` (populated by `pnpm build` in `frontend/web/`). HTTP routes registered in `crates/xvision-dashboard/src/server.rs`.

## Eval

```bash
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval list
xvn eval show <run_id>
xvn eval compare <run_id_a> <run_id_b>
```

## Providers

```bash
xvn provider add --name claude --kind anthropic --base-url https://api.anthropic.com --api-key-env ANTHROPIC_API_KEY
xvn provider ls
xvn provider show --name claude
xvn provider check --name claude
xvn provider remove --name claude
```

Writes to `$XVN_HOME/config/default.toml`. Secrets live separately under `$XVN_HOME/secrets/`.

## Reports + EOD

```bash
xvn report --run runs/headline-2026-05-11.json > reports/headline_2026-05-11.md
xvn eod > reports/eod-2026-05-11.md
```

Headline reports land in `reports/headline_<quant>/<date>.{json,md}` by convention.

## Remote CLI over Tailscale

Use this when driving a live node over `xvn.tail2bb69.ts.net` or `xvnej.tail2bb69.ts.net`.

### Helper script

```bash
scripts/xvn-remote.py exec eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
scripts/xvn-remote.py submit eval list
scripts/xvn-remote.py status <job_id>
scripts/xvn-remote.py output <job_id>
scripts/xvn-remote.py cancel <job_id>
```

### Raw API contract

- `POST /api/cli/jobs` with JSON body `{ "argv": ["eval", "run", ...], "timeout_secs": 3600 }`
- `GET /api/cli/jobs/:id`
- `GET /api/cli/jobs/:id/output`
- `GET /api/cli/jobs/:id/events`
- `POST /api/cli/jobs/:id/cancel`

Rules:

- argv only; no shell text
- no caller-controlled cwd
- no caller-controlled env in v1
- reject `dashboard` and `mcp` argv
- trust boundary is Tailscale reachability for now
