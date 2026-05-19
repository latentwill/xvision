# Eval Evidence and Agent Watchers Design

> Status: proposal from 2026-05-19 model-bakeoff review. This is a product/architecture note, not an implementation record.

## Goal

Turn the 30-decision LLM model bake-off review into concrete xvision improvements:

1. Make eval reports say what kind of evidence they actually provide.
2. Make traces audit-grade enough for self-improvement analysis.
3. Reduce token burn by letting deterministic watchers wake an LLM only when market conditions justify a decision.

## Context

The model bake-off was intentionally small: one BTC/USD 1h scenario, 200 warmup bars, 30 decisions, identical prompt family, and six OpenRouter models. It was useful as a smoke test for provider compatibility, strategy/agent wiring, JSON behavior, action discipline, token use, and latency.

It was not enough evidence for strategy quality or deployability. The external review correctly highlighted that the current report can rank model behavior, but it does not yet clearly explain why a result happened, whether there was any signal edge, or how much fees/slippage/token cost dominated the result.

## Balanced assessment of the review

### Strong and immediately relevant

- **Smoke-test labeling.** The bake-off should be labeled as a `model_behavior_smoke_test`, not a strategy benchmark.
- **Abstainer vs active trader separation.** Models that go all-flat preserve capital, but they have not demonstrated alpha.
- **Baselines.** Every eval should compare against flat, buy-and-hold, always-short when allowed, and optionally random/oracle diagnostics.
- **Cost decomposition.** Gross PnL, fee drag, slippage drag, turnover, and net PnL should be first-class report fields.
- **Decision vs execution trace separation.** The current trace blends model request, no-op behavior, fills, closes, and PnL into one row.
- **Requested vs effective action counts.** Action summaries should distinguish model requests from submitted/filled orders and no-ops.
- **Token economics.** Calling the LLM every bar is expensive enough that it can swamp the value of tiny trading edges.
- **Evaluation receipt.** The platform should show evidence quality, statistical power, execution realism, and data-integrity warnings.

### Relevant but later

- **Regret per decision.** Useful for self-improvement, but needs careful labeling because next-bar oracle regret is ex-post and can encourage overfitting.
- **Conviction calibration.** Good with hundreds or thousands of samples; premature for a 30-decision smoke test.
- **Full OHLC/data receipt.** Important for benchmark trust, but less urgent than trace schema and cost decomposition.
- **Strategy compiler mode.** Highly valuable, but should complement rather than replace LLM-in-the-loop agent eval.

### Overstated or needs reframing

- The review spends a lot of energy criticizing the small sample size. That is fair only if xvision presents the run as a serious strategy evaluation. The smoke test itself is still useful.
- All-flat “winners” are not a platform failure. The platform failure would be presenting them as signal-discovery winners instead of safety/abstention baselines.
- Ex-post oracle metrics are useful diagnostics, but must not be surfaced as tradable strategy evidence.

## Evaluation receipt v1

Add an `EvaluationReceipt` to every eval result.

```json
{
  "evaluation_type": "model_behavior_smoke_test",
  "sample": {
    "bars": 30,
    "trades": 4,
    "scenario_count": 1
  },
  "confidence": {
    "strategy_quality": "low",
    "model_behavior": "medium",
    "sharpe_status": "underpowered"
  },
  "execution_realism": {
    "fill_model": "FullAtClose",
    "slippage_model": "linear_5_bps",
    "partial_fills": false,
    "volume_constraints": false,
    "rating": "low_to_moderate"
  },
  "warnings": [
    "single_scenario",
    "short_window",
    "cost_drag_high",
    "not_deployability_evidence"
  ]
}
```

### User-facing copy

For a 30-decision bake-off, show:

> This is a model behavior smoke test. It checks whether models produce valid, disciplined decisions on one short scenario. It is not enough evidence that any strategy has a tradable edge.

## Baseline comparison v1

Every scenario eval should report:

- **Flat baseline:** no trading, 0% return.
- **Buy-and-hold:** long from first decision close to final close, net of configured costs if modeled.
- **Always-short:** short from first decision close to final close, when shorting is supported.
- **Random policy:** optional distribution over many seeds.
- **Oracle/regret diagnostic:** optional, ex-post only, never labeled as tradable.

