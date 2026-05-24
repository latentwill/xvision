# Firing conditions

A firing condition controls when a strategy is allowed to call its model.

In XVN, firing conditions are saved as strategy filters. The filter runs before
the model. When the filter passes, the strategy fires. When it does not pass,
the strategy skips that candle without spending model tokens.

## Default behavior

If a strategy has no saved filter, it fires on every candle. This is valid for
strategies that should always evaluate the market, but it can be expensive for
strategies that only need to act in specific conditions.

## Add a firing condition

1. Open the strategy page at `/strategies/:id`.
2. Find the **Filter** card.
3. Paste or write a JSON filter.
4. Click **Save filter**.
5. Run an eval.

The eval run detail should show filter activity when the saved filter is used.

## Remove a firing condition

Open the same **Filter** card and click **Clear filter**. The strategy returns
to every-candle behavior.

## Filter vs risk

A filter decides whether the strategy should call the model on a candle.
Risk controls what happens after the model returns a trade decision. For
example, risk may reduce size, block an order, or force a flat action.

## Inline deterministic Filter DSL

Strategies can also carry an inline deterministic filter under
`strategy.filter`, installed by `xvn strategy set-filter <strategy_id>
--from-json <path>`. This is the path used when the filter is pure
indicator logic rather than a Filter-capable LLM agent.

The authoritative indicator/operator catalog and copyable JSON
examples live at `docs/operator/filter-dsl-catalog.md`. Agents and
chat rail should call `xvn strategy filter-catalog --json` before
generating a payload. The important contracts are:

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

## See also

- `docs/operator/filter-dsl-catalog.md` — exact inline filter indicator
  and operator catalog.
- `docs/operator/filters.md` — current strategy-level filter workflow
  and QA checks.
