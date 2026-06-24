# Quickstart

Welcome to xvision — a multistrategy trading-agent backtest harness.

## In two minutes

1. **Configure a provider.** Open Settings → Providers and add at least one
   LLM provider (Anthropic, OpenAI-compatible, etc.). The dashboard reads
   from `$XVN_HOME/config/default.toml` and will accept zero-provider
   startup, but no agent can run until one is configured.
2. **Create a scenario.** Scenarios define a market window: asset, date
   range, granularity, fees, slippage, latency. The `/scenarios/new`
   form gives you a quick start with safe defaults.
3. **Create a strategy.** A strategy composes one or more agents
   (`Agent` = prompt + model + skills). The Strategies page exposes a
   template picker; start from "trend follower" or "custom" and tune.
4. **Run an eval.** Pick a strategy + scenario and launch a backtest or
   paper run. The Eval Runs list streams progress; click in to see the
   decisions list, equity curve, and trace dock.

## What the dashboard is for

xvision is the eval harness, not the strategy itself. Use it to:

- Iterate on strategies in a deterministic backtest harness.
- Compare eval runs side-by-side (`xvn eval compare`).
- Inspect every agent decision in the trace dock — prompt, response,
  tool calls, model cost, span timings.

## Surfaces to know

- **Strategies** — author, validate, list saved `Strategy` artifacts.
- **Scenarios** — define market windows.
- **Eval Runs** — launch and watch backtests; results land in
  `RunStore` (SQLite).
- **Agents** — manage the reusable agent library
  (`Vec<AgentSlot>` per agent).
- **Settings** — providers, brokers, observability retention.

## Glossary

- `cycle_id` — one decision cycle (briefing → decision → outcome).
- `agent_id` — local id of a marketplace pipeline.
- `Strategy` — immutable pipeline configuration.
- `RiskDecision` — risk gate verdict (Approved / Modified / Vetoed).

See **CLI Reference** for the `xvn` command surface.
