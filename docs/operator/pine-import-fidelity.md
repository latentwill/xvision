# Pine Script Import — Fidelity Report & Cost Model Vocabulary

**WU10 — cost-model vocabulary as a fidelity reference.**

When you import a Pine Script strategy, xvision emits a `FidelityReport`
describing how each Pine element was handled (captured / approximated / dropped).
The report also includes a `cost_model` block — a labelled reference that
surfaces xvision's backtest cost assumptions using the same vocabulary as
TradingView's Strategy Tester, so you can anticipate why the two tools will
produce different P&L numbers.

---

## cost_model block

The `cost_model` block appears in every `FidelityReport`. Example (JSON):

```json
{
  "captured": [...],
  "approximated": [...],
  "dropped": [...],
  "cost_model": {
    "commission_type": "Percent of order value",
    "commission_value_bps": 10.0,
    "slippage_model": "Linear (flat basis points)",
    "slippage_value_bps": 2.0,
    "fill_timing": "Next bar open",
    "note": "These are xvision's DEFAULT backtest cost assumptions ..."
  }
}
```

These are the **default** values (used when no Scenario has been assigned to
the imported strategy). Each field is configurable via `VenueSettings` on the
Scenario the strategy runs against.

---

## xvision ↔ TradingView vocabulary mapping

| TradingView Strategy Tester field          | xvision field / type                          | Default value              |
|--------------------------------------------|-----------------------------------------------|----------------------------|
| Commission type: "Percent of order value"  | `commission_type`                             | "Percent of order value"   |
| Commission (%)                             | `commission_value_bps` ÷ 100                  | 10 bps = 0.10%             |
| Slippage (ticks)                           | `slippage_model` + `slippage_value_bps`       | Linear, 2 bps flat         |
| Fill orders on bar: "Open"                 | `fill_timing`                                 | "Next bar open"            |
| Initial capital                            | `Scenario.capital.initial`                    | per scenario (configurable)|
| Order size (% equity / fixed / contracts)  | `Strategy.risk_config`                        | per strategy               |
| Pyramiding                                 | not supported — appears in `dropped`          | n/a                        |

### Where the defaults come from (source of truth)

All default values are sourced from `VenueSettings::default()` in
`crates/xvision-engine/src/eval/scenario.rs`:

```
VenueSettings::default() {
    fees: Fees { maker_bps: 0, taker_bps: 10 },        // line ~515
    slippage: SlippageModel::Linear { bps: 2 },          // line ~519
    fill_model: FillModel {
        market_order_fill: MarketOrderFill::NextBarOpen, // line ~522
        ...
    },
    latency: LatencyModel { decision_to_fill_ms: 500 },  // line ~521
}
```

---

## Why xvision P&L will differ from TradingView

| Factor                     | TradingView default              | xvision default                 |
|----------------------------|----------------------------------|---------------------------------|
| Commission                 | 0% (no commission by default)    | 10 bps taker (0.10%)            |
| Slippage                   | 1–2 ticks (configurable)         | 2 bps flat linear               |
| Fill timing                | Intrabar (at signal bar close)   | Next bar open                   |
| Short financing            | Not modelled                     | 5 bps/day borrow cost           |

To narrow the P&L gap: set the Scenario's `fees.taker_bps` to match the
TradingView commission setting, and set `slippage` to `None` if the source
script was backtested with no slippage.

---

## Advanced slippage models

xvision supports three `SlippageModel` variants (configurable per Scenario):

| Model                  | Description                                   | TV equivalent                         |
|------------------------|-----------------------------------------------|---------------------------------------|
| `Linear { bps }`       | Flat bps applied to every fill (default: 2)   | Fixed ticks slippage                  |
| `None`                 | No slippage — fills at next-bar open exactly  | Slippage = 0 ticks                    |
| `VolumeShare { ... }`  | Quadratic impact model (price_impact × vol²)  | n/a — TV has no volume-impact model   |

For most imported strategies `Linear` with 2–5 bps is appropriate. Use `None`
only when benchmarking against a TradingView script run with zero slippage.

---

## Per-bar cost overrides

xvision also supports optional per-bar cost columns in the Parquet data files
(`fee_bps`, `slip_bps`, `spread_bps`). When present these override the Scenario
defaults on a bar-by-bar basis, enabling regime-aware or time-of-day-aware cost
modelling. The `cost_model` block in the fidelity report always reflects the
Scenario-level defaults; per-bar overrides are visible in the per-fill trace.

---

## Fidelity report structure (full reference)

| Field          | Type              | Populated by                                     |
|----------------|-------------------|--------------------------------------------------|
| `captured`     | `Vec<FidelityItem>` | Entry rules, close policies, filter conditions |
| `approximated` | `Vec<FidelityItem>` | Arithmetic approximations, agentic-fallback indicators |
| `dropped`      | `Vec<FidelityItem>` | Pyramiding, HTF/multi-timeframe, unmapped constructs |
| `cost_model`   | `CostModelReference` | WU10 — always populated with DEFAULT values   |

Each `FidelityItem` has:
- `item` — short identifier (e.g. `"entry_rule:Long"`, `"pyramiding"`)
- `reason` — human-readable explanation of how it was handled

The `cost_model` block is `#[serde(default)]` — pre-WU10 JSON that lacks the
`cost_model` key will deserialize successfully with default values applied.
