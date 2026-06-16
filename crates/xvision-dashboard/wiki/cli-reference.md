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
| `set-filter <strategy-id> --from-json <path>` | Install an inline deterministic Filter DSL payload and switch the strategy to `filter_gated`. See [Filter DSL Catalog](/docs?slug=filter-dsl-catalog). |
| `filter-catalog [--json]` | Print the inline Filter DSL indicator/operator catalog and examples for CLI/chat agents. |
| `set-pipeline <strategy-id> --kind single\|sequential\|graph [--edge from:to …]` | Set the strategy pipeline shape; repeat `--edge` for graph edges. |
| `run <id> --fixture <name> [--decisions <n>] [--mock]` | Run a strategy inline against a fixture parquet; `--mock` uses deterministic dispatch with no API calls. |
| `migrate-agents [--dry-run]` | Migrate legacy slot-shaped strategies into agent references; `--dry-run` previews without writing. |
| `diagnostics <id> [--json]` | Capability-completeness readiness report for every agent slot in the strategy: which capabilities are required, which are unmet (and why), and which are optimizable now. Exits `14` (`OptValidation`) when the strategy is **not launchable**, `4` (`NotFound`) for an unknown id. See [Capability diagnostics](#capability-diagnostics). |
| `leaderboard [--sort return\|sharpe] [--top <n>] [--since-days <n>] [--json]` | Rank strategies by their best completed eval run metric. |

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

Note: all strategy configuration now uses `AgentRef` entries. Legacy slot
fields (`intern_slot`, `trader_slot`, etc.) have been migrated; config
uses the `[default_llm]` TOML block instead of `[intern]`.

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
| `sweep --strategy <id> --scenario <id> --from <date> --to <date> [--window 90d] [--step 30d] [--profile smoke\|deep] [--assets <sym,…>] [--json]` | Clone a base scenario across rolling windows, run each clone, and print a metrics table. |
| `attest <run-id> [--json]` | Sign and persist an `EvalAttestation` for a completed run. |
| `review <run-id> --agent <profile> [--force] [--format human\|json] [--output <path>]` | Generate an analytical review of a completed run using the named agent profile. Idempotent: a prior failed review is retried; a completed review is returned as-is unless `--force` is set. |
| `batch run --strategy <id> --scenarios <id,…> [--mode backtest\|paper] [--wait] [--review-with <profile>] [--json]` | Launch one eval run per scenario, block until all reach terminal state (`--wait`), and return a unified `BatchResult`. When `--review-with <profile>` is set (requires `--wait`), a review is generated for each completed run using the named agent profile; failures are captured per-run and do not abort the batch. |
| `batch status <batch-id> [--json]` | Show the persisted batch row and its attached run ids. |

The `batch compare` workflow: run `eval batch run … --json` to get a `batch_id`,
then pass `--batch <batch_id>` to `eval compare` to resolve run ids automatically.

---

### `xvn live …`

Guarded launcher for live runs against a testnet or real-money venue. The verb
builds a `LiveConfig` and submits it through the same engine `eval run` path as
the dashboard, but it is intentionally denied by the remote-CLI allowlist: run it
only from a trusted local shell.

| Flag | Effect |
|---|---|
| `--venue <name>` | Broker credential key / execution venue; defaults to `byreal`. |
| `--network mainnet\|testnet` | Selects `venue_label=Live` for mainnet or `venue_label=Testnet` for testnet. Mainnet is real money. |
| `--i-understand-real-money` | Required for `--network mainnet`; omitted for testnet. |
| `--strategy <id>` | Strategy id to run. |
| `--display-name <name>` | Human-readable run name shown in eval history. |
| `--asset <symbol>` | Venue asset pair, e.g. `BTC/USD`. |
| `--capital <usd>` | Initial capital for the live run. |
| `--bar-limit <n>` / `--decision-limit <n>` / `--time-limit-secs <n>` | Optional stop limits. |
| `--warmup-bars <n>` | Historical warm-up bars before live streaming starts; default `200`. |
| `--xvn-home <path>` | Override `XVN_HOME`. |
| `--json` | Print the launched run object as JSON. |

Examples:

```
xvn live --venue byreal --network testnet --strategy <id> \
  --display-name "Testnet smoke" --asset BTC/USD --capital 1000 --bar-limit 50

xvn live --venue byreal --network mainnet --i-understand-real-money \
  --strategy <id> --display-name "Mainnet perps" --asset BTC/USD \
  --capital 5000 --time-limit-secs 3600
```

---

### `xvn marketplace …`

Testnet marketplace listing, purchase, and attestation commands. The CLI uses
the mock marketplace driver by default for local automation; setting
`MARKETPLACE_DRIVER=onchain` runs against the deployed Mantle Sepolia
contracts (write verbs need `MANTLE_PRIVATE_KEY` plus the `XVN_*` contract
address env vars; see `xvn marketplace --help` for the full env contract).

| Verb | Effect |
|---|---|
| `list` | Default: read `XVN_MARKETPLACE_FIXTURE` or `$XVN_HOME/marketplace/listings.json` and print listing rows as `agent_id`, `version`, `price_usdc`, `seller`, and `status`. With `MARKETPLACE_DRIVER=onchain`: enumerate the deployed ListingRegistry read-only (no signer; needs `XVN_LISTING_REGISTRY` + `XVN_IDENTITY_REGISTRY`) and print `listing_id \| agent_id \| price_usdc \| seller \| revoked`. Prints `(no listings)` when empty. |
| `show-token --token-id <id> [--svg-out <path>]` | Read-only fetch + decode of `IdentityRegistry.tokenURI(id)` (needs `XVN_IDENTITY_REGISTRY`). Prints name, agent_id, the Symmetry/Palette/Density/Layers attributes, and the decoded SVG byte length; `--svg-out` writes the SVG file. |
| `publish --agent-id <id> --price <usdc> --manifest-path <path>` | Validate the manifest JSON, hash its content, and publish a listing. Prints `listing_id=<id> agent_id=<id>`. |
| `buy --listing-id <id> --buyer <0xaddress>` | Buy a listing for the supplied wallet address. Prints `tx_hash=<hash> license_token_id=<id>`. |
| `attest --listing-id <id> --cycles <n> --sharpe <f>` | Hash and post an eval attestation payload for a listing. Prints `tx_hash=<hash>`. |

`--price` must be a non-negative finite USDC value. `--buyer` must parse as an
EVM address. `XVN_MANTLE_RPC_URL` defaults to `https://rpc.sepolia.mantle.xyz`.

---

### `xvn last`

Compact health card for recent eval runs.

| Verb | Effect |
|---|---|
| `last [--n <n>] [--strategy <id>] [--json]` | Show the most recent eval run(s), including status, strategy, scenario, timestamps, metrics, trades, and decisions. |

---

### `xvn model …`

Bounded `(strategy × model)` matrix evals for model-quality bakeoffs. The
orchestrator persists a `eval_bakeoffs` row with run ids per arm; per-arm hard
limits route through `EvalLimits` (PR #428) so a single chatty model can't
blow out the cap. Two materialization modes: `override` (default — per-launch
provider/model swap via `cli-eval-model-override`) and `clone` (deferred —
will materialize one cloned strategy per arm via
`cli-strategy-clone-model-override`).

| Verb | Effect |
|---|---|
| `bakeoff --strategies <ids,…> --scenario <id> --provider <p> --models <ids,…> [--mode override\|clone] [--clone-name-template <tpl>] [--max-runs <n>] [--sequential\|--parallel] [--max-decisions <n>] [--max-input-tokens <n>] [--max-output-tokens <n>] [--max-wall-clock <n>] [--cancel-on-token-limit] [--run-mode paper\|backtest] [--compare [--markdown]] [--name <name>] [--yes] [--json]` | Launch an N×M bakeoff. Without `--yes` the verb prints a dry-run plan (per-arm `(strategy, provider, model)`, caps, expected ceiling) and exits with a `--yes` reminder. With `--yes` the dry-run still prints to stderr; arms launch sequentially by default. With `--compare`, emits a `ComparisonReport` over all arm run-ids once terminal; pair with `--markdown` for the table form. `--use-strategy-models` (mutually exclusive with `--models`) keeps each strategy's natively-bound model. |
| `status <bakeoff-id> [--json]` | Read the bakeoff record + joined run rows. Same shape as `xvn eval batch status`. |

`--strategy` is accepted as an alias for `--strategies` (singular form is what
the remote-CLI allowlist passes). Compare chunks at 10 arms per markdown table.

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
| `update <id> [--conclusion <text>] [--next-recommendation <text>] [--bind-batch <batch-id>] [--json]` | Apply partial edits; at least one flag is required. |
| `run --name <name> --strategy <id> [--scenarios <id,…>] [--assets <a,…> --timeframe <tf> --target-decisions <n>] [--decision-budget <n>] [--wait] [--review-with <profile>] [--compare [--markdown]] [--json]` | Full orchestration in one command: create experiment → run batch → bind → write `result_json`. Scenario selection either via explicit `--scenarios` or via the scenario selector flags. Emits `{experiment_id, strategy_ids, scenario_ids, batch_id, result: {profitable_count, best_scenario, worst_scenario, runs}, compare_markdown?}` when `--json` is set. |

---

### `xvn agent …`

Workspace agent library: inspect, create, list, and lint agent records.

| Verb | Effect |
|---|---|
| `get <agent-id> [--format json\|json-compact]` | Fetch a single agent by id. Returns the full `Agent` JSON shape (same as `agents[]` in `EvalRunExport`). Alias: `show`. |
| `create --name <name> --capability <trader\|filter\|router> --provider <id> --model <id> --system-prompt <text\|@path> [--temperature <f>] [--max-tokens <n>] [--tags <t>…] [--description <s>] [--format json\|json-compact]` | Create a single-slot agent in the workspace library. `--system-prompt @path` reads the prompt from a file. |
| `ls [--format table\|json\|json-compact] [--tag <t>…] [--include-archived]` | List agents. Default output is a table with columns AGENT_ID, NAME, CAPABILITIES, MODELS, ARCHIVED, TAGS. Use `--format json` or `json-compact` for machine-readable output. Alias: `list`. |
| `lint [<agent-id>] [--json]` | Validate one or all agents and report diagnostics. Exits 0 when there are no error-severity findings; exits 2 when any error-severity diagnostic is present (suitable as a CI gate). `--json` emits a JSON array of `{agent_id, diagnostics: [{code, severity, message, field}]}`. |

---

### `xvn agent inspect <agent-id> --diagnostics`

Per-agent capability readiness, independent of any strategy. For each
capability the agent's slots declare, it reports `slot`, `has_prompt`,
`has_model_binding`, the `required_tools`, whether the runtime supports the
capability, and whether it is `optimizable` (has a DSPy signature). Text mode
prints one line per capability; `--json` emits the structured object below.

```
xvn agent inspect <agent-id> --diagnostics
xvn agent inspect <agent-id> --diagnostics --json
```

```json
{
  "agent_id": "01KSEK3NRR4EVVV0J6ZYDKDEFA",
  "name": "complete-trader",
  "archived": false,
  "capabilities": [
    {
      "capability": "trader",
      "slot": "main",
      "has_prompt": true,
      "has_model_binding": true,
      "required_tools": ["ohlcv"],
      "runtime_supported": true,
      "optimizable": true
    }
  ]
}
```

Exit `0` on success, `4` (`NotFound`) for an unknown agent id. Unlike
`xvn strategy diagnostics`, an agent inspect does not fail just because a
capability is incomplete — it reports state; launch-gating is a strategy-level
concern.

---

### `xvn trajectory …`

Trajectory store operations for Cline record/replay runs. These commands inspect
and maintain the migrated trajectory SQLite tables plus the frame blob store.

| Verb | Effect |
|---|---|
| `inspect <recording-id> [--db <path>] [--blob-root <path>]` | Print schema version, status, key fingerprint, cycle/slot metadata, timestamps, and frame counts by `(slot_role, step_index)`. |
| `validate <recording-id> [--db <path>] [--blob-root <path>] [--quiet]` | Validate that a recording is complete and has no missing or out-of-order frames. Exits non-zero for corrupt, incomplete, or gapped recordings. |
| `purge --expired [--db <path>] [--blob-root <path>] [--dry-run]` | Delete recordings whose `expires_at` is in the past, including frames and orphaned blobs. |
| `purge --before <rfc3339> [--db <path>] [--blob-root <path>] [--dry-run]` | Delete recordings expiring before the supplied timestamp. |
| `reindex [--db <path>] [--blob-root <path>]` | Recompute `key_fingerprint` for all recordings after a schema-compatible trajectory-key change. |

---

### `xvn optimize …`

**The Optimizer cycle has ONE CLI home: `xvn optimize run`.** This is the
operator-surface verb for the AutoOptimizer subsystem (developer-surface
codename `autooptimizer`). It runs the optimizer cycle: propose candidate
experiments against a parent strategy, backtest them on the day + untouched
windows, apply the decision gate, and (optionally) compile the DSPy flywheel.

> **Consolidation (2026-06-11).** Two changes landed together:
> 1. The old top-level **`xvn optimizer`** verb **no longer exists** — its
>    surface folded into `xvn optimize`.
> 2. The **standalone DSPy prompt-optimizer CLI verbs were removed**
>    (`run --agent/--slot/--capability/--corpus/--optimizer/--metric`,
>    `inspect --run`, `export-demos`, `import-demos`, `accept-as-child-agent`,
>    `revert-accepted`, `explain-missing-data`, `memory-demos`,
>    `memory-demos-gate`, and the old `distill/gate/activate/retire/
>    promote/demote`). The DSPy flywheel now runs **inside the cycle
>    automatically** and emits `CycleProgressEvent::FlywheelCompiled`. The
>    engine `optimization/` module and the `Optimizer*` types are **unchanged**
>    — only the CLI verbs went away.

**`xvn optimize run` is the one verb for running the cycle.** By default it
runs one cycle and exits. Use `--max-cycles` to change the run count.

| Verb | Effect |
|---|---|
| `run` | Run the optimizer cycle: propose → backtest (day + untouched windows) → gate → optional DSPy flywheel. Reads `$XVN_HOME/autooptimizer.toml` (or `--config <path>`, which **replaces** that file). See [Optimizer config](/docs?slug=autooptimizer-config). |
| `ls [--json]` | List cycle history (most recent first). Shows live runs while they are in progress. |
| `show <cycle_id> [--json]` | Show one cycle's detail: experiments, gate verdicts, scores. |
| `lineage ls [--json]` | List the strategy lineage (parent → child edges, statuses). |
| `lineage show <id> [--json]` | Show one lineage node / subtree. |
| `unlock` | Clear a wedged cycle lock on a **foreign host** (e.g. a run that died on a different machine). Not needed for normal kill→restart on the same host — stale locks are cleared automatically on the next `run`. |

**Flags on `run`:**

| Flag | Effect |
|---|---|
| `--config <path>` | Replace the default config file (`$XVN_HOME/autooptimizer.toml`) with the supplied path. |
| `--max-cycles <N>` | Unset (default): run one cycle and exit. `0`: run continuously until SIGINT/SIGTERM, the `--budget` ceiling, or convergence (fire-and-forget). `N`: run exactly N cycles. |
| `--ipc-socket <PATH>` | Stream live progress to the dashboard /optimizer page via a Unix socket. Auto-connects to `/tmp/xvn-optimizer.sock` when a dashboard is listening there (start the dashboard server with `dashboard serve --autooptimizer-ipc-socket /tmp/xvn-optimizer.sock`). Pass `--ipc-socket ''` to disable. |
| `--experiments-per-cycle <n>` | Number of experiment proposals per cycle. |
| `--objective <sharpe\|total_return\|max_drawdown\|win_rate>` | Optimization objective. |
| `--min-improvement <f>` | Minimum improvement threshold for the gate. |
| `--day-start` / `--day-end` | Day-window boundaries for the primary backtest. |
| `--baseline-start` / `--baseline-end` | Untouched-window boundaries for the gate holdout. |
| `--budget <n>` | Maximum number of eval decisions across the run (ceiling for continuous mode). |

See [Optimizer config](/docs?slug=autooptimizer-config) for which knobs are
config fields vs CLI-only flags. A bad config surfaces a **field-level** parse
error naming the offending TOML key.

`xvn optimize run` writes live session state on start, so `xvn optimize ls`
shows the run while it is in progress, and the dashboard /optimizer page
reflects it (and, via the IPC socket, streams live progress events).

**Auto-unlock:** if a previous run was killed (SIGKILL, OOM, container restart),
the stale lock is cleared automatically on the next `xvn optimize run` via
heartbeat detection. Manual `xvn optimize unlock` is only needed when the
lock was left by a run on a **different host**.

```
# run one cycle (default) with the default config
xvn optimize run

# run one cycle with an alternate config file (REPLACES the default)
xvn optimize run --config ./my-autooptimizer.toml

# fire-and-forget: run continuously until SIGINT/SIGTERM, budget, or convergence
xvn optimize run --strategy <id> --provider ollama --model qwen3:4b --objective sharpe --max-cycles 0

# run exactly 5 cycles, streaming live progress to the dashboard
xvn optimize run --max-cycles 5 --ipc-socket /tmp/xvn-optimizer.sock

# cycle history + detail
xvn optimize ls --json
xvn optimize show <cycle_id> --json

# lineage
xvn optimize lineage ls
xvn optimize lineage show <id>

# clear a stranded lock from a crashed run on a foreign host
xvn optimize unlock
```

`xvn optimize` returns a distinct exit code per failure class so an agent can
branch on the exact reason without parsing text — see [Exit codes](#exit-codes).

---

### `xvn memory …`

Operator surface for V2D memory items. Reads default to Patterns because those
are the operator-managed kind; Observations can be listed for audit.

| Verb | Effect |
|---|---|
| `ls [--kind pattern\|observation] [--namespace <ns>] [--agent <id>] [--scenario <id>] [--run <id>] [--limit <n>] [--offset <n>] [--json]` | List memory items. `--agent <id>` is shorthand for `namespace=agent:<id>`. |
| `show <id> [--json]` | Print one item with kind, namespace, provenance, training window, and text. |
| `add-pattern "<text>" --namespace <ns> [--training-end <date>] [--force] [--json]` | Seed an operator Pattern. `--agent <id>` may be used instead of `--namespace`. Without an embedder configured, exits non-zero unless `--force` is set. |
| `rm <id> [--json]` | Delete one memory item by id. |
| `forget --namespace <ns> [--json]` | Bulk-delete every item in a namespace. |
| `forget --agent <id> [--json]` | Bulk-delete one agent's private namespace (`agent:<id>`). |

`--training-end YYYY-MM-DD` normalizes to end-of-day UTC so the Pattern is
excluded from scenarios that overlap that date and recalled only by scenarios
starting afterward. Leaving it blank makes the Pattern operator-attested wisdom
that can be recalled in every scenario.

---

> **`xvn optimizer …` was removed (2026-06-11).** The top-level `xvn optimizer`
> verb no longer exists. The Optimizer cycle is now driven entirely by
> [`xvn optimize`](#xvn-optimize-) (default action runs the cycle). The old
> Observation-cohort distillation that lived here now runs **inside the cycle's
> DSPy flywheel** automatically; there is no separate operator verb for it.

### `xvn flywheel …`

Read-only operator telemetry for the memory + Optimizer + DSPy-optimizer loop.

| Verb | Effect |
|---|---|
| `status [--namespace <ns>] [--agent <id>] [--json]` | Summarize Observation count, active Pattern count, staged Pattern count, retired Pattern count, optimizer run count, and optimization count. |
| `velocity [--namespace <ns>] [--agent <id>] [--days <n>] [--json]` | Return daily flywheel counters over a rolling window: Observations, activations, retirements, optimizer runs, and optimizations. |
| `lineage --agent <id> [--limit <n>] [--json]` | Show memory-demo optimization lineage for an agent namespace, including demo/prior Pattern ids, split hashes, and gate verdicts. |

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

### `xvn tool-policy …`

Inspect and override the chat-rail tool policies — which tools the model may
invoke, and whether a Write tool runs without an approval round-trip. Business
logic lives in `xvision_engine::api::tool_policy`; this verb is a thin CLI shim.
Each policy has an `enabled` flag (the tool is visible to the model and may run)
and an `auto_approve` flag (a Write-class tool in Act mode runs without an
approval round-trip). Scope is `global` (workspace-wide, the default) or a user
id for per-user overrides.

| Verb | Effect |
|---|---|
| `list [--scope <global\|user-id>]` | List effective policies for all known tools (overrides + class defaults). |
| `show <tool-name> [--scope <global\|user-id>]` | Show the effective policy for one tool (e.g. `create_strategy`, `run_eval`). |
| `set <tool-name> --enabled <bool> --auto-approve <bool> [--scope <global\|user-id>]` | Upsert an override for one tool. |
| `reset <tool-name> [--scope <global\|user-id>]` | Remove an override, reverting the tool to its class default. |

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
| `xvn spot --side buy\|sell --symbol <ticker> --amount <usd> [--slippage <bps>] [--i-understand-real-money]` | Gated one-shot **Solana spot** swap via `byreal-cli` (curated SPL + xStocks; symbol→mint from `$XVN_HOME/config/byreal_spot_assets.toml`). **Defaults to a no-funds `--dry-run` preview**; `--i-understand-real-money` executes a real swap, with the global safety kill-switch checked first. Long/Flat only; custody is the byreal-cli keystore. |

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

`xvn optimize` and `xvn strategy diagnostics` add a distinct band (10–15) so an
agent can branch on the exact optimization/launch-gate failure without parsing
text:

| Code | Name | Meaning |
|---|---|---|
| 10 | OptMissingData | The corpus / training query resolved to no usable training data. |
| 11 | OptMissingCapability | The requested capability has no optimizer signature (typed `missing_capability_optimizer`). |
| 12 | OptProvider | The model provider could not be reached / is not configured. |
| 13 | OptMetric | The objective metric failed to evaluate (e.g. unknown metric name). |
| 14 | OptValidation | Input/signature validation failed — bad capability/optimizer enum, missing corpus path, signature parse/validate error. Also the **not-launchable** code for `xvn strategy diagnostics`. |
| 15 | OptPersistence | The store write failed (migration not applied, DB error). |

`xvn strategy validate` exits non-zero (code 2) when `eval_ready` is false,
making it safe to use as a gate in a shell pipeline. `xvn strategy diagnostics`
exits `14` when a strategy is not launchable, so it can gate a launch the same
way.

---

## Capability diagnostics

`xvn strategy diagnostics <id>` and `xvn agent inspect <id> --diagnostics`
answer the launch-readiness question: does every required capability in the
strategy have a slot with a prompt, a model binding, its required tools, and a
runtime that supports it?

```
xvn strategy diagnostics <strategy-id>          # text
xvn strategy diagnostics <strategy-id> --json
```

Text mode for a launchable strategy:

```
strategy: 01HZCOMPLETE000000000000AA
launchable: yes
required capabilities: trader
optimizable now (dspy signature): trader

• role 'trader' → agent 01KSEK3NRR4EVVV0J6ZYDKDEFA (complete-trader)
    [required] trader   optimizable tools=ohlcv
```

For an incomplete strategy it lists each unmet capability with a typed reason
and exits `14`:

```
strategy: 01HZINCOMPLETE0000000000BB
launchable: NO
required capabilities: trader

• role 'trader' → agent 01KSEK3NRR4EVVV0J6ZYDKDEFA (complete-trader)
    [required] trader   MISSING_TOOL(ohlcv) tools=ohlcv

UNMET REQUIRED CAPABILITIES:
  - role 'trader' capability 'trader': MISSING_TOOL(ohlcv)
strategy '01HZINCOMPLETE0000000000BB' is not launchable: 1 unmet required capability (trader:trader=missing_tool)
```

The `--json` object carries `per_agent[]` (with each capability's typed
`status`: `optimizable` / `missing_tool` / `missing_prompt` /
`missing_model_binding` / `unsupported`), `required_capabilities[]`,
`required_unmet[]`, `optimizable[]`, and the top-level `launchable: bool`. The
unmet-status `kind` values map onto the same vocabulary the unified event
stream uses (`error_missing_capability`, `error_missing_tool`, …). Only the
`trader` and `filter` capabilities are optimizable today; `critic` and `router`
are recognized but `unsupported` and block launch when required.

---

## Where things live

`XVN_HOME` defaults to `~/.xvn` and is honored by every subcommand. Set it
explicitly with the `XVN_HOME` env var or the per-command `--xvn-home` flag.

| Path | Contents |
|---|---|
| `$XVN_HOME/strategies/<id>.json` | Serialised `Strategy` objects. |
| `$XVN_HOME/xvn.db` | SQLite flight recorder: runs, decisions, batches, experiments, reviews, equity, agents. |
| `$XVN_HOME/bars/<asset>/<granularity>.json` | Bars cache written by `xvn bars fetch`. |
| `$XVN_HOME/config/default.toml` | Runtime config: providers, intern/trader defaults, backtest params. Override path: `XVN_CONFIG_PATH`. |
| `$XVN_HOME/secrets/providers.toml` | Provider API keys (separate from config; checked by `xvn doctor`). |
| `$XVN_HOME/secrets/brokers.toml` | Broker credentials (checked by `xvn doctor`). |
| `$XVN_HOME/identity/signing.key` | Signing key used by `xvn eval attest`. Auto-generated on first use. |
