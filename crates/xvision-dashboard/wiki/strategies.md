# Strategies

A strategy is a saved configuration that says: this hypothesis, these agents,
this risk gate, this asset, this timeframe. It composes one or more agents
that play named roles — the default agent, a trader, and optionally a risk
gatekeeper — and wires them together into a pipeline. Strategies are the
thing you iterate on; scenarios are the market window you run them against.

See [Agents](/docs?slug=agents) for how to manage the reusable agent library
that strategies draw from.

---

## Author a strategy from the dashboard

1. Open `/strategies/new`.
2. Pick a template from the template picker (optional). Templates autofill
   the strategy name, asset, cadence, and default agent roles.
3. Fill in the form sections:
   - **Name** — a display name for the strategy.
   - **Template** — optional starting scaffold (`trend-follower`,
     `mean-reversion`, `custom`, etc.).
   - **Asset** — primary asset this strategy trades (e.g. `ETH/USD`).
   - **Timeframe** — bar granularity: `1m`, `5m`, `15m`, `30m`, `1h`, `2h`,
     `4h`, or `1d`.
   - **Agents** — attach agents from the workspace agent library and assign
     each a role (`trader`, `risk`, etc.).
   - **Pipeline** — single agent, sequential chain, or graph wiring.
   - **Hypothesis** (optional) — family label, hypothesis statement, target
     regimes, regimes to avoid.
4. Click **Save**. The dashboard validates the strategy and surfaces any
   missing-agent or missing-provider issues inline.


## Multi-timeframe

Strategies can declare multiple timeframes in their manifest under
`timeframe_requirements.auxiliary`. Each entry is a timeframe string
(`"15m"`, `"1h"`, `"4h"`, etc.) specifying an additional bar series the
agent receives alongside the strategy's native decision cadence. This
enables multi-timeframe analysis in a single dispatch — the agent gets
bar history for each declared timeframe and can reason across them.

Set auxiliary timeframes via the dashboard under the Timeframe section
of the strategy editor (click **+ Add timeframe**), or directly in the
strategy JSON:

```json
{
  "timeframe_requirements": {
    "auxiliary": ["15m", "1h"]
  }
}
```

When `auxiliary` is empty or omitted, the strategy uses its native
timeframe only — the legacy single-timeframe behavior is preserved.
---

## Author a strategy from the CLI

### File mode

Create editable starter manifests, then import one as a strategy:

```sh
xvn strategies init
xvn strategy create --from-file ~/.xvn/strategies/<strategy-id>.json --json
```

`xvn strategies init` writes starter strategy JSON files under the xvision home
directory. Edit the manifest first, then import it with `--from-file`.

### Atomic mode

`--prompt` creates a strategy and its agent in one command. Required in this
mode: `--name`, `--provider`, `--model`, `--asset`, `--timeframe`.

```sh
xvn strategy create \
  --prompt ./prompts/eth-breakout.txt \
  --name "ETH Breakout 4h" \
  --provider openrouter \
  --model deepseek/deepseek-chat \
  --asset ETH/USD \
  --timeframe 4h \
  --role trader \
  --family compression-breakout \
  --hypothesis "ETH breaks out of compression ranges on 4h bars" \
  --target-regime "post-compression trend" \
  --avoid-regime chop \
  --json
```

`--role` defaults to `trader`. `--template` and `--prompt` are mutually
exclusive.

**Hypothesis flags** (all optional; usable in template mode too):

| Flag | What you type |
|---|---|
| `--family <label>` | Hypothesis family label, e.g. `compression-breakout` |
| `--hypothesis <text>` | One-to-two sentence hypothesis statement |
| `--target-regime <val>` | Regime the strategy targets (repeatable) |
| `--avoid-regime <val>` | Regime the strategy should avoid (repeatable) |
| `--hypothesis-file <path>` | Path to a JSON file with a complete hypothesis object; overrides individual flags |

Use `--hypothesis-file` when you need to supply the full long-form hypothesis
object. The file must be valid JSON.

**`--json` output (atomic mode):**