The report should separate:

- Best abstainer
- Best active trader
- Fastest active trader
- Lowest token cost
- Best risk-adjusted result when metric quality is sufficient

## Cost breakdown v1

Add a `CostBreakdown` to run summaries:

```json
{
  "gross_pnl_usd": 3.46,
  "fees_usd": 25.07,
  "slippage_usd": 2.81,
  "net_pnl_usd": -24.42,
  "turnover_usd": 10029.60,
  "orders_submitted": 10,
  "orders_filled": 10,
  "cost_to_gross_pnl_ratio": 8.05
}
```

This lets users distinguish bad signals from small signals eaten by fees and slippage.

## Decision trace v2

Replace the current blended row with separated concepts.

```json
{
  "decision_index": 12,
  "timestamp": "2025-01-14T04:00:00Z",
  "market_state_ref": "sha256:...",
  "decision": {
    "action_requested": "short_open",
    "conviction": 0.72,
    "rationale": "failed support reclaim",
    "valid_action": true
  },
  "position_before": {
    "side": "flat",
    "size": 0.0,
    "avg_entry": null
  },
  "engine_resolution": {
    "effective_action": "open_short",
    "noop_reason": null,
    "risk_adjustment": null
  },
  "execution": {
    "event_type": "open",
    "order_submitted": true,
    "order_filled": true,
    "fill_price": 95612.10,
    "fill_size": 0.0104,
    "fee_usd": 2.50,
    "slippage_usd": 0.50
  },
  "position_after": {
    "side": "short",
    "size": 0.0104,
    "avg_entry": 95612.10
  },
  "pnl": {
    "realized_trade_pnl_usd": -2.50,
    "unrealized_pnl_usd": 0.0,
    "equity_delta_usd": -2.50
  }
}
```

### Derived action summary

Report both requested and effective behavior:

```json
{
  "requested_actions": {
    "flat": 23,
    "hold": 3,
    "long_open": 2,
    "short_open": 2
  },
  "effective_execution": {
    "submitted_orders": 8,
    "filled_orders": 8,
    "no_ops": 22,
    "duplicate_open_requests": 0,
    "hold_while_flat": 0,
    "position_closes": 4,
    "position_reversals": 0,
    "bars_in_market": 6
  }
}
```

## Token efficiency report v1

Add token and latency economics to eval summaries:

```json
{
  "llm_calls": 30,
  "input_tokens": 701436,
  "output_tokens": 962,
  "input_tokens_per_decision": 23381,
  "output_tokens_per_decision": 32,
  "runtime_seconds": 40,
  "token_efficiency_warning": "high_context_reuse",
  "suggested_mode": "watcher_gated_agent"
}
```

The report should highlight when token spend is wildly disproportionate to net PnL or decision count.

## Agent watchers: the missing platform concept

The review’s most important product implication is that the LLM should not need to wake up every bar.

Users need a way to say:

> Watch the market cheaply. Only wake my agent when the setup it cares about appears.

### Simple user mental model

Use three words:

- **Watcher:** deterministic scanner that checks market conditions every bar.
- **Wake-up:** event created when watcher conditions fire.
- **Agent decision:** LLM call made only after a wake-up, with compact context.

In the UI, describe it as:

> Your watcher is the alarm. Your agent is the trader. The alarm watches every candle; the trader only wakes up when something interesting happens.

### Strategy modes

A strategy should choose one of three activation modes:

1. **Every bar**
   - Current behavior.
   - LLM decides on every decision bar.
   - Best for debugging and pure agent-behavior experiments.

2. **Watcher-gated**
   - Deterministic watcher scans every bar.
   - LLM is called only on trigger bars and while actively managing an open position.
   - Best default for live/paper trading and cost-aware backtests.

3. **Compiled rules**
   - LLM designs or edits rules.
   - Engine executes rules without LLM calls during backtest.
   - LLM returns after the run for diagnosis/revision.
   - Best for large historical sweeps.

## Watcher-gated strategy lifecycle

