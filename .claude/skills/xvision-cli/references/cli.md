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
| `strategy` | Strategy authoring (`new`, `validate`, `ls`, `show`, `templates`, `add-agent`, `remove-agent`, `set-pipeline`, `migrate-agents`, `run`) |
| `scenario` | Scenario authoring (`create`, `ls`, `show`, `clone`, `validate`, `archive`, `rm`, `tree`, `inspect`, `select`, `classify`, `set-regime`) |
| `eval` | Eval runs (`run`, `list`, `show`, `results`, `watch`, `scenarios`, `compare`, `validate`, `attest`, `export`, `review`, `batch`) |
| `experiment` | Experiment ledger (`new`/`create`, `ls`, `show`/`get`, `update`, `run`) |
| `agent` | Inspect agent records from the workspace agent library (`get`/`show`) |
| `provider` | LLM providers (`list`, `show`, `check`, `add`, `remove`, `refresh-models`, `models`) |
| `store` | SQLite flight-recorder (migrate / stats) on `$XVN_HOME/xvn.db` |
| `obs` | Agent-run observability (`retention`, `janitor`) |
| `run` | `run inspect <run_id>` → materialize `xvn_run.json` + `xvn_report.md` |
| `indicator` | Compute one technical indicator from a JSON price/HLC series |
| `bars` | SQLite-cached historical bars (`fetch`, `ls`, `rm`, `gc`) |
| `dashboard` | Run the embedded web dashboard (axum + Vite SPA) |
| `eod` | End-of-day operator report (markdown to stdout) |
| `doctor` | Inspect effective `$XVN_HOME` / config / db / provider / template targets |
| `migrate` | Apply pending migrations + seed; `--dry-run` to report state |
| `example` | Seed curated example strategies, scenarios, tutorial artifacts |
| `portfolio` | Read live portfolio state from a venue |
| `fire-trade` | Manual single-trade smoke test against a live venue |
| `close-position` | Close any open position in `--asset` at the given venue |

## A/B compare — the headline call

```bash
# Required: cycles drive Trader / baseline tick-by-tick. Bars come from
# either a JSON file (--bars) or the SQLite cache (--from / --to / --granularity).
xvn ab-compare \
  --cycles path/to/cycles.json \
  --from 2025-01-01 --to 2025-04-01 --granularity 1h \
  --arms "trader_arm,buy_and_hold,rsi_mean_reversion" \
  --asset BTC \
  --output runs/headline-2026-05-20.json
```

Pre-rename heads-up: this used to be `--setups`; it's `--cycles` now.

Heads include: `trader_arm`, `buy_and_hold`, `always_long`, `always_short`,
`random_direction:seed=<u64>`, `rsi_mean_reversion`,
`ma_crossover:fast=<usize>:slow=<usize>`, `macd_momentum`. Empty `--arms`
selects `default_arms()` (trader_arm + buy_and_hold).

## Strategy authoring

```bash
# Template mode — pick a starter and rename it
xvn strategy new --name funding-fader --template mean_reversion
xvn strategy validate <ulid>
xvn strategy ls
xvn strategy show <ulid>

# Atomic mode — create Strategy + Agent + provider/model binding in one call.
# Emits {"strategy_id","agent_id","eval_ready","provider","model","warnings"}
# when --json is set.
xvn strategy new \
  --prompt prompts/trader.md \
  --name funding-fader --provider openrouter --model kimi-k2 \
  --role trader --asset ETH/USD --timeframe 1h \
  --json

# Attach a Hypothesis (any flag triggers Hypothesis attachment)
xvn strategy new --prompt prompts/trader.md \
  --name reg-breakout --provider anthropic --model claude-haiku-4-5-20251001 \
  --role trader --asset ETH/USD --timeframe 1h \
  --family compression-breakout \
  --hypothesis "Post-compression range breakouts persist for 4–8 bars" \
  --target-regime "post-compression trend" \
  --avoid-regime chop \
  --json

# Agent-composition mutations on an existing Strategy
xvn strategy add-agent <strategy_id> --agent <agent_id> --role trader
xvn strategy remove-agent <strategy_id> --role trader
xvn strategy set-pipeline <strategy_id> --kind <kind>
xvn strategy migrate-agents <strategy_id>   # convert legacy slot-shaped Strategy
xvn strategy filter-catalog --json          # machine-readable Filter DSL catalog
xvn strategy set-filter <strategy_id> --from-json filter.json
```

Strategy artifacts persist at `$XVN_HOME/strategies/<agent_id>.json`
(`agent_id` = ULID).

Dashboard inspector notes:

