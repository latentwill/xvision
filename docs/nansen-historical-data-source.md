# Nansen historical data source option for Xvision scenarios

Date: 2026-05-11
Context: scenario generation and eval-run feature engineering for Xvision
Primary external source: Nansen API documentation, especially Smart Money Historical Holdings and Token God Mode indicators

## Executive summary

Nansen can be a valuable **historical feature data source** for Xvision scenarios, especially for on-chain / wallet-behavior signals that Alpaca cannot provide.

The clean integration is not "Nansen as a venue." It is:

```text
Alpaca = historical price bars + simulated execution venue settings
Nansen = historical on-chain feature bars aligned to the same scenario clock
```

This lets Xvision replay a market scenario bar-for-bar while also providing the agent with point-in-time on-chain context:

```text
2024-03-01 00:00 UTC
  Alpaca bar:  ETH/USD OHLCV
  Nansen bar:  smart-money holdings, holders_count, value_usd, balance_24h_change
  Agent sees:  price + on-chain context available at that time
```

The key constraint: Nansen has clearly documented historical support for **Smart Money Historical Holdings**. Its documented **Token God Mode indicators** endpoint appears to be current-state only, so historical use of those indicators requires either Nansen confirmation of a historical endpoint or Xvision-owned daily caching going forward.

## What Nansen clearly offers historically

Documented endpoint:

```text
POST https://api.nansen.ai/api/v1/smart-money/historical-holdings
```

Documented properties:

- Daily snapshots
- Maximum 4-year rolling lookback
- Date-range filtering
- End-of-day UTC snapshots
- Current day excluded because it would be incomplete
- Point-in-time pricing, not recalculated with future prices
- Suitable for trend analysis, performance attribution, and backtesting

Useful response fields for Xvision:

- `date`
- `chain`
- `token_address`
- `token_symbol`
- `token_sectors`
- `smart_money_labels`
- `balance`
- `value_usd`
- `balance_24h_percent_change`
- `holders_count`
- `share_of_holdings_percent`
- `token_age_days`
- `market_cap_usd`

Supported chains in the historical holdings docs include:

- `ethereum`
- `base`
- `bnb`
- `monad`
- `solana`

Smart Money label filters include:

- `Fund`
- `Smart Trader`
- `30D Smart Trader`
- `90D Smart Trader`
- `180D Smart Trader`
- `Smart HL Perps Trader`

## What Nansen does not clearly offer historically

The documented Token God Mode indicators endpoint is:

```text
POST https://api.nansen.ai/api/v1/tgm/indicators
```

It returns current risk/reward indicators for a token, including:

Risk indicators:

- `btc-reflexivity`
- `liquidity-risk`
- `concentration-risk`
- `token-supply-inflation`

Reward indicators:

- `chain-tvl`
- `trading-range`
- `price-momentum`
- `chain-fees`
- `protocol-fees`
- `cex-flows`
- `funding-rate`

Each indicator includes:

- `score`
- `signal`
- `signal_percentile`
- `last_trigger_on`

But the documented request body is only:

```json
{
  "chain": "ethereum",
  "token_address": "0x..."
}
```

There is no documented `date_range`, `from`, `to`, or `granularity` parameter. So for historical backtests, treat this as **current context only** unless Nansen confirms a historical version.

## Proposed scenario schema extension

Keep the existing world definition, but allow feature data sources alongside market data:

```rust
Scenario {
    id,
    display_name,
    description,
    asset,                         // Vec<String>, v1 len == 1
    time_window: { from, to },
    granularity,                   // Hour1 | Day1 in v1
    venue: VenueSettings {
        venue: Alpaca,
        fees: { maker_bps, taker_bps },
        slippage: SlippageModel,
        latency: LatencyModel,
    },
    data_source: AlpacaHistorical,
    feature_sources: Vec<FeatureSource>,
    replay_mode: Continuous,
    created_at,
    created_by,
}
```

Feature source enum:

