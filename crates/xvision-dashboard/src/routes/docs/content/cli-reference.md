# CLI Reference

`xvn` is the single-binary CLI that backs the dashboard. Anything you
can do via the UI you can do via `xvn`; the dashboard is just a
typed wrapper around the same engine.

## Headline verbs

### `xvn strategy …`

| Verb | Effect |
|---|---|
| `ls` | List saved strategies in `$XVN_HOME/strategies/`. |
| `show <id>` | Print a single strategy as JSON. |
| `new --template <name>` | Seed a draft from a starter template. |
| `add-agent --id <strategy> --agent <agent-id> --role <role>` | Attach an AgentRef. |
| `remove-agent --id <strategy> --role <role>` | Detach an AgentRef. |
| `set-pipeline --id <strategy> --kind sequential` | Switch pipeline shape. |
| `validate <id>` | Run the dashboard validator; non-zero exit on drift. |
| `run --strategy <id> --scenario <id> [--mode backtest\|paper]` | Skip the dashboard, drive the eval directly. |
| `migrate-agents <id>` | Migrate legacy slot fields into AgentRefs. |

### `xvn scenario …`

| Verb | Effect |
|---|---|
| `ls` | List saved scenarios. |
| `show <id>` | Print a single scenario. |
| `create --from-file <path>` | Create from JSON/TOML. |
| `validate --from-file <path>` | Dry-run validate without persisting. |

### `xvn eval …`

| Verb | Effect |
|---|---|
| `run --strategy <id> --scenario <id> --mode backtest` | Queue + execute a run. |
| `list` | Pretty list of recent runs. |
| `get <run-id>` | Print one run's summary + metrics. |
| `watch <run-id>` | Tail SSE progress until terminal. |
| `compare <run-id> <run-id> …` | Side-by-side metrics + equity diff. |

### `xvn ab-compare`

The N-arm backtest workhorse. Each arm is a strategy + arm
configuration; results emit a `BacktestResult` JSON for downstream
analysis. The `--cycles` flag controls the per-arm decision count
(was `--setups` pre-rename).

### `xvn provider …`

`xvn provider ls` / `xvn provider test <name>` to manage the
`$XVN_HOME/config/default.toml` provider section. Default provider
selection follows the same provider-aware posture as the chat rail
and review-agent paths.

### Other

- `xvn metrics` / `xvn gate` — pre-committed metrics + anti-overfit
  verdict.
- `xvn store migrate` / `xvn store stats` — SQLite flight-recorder
  ops (`xvn.db`).
- `xvn eod` — end-of-day operator report (markdown to stdout).
- `xvn doctor` — config / DB / provider health check.

## `--json` output

List/get/create/run verbs accept `--json` (or `--format json`) and
emit stable machine-readable fields safe for chaining in scripts.

## Where things live

- Strategies: `$XVN_HOME/strategies/<id>.json`
- Scenarios: `$XVN_HOME/scenarios/<id>.json`
- Database: `$XVN_HOME/xvn.db` (SQLite flight recorder).
- Bars cache: `$XVN_HOME/bars/<asset>/<granularity>.json`.
- Config: `$XVN_HOME/config/default.toml`.

`XVN_HOME` defaults to `~/.xvn` and is honored by every subcommand;
no hidden fallback to a baked-in default.