- `/strategies/:id` is the canonical inspector route; `/authoring/:id` is an old compatibility alias.
- Manifest display name, summary, asset universe, and cadence are editable in the inspector.
- Strategy ID is stable/read-only and should be shown explicitly when reporting QA.
- Eval readiness validation is not auto-run on first page load. Use **Check eval readiness** or `xvn strategy validate`.
- Mechanical params are no longer an operator tuning panel in the inspector.

Filter QA notes:

- A real XVN filter is a saved strategy filter artifact. Prompt wording that says "filter" is not enough.
- Use the inspector Filter card or the supported strategy filter CLI/API path, then confirm eval detail has `filter_events` / `filter_summaries` when the filter should participate.
- Eval result rows can be synthesized by `noop_skip`, graph gating, or early-stop inheritance. Separate those from direct model decisions before drawing conclusions.

Reusable prompt authoring used to live under `xvn skill …` (Plan 2b). That surface was removed in ADR 0012 — the Agents page (`/agents`, `engine::agents`) is now the canonical authoring path. See `decisions/0012-deprecate-in-app-skills.md`.

### Inline deterministic Filter DSL

Use `xvn strategy filter-catalog --json` as the canonical source before
building a filter payload. Important current tokens:

- operators: `>`, `<`, `>=`, `<=`, `==`, `between`, `crosses_above`,
  `crosses_below`, `above_for_<bars>`, `below_for_<bars>`,
  `crossed_above_<bars>`, `crossed_below_<bars>`, `slope_gt_<bars>`,
  `slope_lt_<bars>`, `zscore_gt_<period>`, `zscore_lt_<period>`,
  `within_pct_<pct>`
- trend/regime: `adx_<period>`, `di_plus_<period>`,
  `di_minus_<period>`, `highest_<period>`, `lowest_<period>`
- session/volume: `opening_range_high_<minutes>`,
  `opening_range_low_<minutes>`, `prev_day_*`, `prev_week_*`,
  `prev_month_*`, `rvol_tod_<period>`, `volume_zscore_<period>`
- optional trigger metadata: `fire.reason`, `fire.priority`,
  `fire.tags`, `fire.context`

Example:

```json
{
  "display_name": "LLM trend breakout fire",
  "asset_scope": ["BTC/USD"],
  "timeframe": "15m",
  "conditions": {
    "all": [
      { "lhs": "adx_14", "op": ">", "rhs": 25 },
      { "lhs": "di_plus_14", "op": "above_for_3", "rhs": "di_minus_14" },
      { "lhs": "close", "op": "crossed_above_3", "rhs": "opening_range_high_30" },
      { "lhs": "rvol_tod_20", "op": ">", "rhs": 1.5 }
    ]
  },
  "fire": {
    "reason": "trend_breakout",
    "priority": 0.85,
    "tags": ["trend", "breakout", "volume"],
    "context": ["close", "opening_range_high_30", "adx_14", "rvol_tod_20"]
  },
  "cooldown_bars": 16
}
```

## Dashboard

```bash
xvn dashboard serve --bind 0.0.0.0:8788
```

SPA baked into the binary via `rust-embed` from `crates/xvision-dashboard/static/` (populated by `pnpm build` in `frontend/web/`). HTTP routes registered in `crates/xvision-dashboard/src/server.rs`.

## Scenario authoring

```bash
xvn scenario create --from path/to/scenario.toml
xvn scenario validate --from path/to/scenario.toml   # dry-run, no write
xvn scenario ls
xvn scenario show <id>
xvn scenario clone <id> --name <new_name>
xvn scenario tree <id>                                # ancestors + immediate children
xvn scenario inspect <id> --card                      # compact plain-text card

# Comparable set query — read-only, no writes
xvn scenario select --asset ETH/USD --timeframe 60 --count 4

# Regime labeling
xvn scenario classify --all                           # auto-derive labels from bars
xvn scenario classify <id> --force                    # overwrite operator labels
xvn scenario set-regime <id> --regime expansion \
  --volatility high --direction up                    # operator-authored labels
```

Label vocabulary: `regime ∈ {trend, chop, crash, expansion, recovery}`,
`volatility ∈ {low, normal, high, extreme}`,
`direction ∈ {up, down, sideways}`.

## Eval

