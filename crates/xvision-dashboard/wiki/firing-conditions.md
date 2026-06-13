# Firing conditions

When an agent in a strategy "fires," it means the engine assembles a
briefing, dispatches it to the agent's LLM, parses the response, and
records the outcome. Every dispatch costs tokens. On the production
default, every Trader agent in every strategy fires on **every bar**.

A **firing condition** is a gate that decides, per bar, whether the
agent should actually fire or skip. Firing conditions reduce LLM cost,
focus the agent on the market regimes where it's expected to perform,
and let one strategy host multiple agents that activate under different
conditions.

This page covers what a firing condition is, how to add one, and what
happens by default.

## The default - every bar

A strategy with one Trader and no firing condition dispatches the
Trader on every bar. This is correct behavior for some strategies (a
simple market-making model that wants continuous quotes, for example)
and wasteful for most (a regime-conditional momentum strategy doesn't
need to think during a sideways drift).

`xvn strategy validate` emits a warning when it finds a Trader agent
with no upstream Filter:

```
warning: strategy 'btc-mean-rev-v1' has a Trader agent with no upstream
Filter - it will dispatch on every bar. Consider adding a Filter to
reduce LLM cost.
```

The warning is advisory. The strategy still validates, still saves,
still runs. To suppress the warning on a strategy where every-bar
dispatch is intentional, save the strategy with the
`acknowledge_no_filter: true` flag (in the SPA: click the "Every bar is
intentional" checkbox next to the warning; in the CLI: pass
`--no-filter-warning` to `xvn strategy create` or `xvn strategy edit`).

## How firing conditions work - the Filter agent

In xvision, firing conditions are themselves agents. A **Filter-capable
agent** is an agent whose job is to emit a `FilterSignal` - a small
typed JSON object like `{ name: "regime_filter", payload: { regime:
"high_vol" } }` - instead of a trading decision. A Filter agent runs on
the same bar cadence as a Trader, but its output is a signal, not an
action.

A strategy gates a Trader on a Filter signal by:

1. Including a Filter-capable agent in the strategy's `agents` list.
2. Adding a `PipelineEdge` from the Filter agent to the Trader.
3. Attaching an `EdgePredicate` to that edge: a typed comparison like
   *"signal `regime_filter`, field `regime`, equals `high_vol`"*. When
   the predicate evaluates true, the Trader fires. When false, it
   skips.

You can wire multiple Filter agents and multiple predicates. A Trader
with two incoming Filter edges only fires when both predicates pass.

## Why firing conditions are a strategy-level concern, not an agent-level one

The agent editor (`/agents/new` and `/agents/:id`) does not author
firing conditions. An agent is a reusable template - the same Trader
might appear in three strategies, each with a different firing
condition. Asking the agent template to carry one firing condition
would either fight the reusability (every strategy gets the same gate)
or require an inheritance/override system (more complexity than the
problem deserves at v1).

So: the agent editor explains firing conditions; the strategy editor
authors them.

A future revision may add a *default* firing condition at the template
level that strategies inherit-and-override. That's a follow-up,
tracked in the spec at
`docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md`.

## Authoring a firing condition from the SPA

1. Open the strategy in `/strategies/:id/edit`.
2. Find the Trader (or Router) agent card you want to
   gate. Each non-Filter agent card has a "When does this fire?"
   sub-section.
3. Click **Add filter**. The inline composer opens.
4. Either pick an existing Filter agent from the workspace, or author a
   new one inline (provider, model, system prompt, skills).
5. Toggle **Save as reusable agent** if you want the new Filter agent
   to appear in your workspace agent list. Default: on. Toggle off to
   keep the Filter agent scoped to this one strategy.
6. Compose the predicate: pick the signal name, the field, the
   operator (`eq`, `ne`, `gt`, `lt`, `in`), and the value.
7. Save the strategy. The "When does this fire?" section now shows the
   active condition: *"Fires when `regime_filter.regime == 'high_vol'`"*.

## Authoring a firing condition from the CLI

The CLI exposes three verbs:

- **Create the Filter agent first:**

  ```
  xvn agent create \
    --name regime-filter-v1 \
    --capability filter \
    --provider anthropic \
    --model claude-haiku-4-5 \
    --system-prompt @prompts/regime-filter.txt
  ```

