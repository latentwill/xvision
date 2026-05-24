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
