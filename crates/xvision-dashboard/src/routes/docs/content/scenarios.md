# Scenarios

A `Scenario` defines the market window an eval runs against. It is
strategy-agnostic — the same scenario can drive any number of
strategies through the same bars / fills.

## Required fields

- **display_name** — operator-visible name (e.g. "ETH 4H bull 2024").
- **asset** — e.g. `BTC/USD`, `SPY`. The asset whitelist is enforced
  on the dashboard's create path; unknown assets are rejected.
- **time_window** — `from` / `to` ISO timestamps.
- **granularity** — one of `1m`, `5m`, `15m`, `30m`, `1h`, `4h`,
  `1d`, `1w`, `1mo`. Sub-hour granularities are gated by the
  scenario API.
- **initial** — starting capital (`capital_usd`).
- **fees / slippage / latency** — execution model parameters.

## Context bars

The "Context bars" field controls how many pre-window bars the
engine fetches before the scenario's `from` timestamp. Defaults to
200 (`DEFAULT_WARMUP_BARS`) which covers typical indicator lookbacks.
The "Estimated bars to fetch" display sums the time-window bar count
plus the context bars.

## Authoring flow

1. `/scenarios/new` — fill the form. The bars-estimate is live; if
   it stays at 0 after picking from/to + granularity, the form will
   refuse to submit (no-op scenarios are rejected at the API).
2. **Save** — the scenario is persisted under
   `$XVN_HOME/scenarios/<id>.json` with an immutable id.
3. **Inspect** — the detail route shows bars-cache status and a
   chart of the asset over the window.

## Common gotchas

- **Bracket-only crypto** — Alpaca crypto rejects bracket orders.
  The executor handles this by submitting the parent order alone and
  recording the bracket-omission in the decision row. No scenario
  change is required.
- **Empty bars cache** — if the dashboard can't reach the data
  provider, the eval surfaces a typed `bars_fetch_failed` failure
  rather than running on stale data. Re-fetch via the scenario
  detail page or run `xvn fetch-bars` directly.
