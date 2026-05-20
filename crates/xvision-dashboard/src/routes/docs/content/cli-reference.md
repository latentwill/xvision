# CLI Reference

`xvn` is the single-binary CLI that drives the xvision eval platform. Every
operation available through the dashboard has a corresponding `xvn` subcommand;
the dashboard is a typed shell over the same engine. The binary is also the
primary automation surface for agents running research loops, batch evals, and
experiment orchestration.

## Headline verbs

### `xvn strategy …`

| Verb | Effect |
|---|---|
| `ls [--json]` | List saved strategy ids from `$XVN_HOME/strategies/`. |
| `show <id> [--format json\|json-compact]` | Print a strategy as JSON; alias `get`. |
| `templates [--json]` | List available strategy templates from the registry. |
| `create --template <name> --name <name> [--provider <p> --model <m>]` | Seed a strategy draft from a template; alias `new`. |
| `create --from-file <path>` | Load and persist a strategy from a JSON or TOML file. |
| `create --prompt <path> --name <name> --provider <p> --model <m> --asset <sym> --timeframe <tf>` | Atomic mode: reads the prompt file, creates a library agent, and wires it into a new strategy in one command. Emits `{strategy_id, agent_id, eval_ready, provider, model, warnings}` when `--json` is set. |
| `validate <id> [--scenario <id>] [--json]` | Shape-only check without `--scenario`; full preflight with `--scenario` (checks agents, provider/model, asset/timeframe alignment). Returns `{eval_ready: bool, warnings: [], errors: [], expected_decisions, asset, timeframe, warmup_bars}`. Non-zero exit when not eval-ready. |
| `add-agent <strategy-id> <agent-id> --role <role>` | Attach a library agent reference to a strategy. |
| `remove-agent <strategy-id> --role <role>` | Detach an agent reference by role. |
| `set-pipeline <strategy-id> --kind single\|sequential\|graph [--edge from:to …]` | Set the strategy pipeline shape; repeat `--edge` for graph edges. |
| `run <id> --fixture <name> [--decisions <n>] [--mock]` | Run a strategy inline against a fixture parquet; `--mock` uses deterministic dispatch with no API calls. |
| `migrate-agents [--dry-run]` | Migrate legacy slot-shaped strategies into agent references; `--dry-run` previews without writing. |

**Atomic-mode hypothesis flags** (usable with `create --template` or `create --from-file`):

| Flag | Purpose |
|---|---|
| `--family <label>` | Hypothesis family / template label (e.g. `compression-breakout`). |
| `--hypothesis <text>` | One-to-two sentence hypothesis statement. |
| `--target-regime <val>` | Regime the strategy targets; repeatable. |
| `--avoid-regime <val>` | Regime the strategy should avoid; repeatable. |
| `--hypothesis-file <path>` | Load a complete Hypothesis JSON/YAML object; overrides individual flags. |

When any hypothesis flag is provided, a `Hypothesis` struct is attached to the
strategy before saving. Accepted timeframes for `--timeframe`: `1m`, `5m`,
`15m`, `30m`, `1h`, `2h`, `4h`, `1d`.

Note: the `intern` slot role is being renamed to "default agent" during the
current crossover period. New strategies use `AgentRef` entries; legacy slot
fields are migrated with `migrate-agents`.

---

### `xvn scenario …`

| Verb | Effect |
|---|---|
| `ls [--source canonical\|user\|clone\|generated] [--tag <tag>] [--archived] [--json]` | List scenarios, newest first; archived excluded by default. |
| `show <id> [--toml] [--format json\|json-compact]` | Print a scenario; alias `get`. `--toml` emits `CreateScenarioRequest` shape suitable for `--from-file`. |
| `create --name <name> --asset <sym> --from <date> --to <date> [--granularity <g>] [--from-file <path>] [--json]` | Create a scenario; granularity defaults to `1h`. |
| `clone <id> [--name <n>] [--from <date>] [--to <date>] [--asset <sym>] [--warmup-bars <n>]` | Clone a scenario, optionally overriding fields. |
| `validate --from-file <path> [--json]` | Dry-run validate a `CreateScenarioRequest` TOML without persisting. |
| `archive <id>` | Soft-delete (archive) a scenario by id. |
| `rm <id>` | Hard-delete a scenario (blocked when eval runs reference it). |
| `tree <id>` | Print the lineage tree for a scenario (ancestors + immediate children). |
| `inspect <id> --card` | Print a compact plain-text summary card; `--card` is required. |
| `select [--assets <sym,…>] [--timeframe <tf>] [--target-decisions <n>] [--same-decisions --max-decisions <n>] [--regimes <r,…>] [--count <n>] [--json]` | Stateless read-only filter: returns a comparable set of scenarios by asset, timeframe, and decision count. Two modes: `--target-decisions` (within ±10 %) or `--same-decisions --max-decisions` (largest common count). |
| `classify [<id>] [--all] [--force]` | Auto-derive `regime_label`, `volatility_label`, and `trend_direction` from the bar window. Skips operator-set labels unless `--force` is given. |
| `set-regime <id> [--regime <label>] [--volatility <label>] [--direction <label>]` | Set operator-authored regime labels (`regime_derived = false`). Omitting a flag leaves the existing value unchanged. |