```text
User prompt
  → Agent drafts strategy intent
  → Agent proposes watcher conditions
  → User reviews watcher card
  → Engine validates/compiles watcher
  → Backtest/live loop scans every bar cheaply
  → Watcher fires wake-up event
  → LLM receives compact context and decides
  → Engine executes/records decision
  → Watcher keeps scanning; agent is called again only if trigger/manage conditions require it
```

## Watcher v1 data model

```json
{
  "watcher_id": "w_...",
  "strategy_id": "s_...",
  "display_name": "EMA pullback wake-up",
  "description": "Wake the trader when trend pullback resumes with volatility expansion.",
  "status": "draft|active|paused|archived",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "scan_cadence": "bar_close",
  "conditions": {
    "all": [
      { "indicator": "ema_20", "operator": ">", "indicator_rhs": "ema_50" },
      { "indicator": "close", "operator": ">", "indicator_rhs": "ema_20" },
      { "indicator": "rsi_14", "operator": "between", "value": [45, 65] },
      { "indicator": "atr_pct", "operator": ">", "value": 0.6 }
    ]
  },
  "cooldown_bars": 3,
  "max_wakeups_per_day": 4,
  "wake_when_in_position": "on_invalidation_or_target_only",
  "agent_context_template": "compact_trade_context_v1"
}
```

## User-facing watcher creation

The user should be able to prompt naturally:

> Create a watcher for this agent. Wake it only when BTC is in an uptrend, price pulls back to EMA20, RSI recovers above 50, and volatility is expanding. Do not wake more than twice per day.

The agent should respond with a readable watcher card, not raw JSON first:

```text
Watcher: Trend Pullback Wake-up
Asset: BTC/USD
Timeframe: 1h
Scans: every bar close
Wake agent when:
  ✓ EMA20 is above EMA50
  ✓ close reclaims EMA20 after a pullback
  ✓ RSI14 crosses back above 50
  ✓ ATR% is above 0.6
Limits:
  • cooldown: 3 bars
  • max wake-ups: 2/day
  • while in position: wake only on invalidation or target
Estimated wake-up rate on this scenario: 4/30 bars
```

Then the user can click:

- **Backtest watcher**
- **Edit conditions**
- **Activate**
- **Run agent every bar instead**

## Watcher authoring flow in the platform

### Step 1: Strategy chat asks the right question

When a user creates a trading agent, ask:

> Should this agent decide every candle, or should we create a watcher that only wakes it up when a setup appears?

Default recommendation:

> Use a watcher unless you are debugging model behavior.

### Step 2: Agent converts prompt to watcher draft

The LLM can propose watcher conditions, but the saved watcher must be deterministic and inspectable.

### Step 3: Engine validates watcher

Validation checks:

- All indicators exist.
- Units are clear.
- Thresholds are numeric.
- No future-looking fields.
- No timestamp/index leakage unless explicitly allowed for synthetic tests.
- Estimated trigger frequency is not too high.
- Cooldown/max wake-up caps are set.

### Step 4: Preview trigger history

Before activation, show historical trigger dots on the chart:

- Triggered bars
- Suppressed by cooldown
- Suppressed because already in position
- Suppressed by daily wake-up cap

### Step 5: Backtest agent with watcher gating

The eval report should compare:

- Watcher scan bars: e.g. 30
- Wake-up events: e.g. 4
- LLM calls: e.g. 4 instead of 30
- Tokens saved: e.g. 86%
- PnL and action summary
- Missed opportunities based on ex-post diagnostic, if enabled

## Watcher trigger types v1

Keep v1 simple and understandable:

1. **Indicator threshold**
   - RSI crosses above/below value.
   - Price above/below EMA.
   - ATR% above threshold.

2. **Indicator relationship**
   - EMA20 > EMA50.
   - Close > Bollinger midline.

3. **Cross/reclaim**
   - Close crosses above EMA20.
   - Price reclaims prior support.

4. **Regime filter**
   - Trending up/down/range/chop from deterministic rules.

5. **Position-management wake-up**
   - Wake when stop, target, invalidation, or opposite signal appears.

Non-goal for v1: arbitrary user code execution. Natural language should compile to a limited safe DSL, not unrestricted scripts.

## Watcher DSL v1

A constrained DSL can power both UI and API:

