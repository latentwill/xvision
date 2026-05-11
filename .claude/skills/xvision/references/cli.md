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
| `skill` | Skill authoring (xvision-internal — NOT Claude Code skills) |
| `provider` | Manage registered LLM providers in `$XVN_HOME/config/default.toml` |
| `store` | SQLite flight-recorder (migrate / stats) on `$XVN_HOME/xvn.db` |
| `indicator` | Compute one technical indicator from a JSON price/HLC series |
| `dashboard` | Run the embedded web dashboard (axum + Vite SPA) |
| `eod` | End-of-day operator report (markdown to stdout) |
| `eval` | Browse eval runs and canonical scenarios |
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
xvn strategy new --name funding-fader --template trader-arm
xvn strategy validate --id <ulid>
xvn strategy ls
xvn strategy show <ulid>
```

Bundles persist at `$XVN_HOME/strategies/<agent_id>.json` (agent_id = ULID).

## Skill authoring (xvision-internal)

Read a markdown skill with YAML frontmatter, save to `$XVN_HOME/skills/`, attach to a bundle slot:

```bash
xvn skill new --from-file my-trader.md      # or `--from-file -` for stdin
xvn skill ls
xvn skill attach <agent_id> --slot trader --skill my-trader [--dry-run]
```

Slots: `regime` | `intern` | `trader`. Attach replaces prompt + model_requirement and unions allowed_tools. **Not** the same thing as a Claude Code skill — the consumer is xvision's own runtime sub-agents.

## Dashboard

```bash
xvn dashboard serve --bind 0.0.0.0:8788
```

SPA baked into the binary via `rust-embed` from `crates/xvision-dashboard/static/` (populated by `pnpm build` in `frontend/web/`). HTTP routes registered in `crates/xvision-dashboard/src/server.rs`.

## Providers

```bash
xvn provider add --name claude --kind anthropic --model claude-sonnet-4-6 --api-key-env ANTHROPIC_API_KEY
xvn provider ls
xvn provider set-default --name claude --model claude-sonnet-4-6
xvn provider rm --name claude
```

Writes to `$XVN_HOME/config/default.toml`. Secrets live separately under `$XVN_HOME/secrets/`.

## Reports + EOD

```bash
xvn report --run runs/headline-2026-05-11.json > reports/headline_2026-05-11.md
xvn eod > reports/eod-2026-05-11.md
```

Headline reports land in `reports/headline_<quant>/<date>.{json,md}` by convention.
