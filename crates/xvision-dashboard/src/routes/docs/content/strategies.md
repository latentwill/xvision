# Strategies

A `Strategy` is the immutable pipeline configuration that drives an eval.
It composes one or more `Agent`s and binds them to roles
(intern → trader → risk → executor by convention). The `intern` role is
being renamed to "default agent" in the UI; both terms remain valid during
the crossover period.

## Anatomy

- **Manifest** — display name, creator, template label, `asset_universe`,
  `decision_cadence_minutes`, `regime_fit`, `risk_preset_or_config`,
  `required_models`, `required_tools`, and optional `min_warmup_bars`.
- **AgentRefs** — `{ agent_id, role }` pointers into the workspace agent
  library. Agents are reusable across strategies; a strategy references them
  by ULID.
- **Pipeline** — wiring spec for the agent refs. `{ kind: "single" }` for
  one-agent strategies; `{ kind: "sequential" }` for ordered multi-stage
  pipelines; `{ kind: "graph", edges: [...] }` for arbitrary DAG topologies.
  Edges are `from_role:to_role` pairs.
- **Risk** — gate parameters that the executor reads: `risk_pct_per_trade`,
  `max_leverage`, stop-loss, and related fields stored in `RiskConfig`.
- **Mechanical params** — template-specific numeric parameters (EMA periods,
  RSI thresholds, Donchian length, etc.) stored as a typed JSON blob keyed
  on `manifest.template`.

## Hypothesis manifest

Added in wave-C, the `Hypothesis` struct attaches structured intent to a
strategy. It is additive — it does not replace the agent's freeform prompt.
Its purpose is grouping and comparison: experiments that share a hypothesis
family can be filtered and ranked together.

All fields are optional. A strategy without any hypothesis annotation
round-trips cleanly.

| Field | Type | Purpose |
|---|---|---|
| `family` | `string` | Template or grouping label, e.g. `compression-breakout` |
| `statement` | `string` | One-to-two sentence hypothesis statement |
| `target_regime` | `string[]` | Regimes the strategy is expected to perform well in |
| `avoid_regime` | `string[]` | Regimes the strategy should avoid |
| `asset_assumptions` | `string[]` | Asset characteristics assumed (e.g. `high liquidity`) |
| `timeframe_preferred` | `string` | Preferred bar granularity, e.g. `4h` |
| `entry_logic` | `string[]` | Conditions under which the strategy enters a position |
| `exit_logic` | `string[]` | Conditions under which the strategy exits a position |
| `risk_logic` | `object` | Optional hints: `max_trade_frequency` (`low`/`medium`/`high`) and `no_direct_flips` (bool) |

For experiment grouping and cross-strategy comparison, see
[Experiments](/docs?slug=experiments).

## Author flow

### Dashboard

1. Open `/strategies/new` — choose an optional template from the picker.
   Picking a template autofills the name, asset, cadence, and agent slots.
2. **Save** creates a draft in `$XVN_HOME/strategies/<id>.json`.
3. **Attach agents** through the Inspector (Strategy detail page). Use the
   chat rail to compose a new agent or attach an existing one from the agent
   library.
4. **Validate** — the dashboard surfaces missing-agent, missing-provider, and
   model-resolution drift inline using the same checks the CLI runs.

### CLI atomic create

`xvn strategy create --prompt <file>` bundles strategy + agent +
provider/model binding + role assignment in a single command. Required flags:
`--name`, `--provider`, `--model`, `--asset`, `--timeframe`. `--role`
defaults to `trader`.

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

`--json` output shape:

```json
{
  "strategy_id": "01HZSTRATEGY...",
  "agent_id": "01HZAGENT...",
  "eval_ready": true,
  "provider": "openrouter",
  "model": "deepseek/deepseek-chat",
  "warnings": []
}
```

`eval_ready` is `false` when `warnings` is non-empty (e.g. prompt mentions
a different asset than `--asset`). `--template` and `--prompt` are mutually
exclusive.

To supply the full hypothesis object including `asset_assumptions`,
`timeframe_preferred`, `entry_logic`, `exit_logic`, and `risk_logic`, pass
`--hypothesis-file <path>` with a JSON file instead of individual flags.

## Validate

`xvn strategy validate <id>` is shape-only — checks agent composition,
pipeline wiring, asset universe, and risk config.

`xvn strategy validate <id> --scenario <scenario-id> --json` runs full
preflight, additionally checking:

- Agents exist in the workspace library and have provider + model set.
- Provider names resolve against the active config.
- Scenario asset is in `manifest.asset_universe`.
- Scenario granularity matches `manifest.decision_cadence_minutes`.
- Warmup bars adequacy.

JSON output:

```json
{
  "strategy_id": "...",
  "eval_ready": true,
  "expected_decisions": 42,
  "asset": "ETH/USD",
  "timeframe": "4h",
  "warmup_bars": 200,
  "warnings": [],
  "errors": []
}
```

`eval_ready` is `true` only when both `errors` and `warnings` are empty.

## Templates

Templates are reference scaffolds, not enforcement. They live in
`crates/xvision-engine/src/agents/templates.rs` (three shapes: `single-trader`,
`analyst-executor`, `risk-checked-trader`) and in the strategy template
registry at `crates/xvision-engine/src/templates/` (role labels include
`intern`/default agent, `trader`, `risk`). Strategies may rename or invent
roles freely.

List available strategy templates: `xvn strategy templates [--json]`.

## CLI parity

See [CLI Reference](/docs?slug=cli-reference) for full flag documentation.

- `xvn strategy create [--template <name>] [--prompt <file>]` — create a
  strategy draft from a template, or atomically from a prompt file.
- `xvn strategy ls [--json]` — list saved strategy ids.
- `xvn strategy show <id> [--format json|json-compact]` — print a single
  strategy (`get` is a visible alias).
- `xvn strategy validate <id> [--scenario <id>] [--json]` — shape-only or
  full preflight check.
- `xvn strategy templates [--json]` — list available strategy templates.
- `xvn strategy add-agent <strategy-id> --role <role> <agent-id>` — attach
  a library agent to a strategy.
- `xvn strategy remove-agent <strategy-id> --role <role>` — detach an agent
  by role.
- `xvn strategy set-pipeline <strategy-id> --kind single|sequential|graph
  [--edge from:to ...]` — set the pipeline wiring.
- `xvn strategy migrate-agents [--dry-run]` — lift legacy slot-shaped
  strategies into agent references.
- `xvn strategy run <id> --fixture <name> [--decisions N] [--mock]` — run
  inline against a fixture parquet without the eval engine.