---

### `xvn eval …`

| Verb | Effect |
|---|---|
| `run --strategy <id> --scenario <id> [--mode paper\|backtest] [--json]` | Queue and execute an eval run. |
| `list [--strategy <id>] [--scenario <id>] [--status <s>] [--json]` | List recent runs, most-recent first. Status values: `queued\|running\|completed\|failed\|cancelled`. |
| `show <run-id> [--behavior] [--json]` | Print a single run; alias `get`. `--behavior` computes and appends a behavior summary (flat_rate, avg_bars_held, primary_failure_mode, …). |
| `results <run-id> [--json]` | Alias for `show`; returns the same run shape. |
| `watch <run-id> [--interval-secs <n>] [--once] [--json]` | Poll a run until terminal state; `--once` polls once then exits. |
| `compare <run-id> … [--runs r1,r2] [--batch <id>] [--json] [--markdown] [--sort return\|sharpe\|drawdown]` | Side-by-side metrics and equity diff for 2+ runs. `--markdown` (alias `--md`) emits a GitHub-flavoured table. `--batch` resolves run ids from a persisted eval batch. |
| `export <run-id> [--output <path>] [--pretty]` | Export a completed run as `EvalRunExport` JSON (q15 §3). Writes to stdout by default. |
| `validate --strategy <id> --scenario <id> [--mode paper\|backtest] [--json]` | Validate an eval request without launching it. |
| `attest <run-id> [--json]` | Sign and persist an `EvalAttestation` for a completed run. |
| `review <run-id> --agent <profile> [--force] [--format human\|json] [--output <path>]` | Generate an analytical review of a completed run using the named agent profile. Idempotent: a prior failed review is retried; a completed review is returned as-is unless `--force` is set. |
| `batch run --strategy <id> --scenarios <id,…> [--mode backtest\|paper] [--wait] [--review-with <profile>] [--json]` | Launch one eval run per scenario, block until all reach terminal state (`--wait`), and return a unified `BatchResult`. When `--review-with <profile>` is set (requires `--wait`), a review is generated for each completed run using the named agent profile; failures are captured per-run and do not abort the batch. |
| `batch status <batch-id> [--json]` | Show the persisted batch row and its attached run ids. |

The `batch compare` workflow: run `eval batch run … --json` to get a `batch_id`,
then pass `--batch <batch_id>` to `eval compare` to resolve run ids automatically.

---

### `xvn experiment …`

Experiments group a research question across a set of strategies and scenarios.
They persist to the `experiments` table and can be bound to a batch after the
fact.

| Verb | Effect |
|---|---|
| `create --name <name> --strategy <id> --scenarios <id,…> [--question <text>] [--decision-budget <n>] [--json]` | Create a new experiment in the ledger; alias `new`. |
| `ls [--json]` | List all experiments, most-recent first. |
| `show <id> [--json]` | Show a single experiment by id; alias `get`. |
| `update <id> [--conclusion <text>] [--next-recommendation <text>] [--bind-batch <batch-id>] [--json]` | Apply partial mutations; at least one flag is required. |
| `run --name <name> --strategy <id> [--scenarios <id,…>] [--assets <a,…> --timeframe <tf> --target-decisions <n>] [--decision-budget <n>] [--wait] [--review-with <profile>] [--compare [--markdown]] [--json]` | Full orchestration in one command: create experiment → run batch → bind → write `result_json`. Scenario selection either via explicit `--scenarios` or via the scenario selector flags. Emits `{experiment_id, strategy_ids, scenario_ids, batch_id, result: {profitable_count, best_scenario, worst_scenario, runs}, compare_markdown?}` when `--json` is set. |

---

### `xvn agent get <agent-id>`

Read path into the workspace agent library. Returns the full `Agent` object
(same JSON shape as the `agents[]` slot in `EvalRunExport`), including all
slots with resolved `max_tokens`. Format flag: `--format json` (default,
pretty-printed) or `json-compact` (single-line, suitable for piping). Alias:
`show`. List is intentionally out of scope in this release.

---

### `xvn ab-compare`

N-arm backtest workhorse. Each arm carries a strategy + optional
`intern=<provider>/<model>` and `trader=<provider>/<model>` overrides; arms
resolving to the same `(provider, model)` share one HTTP client.