```json
{
  "strategy_id": "01HZSTRATEGY...",
  "agent_id":    "01HZAGENT...",
  "eval_ready":  true,
  "provider":    "openrouter",
  "model":       "deepseek/deepseek-chat",
  "warnings":    []
}
```

`eval_ready` is `false` when `warnings` is non-empty (for example, if the
prompt text mentions a different asset than `--asset`).

---

## Filters

Strategies can gate agent dispatch behind firing conditions — typed
predicates that decide, per bar, whether an agent should fire or skip.
See [Firing Conditions](/docs?slug=firing-conditions) for the full
authoring workflow, filter timeline states, and cost framing.

xvision supports two filter paths:

1. **Filter-capable LLM agent** — a dedicated agent that emits a
   `FilterSignal` wired to a Trader via a `PipelineEdge` with an
   `EdgePredicate`. Author from the SPA strategy editor or via
   `xvn strategy add-filter`. This is the primary path for
   regime-conditional dispatch gates.
2. **Inline deterministic Filter DSL** — a pure indicator-condition
   tree under `strategy.filter`, installed via `xvn strategy
   set-filter --from-json`. No LLM call; the engine evaluates
   indicator conditions directly. See [Filter DSL
   Catalog](/docs?slug=filter-dsl-catalog) for the indicator/operator
   reference and copyable JSON examples.

A strategy with no filter fires its Trader on every bar. The validator
flags this with an advisory warning (see [Validate](#validate)).

## Validate

Shape-only check (no scenario required):

```sh
xvn strategy validate <id>
```

Full preflight with a scenario:

```sh
xvn strategy validate <id> --scenario <scenario-id> --json
```

With `--scenario`, the preflight checks:

- All attached agents exist in the workspace library with provider and model
  set.
- Provider names resolve against the active config.
- The scenario asset is in the strategy's asset universe.
- The scenario granularity matches the strategy's timeframe.
- Warmup bar adequacy.
- Filter indicators have adequate warmup bars for the scenario duration
  (e.g. `rvol_tod_20` needs 20 same-slot sessions × bars-per-day;
  under-provisioned scenarios emit a warning).

`--json` output:

```json
{
  "strategy_id":        "...",
  "eval_ready":         true,
  "expected_decisions": 42,
  "asset":              "ETH/USD",
  "timeframe":          "4h",
  "warmup_bars":        200,
  "warnings":           [],
  "errors":             []
}
```

`eval_ready` is `true` only when both `errors` and `warnings` are empty. The
command exits non-zero when the strategy is not eval-ready.

---

## CLI verbs at a glance

See [CLI Reference](/docs?slug=cli-reference) for full flag documentation.

| Verb | Effect |
|---|---|
| `xvn strategy create --template <name> --name <name>` | Create a strategy draft from a template. Alias: `new`. |
| `xvn strategy create --prompt <file> --name <n> --provider <p> --model <m> --asset <sym> --timeframe <tf>` | Atomic mode: create strategy + agent in one command. |
| `xvn strategy create --from-file <path>` | Load and persist a strategy from a JSON or TOML file. |
| `xvn strategy ls [--json]` | List saved strategy ids. |
| `xvn strategy show <id> [--format json\|json-compact]` | Print a strategy as JSON. Alias: `get`. |
| `xvn strategy validate <id> [--scenario <id>] [--json]` | Shape-only or full preflight check. |
| `xvn strategy templates [--json]` | List available strategy templates. |
| `xvn strategy add-agent <strategy-id> <agent-id> --role <role>` | Attach a library agent to a strategy. |
| `xvn strategy remove-agent <strategy-id> --role <role>` | Detach an agent reference by role. |
| `xvn strategy set-pipeline <strategy-id> --kind single\|sequential\|graph [--edge from:to …]` | Set the pipeline wiring. |
| `xvn strategy migrate-agents [--dry-run]` | Migrate legacy slot-shaped strategies into agent references. |
| `xvn strategy run <id> --fixture <name> [--decisions <n>] [--mock]` | Run a strategy inline against a fixture parquet. |