```toml
[watcher]
name = "Trend Pullback Wake-up"
strategy_id = "..."
asset = "BTC/USD"
timeframe = "1h"
scan = "bar_close"
cooldown_bars = 3
max_wakeups_per_day = 2

[[conditions.all]]
lhs = "ema_20"
op = ">"
rhs = "ema_50"

[[conditions.all]]
lhs = "close"
op = "crosses_above"
rhs = "ema_20"

[[conditions.all]]
lhs = "rsi_14"
op = ">"
value = 50

[[conditions.all]]
lhs = "atr_pct"
op = ">"
value = 0.6
```

The UI can render this as editable condition cards. The agent can author it from natural language, but the user reviews the card before activation.

## Runtime semantics

### Backtest

For every bar:

1. Compute indicators deterministically.
2. Evaluate watcher conditions.
3. If watcher does not fire, no LLM call is made.
4. If watcher fires, build compact decision context and call the agent.
5. If already in position, use management watcher rules to decide whether to call the agent.
6. Record both watcher events and agent decisions.

### Live/paper

Same as backtest, plus:

- durable watcher daemon
- audit log for every trigger/suppression
- wake-up caps
- kill switch integration
- notification when watcher is firing too often or never firing

## Compact agent context v1

When a watcher fires, the agent should receive only what it needs:

```json
{
  "wake_reason": "trend_pullback_reclaim",
  "asset": "BTC/USD",
  "timeframe": "1h",
  "current_bar": {},
  "last_12_bars": [],
  "indicator_snapshot": {},
  "position_state": {},
  "recent_trades": [],
  "watcher_conditions": {
    "passed": [],
    "near_misses": []
  },
  "risk_limits": {}
}
```

Do not resend full warmup bars, static strategy text, or long scenario metadata every time.

## How this stays easy for users

Avoid words like “event-driven predicate graph” in the UI. Use:

- **Watcher** instead of trigger engine.
- **Wake-up** instead of event dispatch.
- **Condition card** instead of DSL node.
- **Alarm history** instead of trigger audit log.
- **Token savings** instead of context efficiency.

Recommended UI copy:

> A watcher checks simple conditions every candle. If nothing interesting happens, your agent sleeps and spends no tokens. When the watcher fires, your agent gets a short market brief and decides what to do.

## V1 acceptance criteria

- A user can create a watcher from natural language and review/edit it as condition cards.
- The saved watcher is deterministic JSON/TOML, not an opaque prompt.
- Backtests can run in `every_bar` and `watcher_gated` modes.
- Eval summaries report number of scanned bars, wake-ups, LLM calls, and estimated tokens saved.
- Decision traces include watcher event rows even when no LLM call occurs.
- Watcher conditions are validated for future leakage and unsupported indicators.
- Reports separate watcher quality from agent decision quality.

## Suggested implementation phases

### Phase 1 — evidence/reporting

- Add evaluation receipt.
- Add baseline comparison.
- Add cost breakdown.
- Add requested/effective action summary.
- Mark Sharpe underpowered for tiny samples.

### Phase 2 — trace schema

- Add `DecisionTraceV2` with decision, position before/after, execution, and PnL sections.
- Keep legacy export compatibility while UI migrates.

### Phase 3 — watcher MVP

- Add watcher schema and validation.
- Add deterministic indicator-condition evaluator.
- Add backtest support for watcher-gated eval.
- Add watcher event records to exports.

### Phase 4 — watcher UX

- Natural-language watcher creation in strategy chat.
- Watcher card editor.
- Chart preview of trigger history.
- Token-savings estimate.

### Phase 5 — live/paper daemon

- Durable watcher scans.
- Audit log.
- Wake-up caps and cooldowns.
- Notification and kill-switch integration.

## Open questions

- Should watcher authoring live under strategies, agents, or deployments?
- Should watchers be reusable marketplace artifacts?
- How do we version watcher DSL changes without breaking old backtests?
- How should watcher performance be scored separately from agent performance?
- Should compiled-rule strategies and watcher-gated agents share the same condition DSL?

## Product recommendation

Add watchers as a first-class platform primitive.

The user-facing story is simple: **the watcher watches; the agent decides**. This gives users an understandable way to reduce token usage without giving up LLM judgment. It also creates a clean bridge between fully deterministic strategies and expensive every-bar LLM agents.