```rust
enum FeatureSource {
    NansenSmartMoneyHistoricalHoldings {
        chain: Chain,
        token_address: String,
        token_symbol: Option<String>,
        labels: Vec<NansenSmartMoneyLabel>,
        include_stablecoins: bool,
        include_native_tokens: bool,
        alignment: FeatureAlignment,
    },
}

enum FeatureAlignment {
    AsOfPreviousSnapshot,
    SameDayEodOnly,
}
```

Recommended default:

```rust
FeatureAlignment::AsOfPreviousSnapshot
```

Reason: Nansen daily snapshots are end-of-day UTC and generally available the following morning. For a no-lookahead backtest, an intraday Alpaca bar should only see the latest Nansen snapshot that would have existed at that time.

## Bar-for-bar pairing with Alpaca

This is the interesting part.

Xvision can construct a single replay stream that joins price bars and feature bars by timestamp:

```rust
struct ReplayBar {
    ts: DateTime<Utc>,
    asset: AssetSymbol,
    market: MarketBar,                  // Alpaca OHLCV
    features: ScenarioFeatureFrame,      // Nansen point-in-time features
}

struct ScenarioFeatureFrame {
    as_of: DateTime<Utc>,
    source: FeatureSourceId,
    values: serde_json::Value,
}
```

Example joined bar:

```json
{
  "ts": "2024-03-01T13:00:00Z",
  "asset": "ETH/USD",
  "market": {
    "open": 3432.10,
    "high": 3477.12,
    "low": 3419.88,
    "close": 3461.44,
    "volume": 12345.67
  },
  "features": {
    "nansen.smart_money.value_usd": 1800000000,
    "nansen.smart_money.holders_count": 942,
    "nansen.smart_money.balance_24h_percent_change": 2.7,
    "nansen.smart_money.share_of_holdings_percent": 4.3,
    "nansen.as_of": "2024-02-29"
  }
}
```

For `Day1` scenarios, the join is straightforward: one Alpaca daily bar plus one Nansen daily snapshot.

For `Hour1` scenarios, use the previous completed daily Nansen snapshot for all intraday bars until the next snapshot becomes available.

## No-lookahead alignment rules

Nansen historical data is point-in-time, but Xvision still needs explicit alignment rules.

Recommended rules:

1. Nansen snapshot date represents end-of-day UTC state.
2. Snapshot for date `D` is not available during day `D`.
3. For any Alpaca bar timestamp `T`, select the latest Nansen snapshot with:

```text
snapshot_available_at <= T
```

4. Approximate availability as `D + 1 day at 07:00 UTC` unless Nansen exposes exact availability metadata.
5. Store both:

```text
feature_snapshot_date
feature_available_at
```

This makes backtests honest and reproducible.

## Candidate derived features

Raw Nansen fields are useful, but Xvision should also compute normalized features during feature-cache construction.

Suggested v1 derived features:

```text
smart_money_value_usd
smart_money_value_usd_1d_delta
smart_money_value_usd_7d_delta
smart_money_value_usd_30d_delta
smart_money_balance
smart_money_balance_1d_delta
smart_money_balance_7d_delta
smart_money_balance_30d_delta
smart_money_holders_count
smart_money_holders_7d_delta
smart_money_share_of_holdings_percent
smart_money_share_7d_delta
smart_money_balance_24h_percent_change
smart_money_market_cap_usd
smart_money_value_to_market_cap
```

For agent prompts, expose these in a compact form:

```text
Nansen Smart Money context as of 2024-02-29:
- Smart Money value held: $1.80B, +2.7% over 24h
- Holders count: 942, +31 over 7d
- Share of Smart Money holdings: 4.3%
- Value / market cap: 1.8%
```

For non-LLM strategies, expose them as numeric columns in the replay bar.

## Storage model

Add a feature cache separate from the Alpaca bar cache.

```sql
CREATE TABLE feature_cache_nansen_smart_money_holdings (
    chain TEXT NOT NULL,
    token_address TEXT NOT NULL,
    token_symbol TEXT,
    snapshot_date DATE NOT NULL,
    available_at TIMESTAMP NOT NULL,
    labels_json TEXT NOT NULL,
    token_sectors_json TEXT,
    balance REAL,
    value_usd REAL,
    balance_24h_percent_change REAL,
    holders_count INTEGER,
    share_of_holdings_percent REAL,
    token_age_days INTEGER,
    market_cap_usd REAL,
    raw_json TEXT NOT NULL,
    fetched_at TIMESTAMP NOT NULL,
    PRIMARY KEY (chain, token_address, snapshot_date, labels_json)
);
```

