# xvision-engine

Strategy creation, bundling, and inline agent execution for xvn.

See specs:
- `docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`
- `docs/superpowers/specs/2026-05-08-eval-engine-design.md`

## What ships in v0.2 (Plan 2a)

- Strategy bundle types (manifest + slots + risk + mechanical params) **— v0.1 (Plan #1)**
- 8 templates: `trend_follower`, `breakout`, `mean_reversion`, `momentum`,
  `range_trade`, `scalping`, `news_trader`, `custom`, plus the
  `ma_crossover_baseline` seed
- `ToolRegistry` with `ohlcv` and `indicator_panel` tools (fixture-mode) **— v0.1**
- 3-slot agent pipeline (regime → intern → trader), inline execution **— v0.1**
- `LlmDispatch` trait + Anthropic + Mock implementations **— v0.1, extended for tool-use blocks (Plan 2a 2A.C T10)**
- Multi-turn `LlmRequest` / `LlmResponse` with `Message` + `ContentBlock`
  (`Text` / `ToolUse` / `ToolResult`) + `ToolDefinition` + `StopReason` —
  the surface that `WizardLoop` and Stage-1 Intern tool dispatch build on
- `xvision_engine::authoring` shared dispatcher — `list_templates`,
  `create_strategy`, `get_strategy`, `update_slot`, `set_mechanical_param`,
  `set_risk_config`, `validate_draft`. Both the `xvn-mcp` server and the
  dashboard's `WizardLoop` route through this module.
- Token estimator **— v0.1**
- CLI: `xvn strategy {new | validate | ls | show | templates | run}` **— v0.1**
- **Agents (`engine::agents`):** workspace-level first-class `Agent` entity
  with named slots. Each slot owns its prompt, provider, model, and
  max_tokens directly. Authored at `/agents` / `/agents/:id` in the
  dashboard, backed by `agent_slots` + `agents` tables in `xvn.db`. See
  `docs/superpowers/plans/2026-05-11-agents-page-v1.md`. Replaces the
  Plan 2b in-app "skills" surface that was removed per
  [ADR 0012](../../decisions/0012-deprecate-in-app-skills.md).

## What does NOT ship in v0.2

- Live execution daemon (Plan 2c)
- Durable scheduler (Plan 2c)
- Tier B sealing + xvn API server (Plan 4)
- **Marketplace publish/browse/install/attest (deferred to Plan 5 —
  blockchain integration).** No `xvision-marketplace` crate, no marketplace
  MCP verbs, no `License` / `Listing` / `ReputationReceipt` types,
  no on-chain author identity. Marketplace ships together with the
  on-chain registries when Plan 5 lands, against the `Agent` entity.
- Real news/sentiment tool — `news_trader` template ships with a
  fallback prompt
- Stage-1 Intern in-loop tool dispatch (Plan 2a T11) — the trait now
  carries the shape, but `execute_slot` still single-turns; the
  follow-up wires `tool_use` blocks back through `ToolRegistry`

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

> **Exit codes:** `xvn strategy *` and `xvn eval *` return typed exit codes
> (0 / 2 / 3 / 4 / 5 / 7) — see **Exit codes** in `MANUAL.md`.