Key flags: `--cycles <path>` (JSON `Vec<MarketSnapshot>`), `--bars <path>` or
`--from <date> --to <date> [--granularity <g>]` (cache-backed via `bars_cache`),
`--arms <spec,…>`, `--output <path>`.

The `--cycles` flag controls the per-arm decision input. The flag was renamed
from `--setups` before the wave A shipment; any scripts using `--setups` must
be updated.

---

### `xvn provider …`

| Verb | Effect |
|---|---|
| `list` | List all registered providers from `$XVN_HOME/config/default.toml`. |
| `show --name <name>` | Show one provider in full. |
| `check --name <name> [--probe]` | TCP-connect smoke test; `--probe` sends a real `/models` request. |
| `add --name <n> --kind <k> --base-url <url> [--api-key-env <env>] [--api-key <key>]` | Register a new provider. Kind values: `anthropic`, `openai-compat`, `local-candle`. |
| `remove --name <name>` | Remove a provider; refused if any slot references it. |
| `refresh-models [--name <name>]` | Hit `/v1/models` and write the catalog to disk; omit `--name` to refresh all. |
| `models --name <name>` | Print the cached model catalog for a provider (does not fetch). |

---

### Other verbs

| Verb | Effect |
|---|---|
| `xvn metrics --report <path> --treatment <arm> --baseline <arm>` | Compute pre-committed metrics from a `BacktestResult` JSON; emits JSON. |
| `xvn gate --report <path> --treatment <arm> --baseline <arm>` | Apply the anti-overfit verdict to pre-committed metrics; emits JSON. |
| `xvn store migrate [--db <path>]` | Open the SQLite flight recorder and apply pending migrations. |
| `xvn store stats [--db <path>]` | Print row counts per table in the flight recorder. |
| `xvn eod [--hours <n>]` | End-of-day operator report as markdown to stdout (default window: 24 h). |
| `xvn doctor [--json]` | Config / DB / provider health check; lists paths, template registry, and secret-file presence. |

---

## `--json` output

Every list, get, create, run, validate, and batch verb accepts `--json` (or for
`strategy show`, `--format json`). JSON output emits stable machine-readable
fields safe for chaining in scripts and agent automation loops. Key contracts:

- `xvn strategy validate … --json` → `{eval_ready: bool, warnings: [], errors: [], expected_decisions, asset, timeframe, warmup_bars}`
- `xvn strategy create … --json` (atomic mode) → `{strategy_id, agent_id, eval_ready, provider, model, warnings}`
- `xvn eval batch run … --json` → `{batch_id, strategy_id, runs: [{scenario_id, run_id, status, return_pct, sharpe, drawdown_pct, decisions, actions, review?}]}`
- `xvn experiment run … --json` → `{experiment_id, name, strategy_ids, scenario_ids, batch_id, result, compare_markdown?}`
- `xvn scenario select … --json` → `[{id, name, asset, timeframe, decision_count}]`

---

## Exit codes

`xvn` exits with a typed code that automation can dispatch on without parsing
error text (see `crate::exit::XvnExit`):

| Code | Name | Meaning |
|---|---|---|
| 0 | Success | Command completed normally. |
| 2 | Usage | Caller-fixable: bad flag, malformed input, validation drift, not eval-ready. |
| 3 | Auth | Missing or invalid credential (e.g. `ANTHROPIC_API_KEY` unset). |
| 4 | NotFound | Referenced resource does not exist (strategy id, run id, scenario id, agent id). |
| 5 | Upstream | LLM API / broker / network / file system / database error. |
| 7 | Conflict | State collision (e.g. duplicate name on rename or create). |

`xvn strategy validate` exits non-zero (code 2) when `eval_ready` is false,
making it safe to use as a gate in a shell pipeline.

---

## Where things live

`XVN_HOME` defaults to `~/.xvn` and is honored by every subcommand. Set it
explicitly with the `XVN_HOME` env var or the per-command `--xvn-home` flag.

| Path | Contents |
|---|---|
| `$XVN_HOME/strategies/<id>.json` | Serialised `Strategy` objects. |
| `$XVN_HOME/xvn.db` | SQLite flight recorder: runs, decisions, batches, experiments, reviews, equity, agents. |
| `$XVN_HOME/bars/<asset>/<granularity>.json` | Bars cache written by `xvn bars fetch` and the `ab-compare` date-range path. |
| `$XVN_HOME/config/default.toml` | Runtime config: providers, intern/trader defaults, backtest params. Override path: `XVN_CONFIG_PATH`. |
| `$XVN_HOME/secrets/providers.toml` | Provider API keys (separate from config; checked by `xvn doctor`). |
| `$XVN_HOME/secrets/brokers.toml` | Broker credentials (checked by `xvn doctor`). |
| `$XVN_HOME/identity/signing.key` | Ed25519 key used by `xvn eval attest`. Auto-generated on first use. |
