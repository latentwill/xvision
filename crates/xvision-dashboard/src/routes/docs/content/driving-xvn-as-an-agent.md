# Driving xvn as an agent

`xvn` is designed for an autonomous agent to drive end-to-end without bolting
Python around it. Every verb that produces state has a `--json` shape; every
blocking operation has a `--wait` posture; every failure is typed. The bar for
"fluent" is: an agent can run a full strategy-iteration loop without scraping
stdout or guessing state.

This page is the operator contract for that loop. It distills verbatim feedback
from an agent that drove xvn end-to-end and had to glue the workflow together
with Python: POST strategy / agent / eval requests, poll until runs finish,
normalize metrics, count decisions and action types, compare scenarios, preserve
IDs. None of that should be necessary.

## The loop

A canonical strategy-iteration loop looks like this:

**1. Author hypothesis**

```
xvn strategy create \
  --prompt prompt.md \
  --name "ETH 4h compression sniper v3" \
  --provider openrouter \
  --model kimi-k2 \
  --role trader \
  --asset ETH/USD \
  --timeframe 4h \
  --json
```

Returns `{"strategy_id": "...", "agent_id": "...", "eval_ready": true, "provider": "openrouter", "model": "kimi-k2", "warnings": []}`.
`eval_ready: false` means warnings are non-empty; inspect and fix before
proceeding. The `--prompt` flag activates atomic mode: strategy + agent +
slot binding happen in one transaction.

**2. Validate**

```
xvn strategy validate <strategy_id> --scenario <scenario_id> --json
```

Returns `{"strategy_id": "...", "eval_ready": true, "expected_decisions": 49, "asset": "ETH/USD", "timeframe": "4h", "warmup_bars": 200, "warnings": [], "errors": []}`.
Exit code is non-zero when `eval_ready` is false. Run this before spending eval
tokens; it checks agents, provider/model config, scenario asset/timeframe
alignment, and warmup adequacy.

**3. Pick comparator set**

```
xvn scenario select \
  --assets ETH/USD,BTC/USD,SOL/USD \
  --timeframe 4h \
  --target-decisions 49 \
  --count 4 \
  --regimes bull,bear,range,crash \
  --json
```

Or, to force a common decision count across scenarios:

```
xvn scenario select \
  --same-decisions \
  --max-decisions 105 \
  --count 4 \
  --json
```

Returns a JSON array of scenario rows with `id`, `name`, `asset`, `timeframe`,
`decision_count`, and matched regime tags. Feed the `id` fields into the next
step.

**4. Run + persist**

```
xvn experiment run \
  --name compression-sniper-v3-cross-regime \
  --question "Does tighter compression logic generalize across regimes?" \
  --strategy <strategy_id> \
  --scenarios <id1>,<id2>,<id3>,<id4> \
  --decision-budget 49 \
  --wait \
  --compare \
  --markdown \
  --json
```

Blocks until all runs reach a terminal state. Emits a single
`ExperimentRunOutput` JSON object. To run the batch without the experiment
ledger wrapper:

```
xvn eval batch run \
  --strategy <strategy_id> \
  --scenarios <id1>,<id2>,<id3>,<id4> \
  --mode backtest \
  --wait \
  --json
```

**5. Read compare + review**

`ExperimentRunOutput` fields an agent needs:

- `experiment_id` — stable ledger id for later updates
- `batch_id` — links to per-run status
- `result.profitable_count`, `result.best_scenario`, `result.worst_scenario`
- `result.runs[*]` — per-scenario `return_pct`, `sharpe`, `drawdown_pct`,
  `decisions`, `status`
- `compare_markdown` — GFM table, present when `--compare --markdown` is set

For richer per-run behavior (action distribution, avg bars held, failure mode),
fetch each run with:

```
xvn eval show <run_id> --behavior --json
```

**6. Update ledger**

```
xvn experiment update <experiment_id> \
  --conclusion "Selectivity improved; no positive edge. Needs regime gate." \
  --next-recommendation "Tighten SOL variant; add invalidation exit." \
  --json
```

**7. Repeat with next hypothesis**

Revise `prompt.md`, increment strategy version, re-run from step 1. The
experiment ledger accumulates the history; `xvn experiment ls --json` returns
all rows.

## `--json` everywhere

Every list / get / create / run / validate / compare verb supports `--json`
(or `--format json` / `--format json-compact` on object-get commands). The JSON
shapes are stable and safe for chaining via `jq`. Do NOT regex stdout — use
the exit code and the JSON object.

Examples:

```
xvn strategy ls --json
xvn scenario ls --json
xvn eval list --strategy <id> --json
xvn eval show <run_id> --json
xvn eval compare --batch <batch_id> --json
xvn experiment ls --json
xvn experiment show <exp_id> --json
```

## Exit codes

