# xianvec-engine

Strategy creation, bundling, and inline agent execution for xvn.

See specs:
- `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md`

## What ships in MVP (this crate, v0.1)

- Strategy bundle types (manifest + slots + risk + mechanical params)
- 1 template: `mean_reversion`
- 1 migrated baseline: `ma_crossover` (LLM-shimmed)
- `ToolRegistry` with `ohlcv` and `indicator_panel` tools (fixture-mode)
- 3-slot agent pipeline (regime → intern → trader), inline execution
- `LlmDispatch` trait + Anthropic + Mock implementations
- Token estimator
- CLI: `xvn strategy new | validate | ls | show | templates | run`

## What does NOT ship in MVP

- Web dashboard / Agent Wizard (Plan #2)
- MCP server (Plan #2)
- Tier B sealing + xvn API server (Plan #2)
- Durable scheduler (Plan #2)
- Live execution daemon (Plan #2)
- Eval harness (Plan #3)
- More than 1 template + 1 baseline (Plan #2)

## CLI quick-start

```bash
# create a draft
xvn strategy new --template mean_reversion --name eth-mr-v1
# → 01H8N7ZAB...

# validate
xvn strategy validate 01H8N7ZAB...

# inspect
xvn strategy show 01H8N7ZAB...

# run inline against the test fixture (mock LLM = no API cost)
xvn strategy run 01H8N7ZAB... --fixture test-fixture-btc-2024-01 --decisions 5 --mock

# run with real LLM (requires ANTHROPIC_API_KEY)
ANTHROPIC_API_KEY=$(op read 'op://Personal/Anthropic API/credential') \
  xvn strategy run 01H8N7ZAB... --fixture test-fixture-btc-2024-01 --decisions 5
```

Strategies are stored under `$XVN_HOME/strategies/<id>.json` (default `~/.xvn/strategies/`).