- **Wire it into the strategy:**

  ```
  xvn strategy add-filter <strategy_id> \
    --filter-agent <agent_id> \
    --gates trader \
    --when '{"signal":"regime_filter","field":"regime","op":"eq","value":"high_vol"}'
  ```

  `--gates` is the role label of the downstream agent the Filter
  gates. The Filter agent gets added to the strategy's `agents` list,
  and a new `PipelineEdge` with the given predicate connects them.

- **Remove a Filter from a strategy:**

  ```
  xvn strategy remove-filter <strategy_id> --role <filter_role>
  ```

  Removes the Filter agent and every edge originating from it. Other
  Filter agents and their edges are untouched.

The `--when` argument takes a JSON-serialized `EdgePredicate`. The
schema matches what the SPA composer produces, which is useful when
copying a predicate out of a working strategy. There is no DSL parser at
the CLI; for multi-line or complex predicates, use the SPA composer.

## Inline deterministic Filter DSL

Strategies can also carry an inline deterministic filter under
`strategy.filter`, installed by `xvn strategy set-filter <strategy_id>
--from-json <path>`. This is the path used when the filter is pure
indicator logic rather than a Filter-capable LLM agent.

See [Filter DSL Catalog](/docs?slug=filter-dsl-catalog) for the
authoritative indicator/operator list and copyable JSON examples.
Agents and chat rail should call `xvn strategy filter-catalog --json`
before generating a payload. The important contracts are:

- operators are `>`, `<`, `>=`, `<=`, `==`, `crosses_above`,
  `crosses_below`, `between`, plus parameterized operators such as
  `above_for_<bars>`, `crossed_above_<bars>`, `slope_gt_<bars>`,
  `zscore_gt_<period>`, and `within_pct_<pct>`
- `crosses_above` and `crosses_below` require indicator operands on
  both sides
- use canonical tokens such as `ema_12`, `macd_hist`, `macd_12_26_9`,
  `adx_14`, `di_plus_14`, `bb_pct_b_20`, `donchian_upper_20`,
  `opening_range_high_30`, `rvol_tod_20`, and `volume_zscore_20`
- every inline filter must include `display_name`, `asset_scope`,
  `timeframe`, and a non-empty `conditions` tree
- optional `fire` metadata (`reason`, `priority`, `tags`, `context`)
  adds compact trigger context to traces and trader briefings when the
  gate is active; it does not change pass/fail semantics

## What firing conditions are not

- **Not risk gates.** Risk lives in the executor stage (`RiskDecision`
  approving/modifying/vetoing a `TraderDecision`). A firing condition
  decides whether the Trader runs at all; risk decides what happens
  to its output.
- **Not a hard cooldown.** If you want a Trader that fires at most
  once per N bars regardless of regime, use a Filter agent whose
  signal-cache granularity is `Bar` and whose predicate consults a
  bar-counter. The composer makes this composable; the engine doesn't
  ship a "cooldown" primitive separately.
- **Not a tool-use restriction.** Tool allowlists live on the agent
  template (`AgentSlot.allowed_tools`). A firing condition gates
  dispatch; tool allowlists gate which tools the agent can call when
  it does dispatch.

## Cost framing

The `xvision-filters` crate ships a constant
`AVG_BRIEFING_TOKEN_COST` (currently 50,000 tokens) used to estimate
the LLM cost saved by a firing condition versus the every-bar
baseline. A Filter-gated strategy that fires on 1 in 20 bars saves
roughly 19 x 50,000 = 950,000 input tokens per 20-bar window - the
order of magnitude that justifies the Filter authoring overhead.

A future revision will replace the constant with a per-strategy
measurement so the savings number tracks actual briefing sizes.

## See also

- `docs/superpowers/specs/2026-05-22-capability-first-agent-model-and-graph-composition.md` -
  the engine spec that introduces `Capability::Filter`, `FilterGranularity`,
  and `PipelineEdge.condition`.
- `docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md` -
  the operator-surface spec this page accompanies.
- [Filter DSL Catalog](/docs?slug=filter-dsl-catalog) - inline
  deterministic filter indicators, operators, and examples.
- `crates/xvision-filters/` - the deterministic DSL filter substrate;
  used by Filter-capable agents and (eventually) authorable directly.
