# Strategy filters

A strategy filter is a deterministic rule that decides whether a strategy
should run on a candle.

Without a saved filter, the strategy runs on every candle. With a saved
filter, XVN checks the filter first. If the filter passes, the strategy can
call its model. If the filter does not pass, the strategy skips that candle.

## Execution labels

- **Default: Filter-gated agent** — the saved deterministic filter decides
  whether the configured agent/model is called. A skipped candle is expected;
  a passed filter still requires a launchable agent capability.
- **Advanced: Rules-only mechanical** — deterministic rules decide without a
  model call. This is intentional no-agent execution, not a broken or missing
  agent, and should be described explicitly in the strategy/report.
- **Legacy/discouraged: Agent-direct** — the model is called without a saved
  filter gate. Prompt wording that says "filter" is not enough; only a saved
  filter artifact makes the strategy filter-gated.

Before launching an eval for a filter-gated agent, use the safe CLI path:
provider readiness (`xvn doctor`, `xvn provider list`, `xvn provider check`,
`xvn provider models`) → `xvn strategy diagnostics` → `xvn eval validate` →
`xvn eval run`.

## Where to edit

Open the strategy page at `/strategies/:id`, then use the **Filter** card.

- **Save filter** stores the JSON filter on the strategy.
- **Insert JSON example** fills the editor with a starter filter.
- **Clear filter** removes the filter and returns the strategy to every-candle behavior.

Filters are JSON only.

## Example

```json
{
  "id": "filter-upswing-v1",
  "strategy_id": "strategy-id",
  "display_name": "Upswing filter",
  "description": "Run only when fast EMA is above slow EMA.",
  "asset_scope": ["BTC/USD"],
  "timeframe": "1h",
  "conditions": {
    "all": [
      { "lhs": "ema_20", "op": ">", "rhs": "ema_50" }
    ]
  },
  "cooldown_bars": 3
}
```

Replace `strategy-id` with the strategy ID shown on the strategy page. Match
`asset_scope` and `timeframe` to the strategy you intend to evaluate.

## What to expect

After saving, the Filter card should show that a filter is saved. Future evals
for that strategy will use the filter before dispatching the model.

If an eval looks like it ignored the filter, check the eval run detail for
filter events and summaries. A run with no filter events usually means the
strategy did not have a saved filter when the eval started.