Non-zero on validation drift, failed runs, missing resources, or provider
errors. Do not infer success from string matching — read the exit code.

| Code | Enum | Meaning |
|------|------|---------|
| 0 | `Success` | Command completed |
| 2 | `Usage` | Caller-fixable: bad flag, malformed input, unknown enum variant, eval-not-ready |
| 3 | `Auth` | Missing or invalid credential (e.g. `ANTHROPIC_API_KEY`) |
| 4 | `NotFound` | Referenced resource does not exist (strategy id, run id, scenario id) |
| 5 | `Upstream` | LLM API / broker / network / filesystem / database error |
| 7 | `Conflict` | State collision (e.g. duplicate name on rename) |

`Upstream` (5) signals "external system failure; retry may help."
`NotFound` (4) signals "the id is wrong; don't retry unchanged."
`Usage` (2) signals "fix the arguments."

## `--wait` vs polling

Long-running verbs (`xvn eval run`, `xvn eval batch run`, `xvn experiment run`)
have two postures:

**`--wait` (recommended for agents):** blocks until the run or batch reaches a
terminal state, then emits a single JSON object on exit. No polling code
required. All three blocking verbs emit a clean object on `--json`:

```
xvn eval run --strategy <id> --scenario <id> --wait --json
xvn eval batch run --strategy <id> --scenarios <ids> --wait --json
xvn experiment run --name <slug> --strategy <id> --scenarios <ids> --wait --json
```

**Without `--wait`:** the verb returns immediately with a run id. Poll state
with:

```
xvn eval show <run_id> --json           # single run
xvn eval batch status <batch_id> --json # batch
```

`xvn eval watch <run_id>` prints a status line on each poll interval. It is
designed for human monitoring in a terminal, not for agent consumption; use
`xvn eval show` with a polling loop instead. SSE streaming is not part of the
CLI surface.

## Idempotency

Batch runs are coalesced by fingerprint (`agent_id`, `scenario_id`, `mode`,
`params_override`). If the same batch is submitted twice — for example, after a
transient `Upstream` failure — you will get the in-flight or completed batch id
back rather than a duplicate run. Do not build dedup logic on top of the CLI.

## Error classes

Failed runs carry a typed behavior summary on `BehaviorSummary.primary_failure_mode`
and per-run `status` fields. The canonical status values are:

- `completed` — run finished; metrics are present
- `failed` — terminal failure; check `error` field for detail
- `cancelled` — operator-cancelled
- `queued` / `running` — non-terminal; should not appear after `--wait`

Within a completed run, `behavior_summary.primary_failure_mode` classifies the
agent's decision pattern (e.g. `late_entries`, `held_through_invalidation`,
`direct_flips`). Branch on this field rather than the raw metric numbers when
diagnosing why a run underperformed.

When a batch has mixed outcomes — some `completed`, some `failed` — the batch
itself does not return a non-zero exit code; inspect `result.runs[*].status`
per scenario.

## Don'ts

**Do not shell around the CLI to glue state.** The intake feedback was exactly
"I had to bolt Python around the CLI" — that is the failure mode this surface
exists to prevent. If a workflow step requires you to write a Python loop
around `xvn`, it means the step is missing from the CLI surface; file it
as a track.

**Do not bypass `xvn` and write to `$XVN_HOME` directly.** Migrations and
validators run through the CLI seam. Direct writes to the SQLite flight-recorder
(`insert_decision`, `insert_briefing`, `insert_trace`) break replay and
metrics reproducibility and are intentionally not surfaced as CLI primitives.

**Do not use the dashboard HTTP API as an agent surface.** The CLI wraps it and
is the agreed contract. Dashboard API shapes can change without the same
stability promise that CLI `--json` shapes carry.

**Do not use the MCP surface without explicit operator authorization.** The MCP
indicator server (`xvn-mcp`) is no longer part of the recommended agent surface
as of 2026-05-10. The MCP crate remains in the workspace for external MCP
clients but is not on the hot path. See
[CLI non-surfaced](docs/cli-non-surfaced.md) for the full footgun inventory.

**Do not mint on-chain or place real orders without an explicit op-mode flag.**
`xvision-identity` (register, post_reputation) and live order submission via
`xvn fire-trade --venue orderly` against mainnet are deliberately out of the
default `xvn` binary. See `docs/cli-non-surfaced.md`.

## Cross-references

- [Strategies](/docs?slug=strategies) — strategy anatomy, atomic create, and
  hypothesis manifest fields
- [Scenarios](/docs?slug=scenarios) — scenario library, select, and regime tags
- [Eval Runs](/docs?slug=eval-runs) — run lifecycle, metrics, behavior summary,
  and review pipeline
- [Experiments](/docs?slug=experiments) — experiment ledger, result_json
  structure, and update contract
- [CLI Reference](/docs?slug=cli-reference) — complete flag inventory for every
  `xvn` verb