Potential derived-feature table:

```sql
CREATE TABLE scenario_feature_bars (
    scenario_id TEXT NOT NULL,
    asset TEXT NOT NULL,
    bar_ts TIMESTAMP NOT NULL,
    source TEXT NOT NULL,
    feature_as_of DATE NOT NULL,
    feature_available_at TIMESTAMP NOT NULL,
    features_json TEXT NOT NULL,
    PRIMARY KEY (scenario_id, asset, bar_ts, source)
);
```

The second table can be materialized at scenario creation or lazily generated at eval-run start.

## CLI shape

Feature cache fetch:

```text
xvn features fetch nansen-smart-money \
  --chain ethereum \
  --token-address 0x... \
  --from 2024-01-01 \
  --to 2024-06-30 \
  --labels "Fund,Smart Trader"
```

Scenario creation with feature source:

```text
xvn scenario create \
  --name "ETH smart-money bull test" \
  --asset ETH/USD \
  --from 2024-01-01 \
  --to 2024-06-30 \
  --granularity Hour1 \
  --venue alpaca \
  --data-source alpaca-historical \
  --feature-source nansen-smart-money:ethereum:0x...
```

Preview joined bars:

```text
xvn scenario preview <scenario_id> --with-features --limit 20
```

## Dashboard UX

In the scenario wizard, Nansen should appear under **Feature Sources**, not venue or market data.

Minimal UI:

```text
Feature sources
[ ] Nansen Smart Money historical holdings
    Chain: Ethereum
    Token address: 0x...
    Labels: Fund, Smart Trader
    Alignment: Previous available snapshot
```

On the eval run page:

- show Nansen feature badges in the run metadata
- add a feature lane below the price chart
- allow overlaying Smart Money accumulation/delta against price
- allow filtering losing trades by feature regime, e.g. "show trades opened when Smart Money 7d delta < 0"

## Implementation staging

### Phase N1 — Documentation and schema only

- Add `FeatureSource` to scenario design doc.
- Document that v1 execution remains Alpaca historical + simulated fills.
- Do not block M1/M2/M3 on Nansen.

### Phase N2 — Cache Nansen historical holdings

- Add `xvision-data` Nansen API client.
- Add `feature_cache_nansen_smart_money_holdings` table.
- Add `xvn features fetch nansen-smart-money`.
- Verify no-lookahead date alignment.

### Phase N3 — Join feature bars to replay bars

- Add feature-frame support to `BacktestExecutor` / eval replay loop.
- Add `scenario_feature_bars` materialization.
- Expose feature values in agent decision context.

### Phase N4 — Dashboard overlays and analysis

- Price chart + Nansen overlays.
- Decision breakdown by Nansen feature regimes.
- Feature-aware run comparison.

## Risks and caveats

- **Coverage mismatch:** Alpaca assets and Nansen token addresses need explicit mapping.
- **Granularity mismatch:** Alpaca can be hourly/daily; Nansen historical holdings are daily.
- **Lookahead risk:** using same-day end-of-day Nansen data during that same day would contaminate backtests.
- **API limits and credits:** Nansen endpoints consume credits; cache aggressively.
- **Beta endpoint warning:** historical holdings docs mark the endpoint as beta, so schemas or calculations may change.
- **Current-only indicators:** Token God Mode indicators are useful, but not historical unless Nansen provides historical access or Xvision starts caching from now on.

## Bottom line

Yes: Nansen is a strong optional historical data source for Xvision, especially if paired bar-for-bar with Alpaca.

The recommended semantics are:

```text
AlpacaHistorical -> market bars
NansenSmartMoneyHistoricalHoldings -> on-chain feature bars
BacktestExecutor -> continuous replay over joined market + feature bars
```

This gives Xvision something more interesting than normal OHLCV backtesting: scenarios where agents can react to price action and historically accurate on-chain Smart Money context without using future information.