```bash
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest
xvn eval run --strategy <id> --scenario crypto-bull-q1-2025 --mode backtest --auto-fire-review --max-review-annotations 8
xvn eval list
xvn eval show <run_id>
xvn eval results <run_id>
xvn eval watch <run_id>
xvn eval scenarios                                    # canonical scenarios in the binary

# Compare 2+ completed runs. --markdown drops a PR-ready table.
# --batch resolves ids from a persisted eval batch.
xvn eval compare <run_id_a> <run_id_b> --markdown --sort sharpe
xvn eval compare --batch <batch_id> --markdown

# Multi-scenario batch in one invocation
xvn eval batch --strategy <id> --scenarios sc_a,sc_b,sc_c --wait

# Validate / attest / export / review
xvn eval validate --strategy <id> --scenario <id>     # preflight without launching
xvn eval attest <run_id>                              # sign + persist EvalAttestation
xvn eval export <run_id> --output exports/<run>.json  # canonical EvalRunExport (q15 §3)
xvn eval review <run_id>                              # analytical review
```

Review annotations are persisted on completed reviews and rendered by
`/charts/annotated?run_id=<run_id>`. Use `xvn eval show <run_id>` to confirm
whether a run was launched with `auto_review true`.

`eval compare --markdown` includes a Baseline (buy_hold) column,
backed by baseline auto-comparison in `BacktestExecutor`.

For filter-functionality QA, compare aggregate metrics plus decision
divergence, filter events/summaries, and synthesized-decision counts.
Identical headline metrics alone do not prove the filter is ineffective.

## Experiment ledger

The experiment ledger groups a research question + a strategy + the
scenarios that question demands. `experiment run` orchestrates the
whole loop in one call.

```bash
xvn experiment new --name reg-breakout-eth-q1 \
  --question "Does the breakout edge survive across 1h ETH regimes?" \
  --strategy <strategy_id>
xvn experiment ls
xvn experiment show <experiment_id>
xvn experiment update <experiment_id> --question "..."

# Orchestrate: pick scenarios → run batch → bind run ids → write result_json.
# Selector mode (when --scenarios is omitted):
xvn experiment run \
  --name reg-breakout-eth-q1 \
  --question "Does the breakout edge survive across 1h ETH regimes?" \
  --strategy <strategy_id> \
  --assets ETH/USD --timeframe 60 --count 4 \
  --target-decisions 200 \
  --regimes "post-compression trend,expansion" \
  --wait --compare --markdown --output reports/exp.md
```

`--decision-budget` is metadata only — it records operator intent
("this experiment was designed around N decisions per scenario") so
cross-experiment comparison is meaningful. It does **not** cap eval
execution; the underlying pipeline still runs every cadence-gated
decision per scenario.

## Agent records

```bash
xvn agent get <agent_id>   # workspace agent library; shape matches EvalRunExport.agents[]
```

## Agent-run observability

```bash
xvn obs retention                  # inspect / edit retention policy
xvn obs janitor                    # run TTL + max-bytes pass
xvn run inspect <run_id>           # materialize xvn_run.json + xvn_report.md
```

## Providers

```bash
xvn provider add --name claude --kind anthropic --base-url https://api.anthropic.com --api-key-env ANTHROPIC_API_KEY
xvn provider list                              # all registered providers
xvn provider show --name claude
xvn provider check --name claude               # probe reachability
xvn provider remove --name claude              # refused if any slot references it
xvn provider refresh-models --name openrouter  # hit /v1/models, write to disk
xvn provider refresh-models                    # all providers
xvn provider models --name openrouter          # cached catalog only (no network)
```

Writes to `$XVN_HOME/config/default.toml`. Secrets live separately under `$XVN_HOME/secrets/`.

Do not dedupe / normalize provider model lists across providers — fix
rendering instead. Each provider's catalog is shown verbatim.

## MCP tool peers for new CLI verbs

When an MCP client is already attached, prefer these over
`POST /api/cli/jobs` shell-outs — they wrap the same engine APIs the
CLI calls.

| MCP tool | CLI equivalent |
|---|---|
| `xvn_strategy_create_atomic` | `xvn strategy new --prompt …` (atomic mode) |
| `xvn_strategy_validate_preflight` | `xvn strategy validate <id>` (returns `eval_ready` + warnings/errors) |
| `xvn_eval_batch_run` | `xvn eval batch --strategy <id> --scenarios …` |
| `xvn_eval_compare_report` | `xvn eval compare …` decorated with behavior summary per row |
| `xvn_scenario_inspect_card` | `xvn scenario inspect <id> --card` |
| `xvn_eval_behavior` | on-demand `BehaviorSummary` for a finished run |

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
- normal operator/eval/research commands are supported without a dev-mode bypass
- reject server/live-trading heads such as `dashboard`, `mcp`, `fire-trade`, and `close-position`
- reject destructive/admin nested forms such as `bars rm`, `bars gc`, provider config mutations,
  scenario/strategy authoring mutations, `example seed`, `store migrate`, and observability
  retention/janitor writes
- trust boundary is Tailscale reachability plus dashboard auth when bound outside loopback
