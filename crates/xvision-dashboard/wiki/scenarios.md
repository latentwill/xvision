# Scenarios

A scenario is a saved market window that strategies get backtested against. It pins every variable that affects the bar stream and the simulated fills: asset, date range, granularity, starting capital, fees, slippage, fill latency, and how many pre-window warmup bars to pre-fetch. The same scenario can drive any number of strategies through identical bars and execution conditions, making results comparable.

---

## What a scenario is

A scenario captures:

- **Asset** — which symbol is traded (e.g. `ETH/USD`)
- **Date window** — inclusive start and end dates
- **Granularity** — bar size: `1m` `5m` `15m` `30m` `1h` `4h` `6h` `12h` `1d` `1w` `1mo`
- **Fees** — maker and taker fees in basis points (defaults: 10 bps maker, 25 bps taker)
- **Slippage** — `linear:<bps>` or `none` (default: `linear:5`)
- **Latency** — simulated fill latency in milliseconds (default: 500 ms)
- **Context bars (warmup)** — bars pre-fetched before the window start so indicators have real history at decision t=0 (default: 200)
- **Tags and notes** — free-form labels and annotations

Scenarios are immutable after creation. To change a field, clone the scenario and pass override flags. The clone records the parent id so the lineage stays traceable.

---

## Author a scenario from the dashboard

Navigate to `/scenarios/new`. The form has these fields:

| Section | Field | Notes |
|---|---|---|
| — | **Name** | Display name, required |
| — | **Notes** | Optional free-text annotation |
| — | **Tags** | Press Enter after each tag to add it |
| Market | **Asset** | Dropdown of supported Alpaca crypto symbols |
| Market | **Quote** | Fixed to USD |
| Market | **Backtest window** | Start and end date picker |
| Market | **Calendar** | `Continuous (24/7)`, `US equities`, or a custom calendar id |
| Market | **Granularity** | Bar size dropdown |
| Market | **Context bars** | Pre-window bars for indicator warmup; default 200 |
| Venue (Advanced) | **Fees maker (bps)** | Default 10 |
| Venue (Advanced) | **Fees taker (bps)** | Default 25 |
| Venue (Advanced) | **Slippage (linear bps)** | Default 5 |
| Venue (Advanced) | **Latency (ms)** | Default 500 |

The **Estimated bars to fetch** counter at the bottom of the form updates live as you change the window and granularity. It sums the decision-window bars and the context bars. If it reads 0 after setting the window, the form will refuse to submit — zero-bar scenarios are rejected at the API.

The **Regime range presets** picker in the Market section offers quick-fill date ranges for commonly studied market conditions. These are convenience shortcuts — use `xvn scenario set-regime` or `classify` to attach official regime labels.

Click **Create →** to persist the scenario. The detail page shows the bars-cache status and an asset chart over the window.

---

## Author a scenario from the CLI

### From a file

Create a scenario by loading a full `CreateScenarioRequest` from a JSON or TOML file:

```
xvn scenario create --from-file path/to/scenario.toml
```

Dry-run the file without persisting it:

```
xvn scenario validate --from-file path/to/scenario.toml
```

`validate` exits non-zero and prints errors if the request fails schema or business-logic checks.

### From flags

For quick, ad-hoc scenarios use individual flags. `--name`, `--asset`, `--from`, and `--to` are required; everything else has a safe default:

```
xvn scenario create \
  --name "ETH 4h bull 2024" \
  --asset ETH \
  --from 2024-01-01 \
  --to 2024-06-30 \
  --granularity 4h \
  --fees-maker 10 \
  --fees-taker 25 \
  --slippage linear:5 \
  --latency-ms 500 \
  --warmup-bars 200 \
  --tag regression \
  --tag eth \
  --json
```

| Flag | Default | Notes |
|---|---|---|
| `--name <text>` | — | Required |
| `--asset <ticker>` | — | Required (e.g. `ETH`, `BTC`) |
| `--from <YYYY-MM-DD>` | — | Required; UTC midnight |
| `--to <YYYY-MM-DD>` | — | Required; UTC midnight |
| `--granularity <g>` | `1h` | Bar size |
| `--venue <name>` | `alpaca` | Only `alpaca` in v1 |
| `--fees-maker <bps>` | `10` | Maker fee in basis points |
| `--fees-taker <bps>` | `25` | Taker fee in basis points |
| `--slippage <spec>` | `linear:5` | `linear:<bps>` or `none` |
| `--latency-ms <n>` | `500` | Fill latency in ms |
| `--warmup-bars <n>` | `200` | Pre-window context bars |
| `--tag <label>` | — | Repeatable |
| `--notes <text>` | — | Optional |
| `--json` | — | Emit created scenario as JSON |
| `--from-file <path>` | — | Load full request from file; ignores other flags |

---

## Regime labels

Regime labels tag a scenario with its market character. They let the experiment loop pick comparator sets — for example, "run this hypothesis across three trend-up windows" or "compare two strategies only on high-volatility crash windows."

Three labels exist:

| Label | Values |
|---|---|
| **Regime** | `trend` `chop` `crash` `expansion` `recovery` |
| **Volatility** | `low` `normal` `high` `extreme` |
| **Direction** | `up` `down` `sideways` |

All three start unset. They can be populated in two ways:

### Auto-classify

`xvn scenario classify` derives all three labels from the scenario's cached bar window using built-in heuristics:

```
xvn scenario classify <id>
xvn scenario classify <id> --force
xvn scenario classify --all
```

`--all` processes every scenario that has no regime label set and no operator override. `--force` overwrites even manually set labels. Without `--force`, `classify` skips scenarios that have been operator-set (shown as `(operator)` in the inspect card).

The heuristics are conservative and calibrated on hourly BTC/ETH data. For other markets, set labels manually.

### Set manually

```
xvn scenario set-regime <id> --regime expansion --volatility high --direction up
xvn scenario set-regime <id> --regime crash
```

| Flag | Accepts |
|---|---|
| `--regime` | `trend` `chop` `crash` `expansion` `recovery` |
| `--volatility` | `low` `normal` `high` `extreme` |
| `--direction` | `up` `down` `sideways` |

At least one flag is required. Unspecified labels are left unchanged. Manually set labels are not overwritten by future `classify` runs unless `--force` is given.

---

## Pick a comparator set

`xvn scenario select` is a read-only query that filters the scenario library and returns a list of comparable scenario ids. The output is the input shape that `xvn experiment run` consumes.

```
# Mode A — target a decision count
xvn scenario select --assets ETH/USD,BTC/USD --timeframe 4h --target-decisions 49 --count 4

# Mode B — normalise by shared decision count
xvn scenario select --same-decisions --max-decisions 200 --count 4 --json
```

**Mode A (`--target-decisions <N>`)**: selects scenarios whose decision count falls within ±10% of N. Useful when you want a fixed decision budget per scenario.

**Mode B (`--same-decisions` + `--max-decisions <N>`)**: finds the largest decision count shared by at least `--count` scenarios, capped at N. Useful when comparing strategies over identical decision budgets without hand-picking a number.

Filter flags:

| Flag | Effect |
|---|---|
| `--assets <sym,...>` | Restrict to scenarios whose asset matches any symbol |
| `--timeframe <gran>` | Restrict to a single granularity (e.g. `4h`) |
| `--regimes <label,...>` | Restrict to scenarios whose regime label matches |
| `--target-decisions <N>` | Mode A: select within ±10% of N decisions |
| `--same-decisions` | Mode B: find a common decision count (requires `--max-decisions`) |
| `--max-decisions <N>` | Mode B: cap on the shared decision count |
| `--count <N>` | Max scenarios to return (default 4) |
| `--json` | Emit results as JSON |

With `--json`, output is an array of objects:

```json
[
  { "id": "sc_01JR3...", "name": "ETH 4H bull 2024", "asset": "ETH/USD", "timeframe": "4h", "decision_count": 1262 }
]
```

---

## Inspect

Print a plain-text summary card for a scenario:

```
xvn scenario inspect <id> --card
```

`--card` is required. Example output:

```
id: sc_01JR3PPWB1WE5XKYGEP7NYWRT9
name: ETH 4H bull 2024
asset: ETH/USD
timeframe: 4h
date_window: 2024-01-01..2024-06-30
warmup_bars: 200
decision_bars: 1262
regime:
  label: expansion (auto)
  volatility: normal
  direction: up
previous_runs:
  count: 3
  best_return_pct: 12.47
```

`(auto)` means the labels were set by `classify`; `(operator)` means they were set manually with `set-regime`. `previous_runs` shows how many eval runs have used this scenario and the best total return across them.

For machine-readable output, use `xvn scenario show <id>` (JSON by default, or `--toml` to get a `CreateScenarioRequest`-shaped file suitable for `--from-file`).

---

## CLI verbs at a glance

| Verb | Effect |
|---|---|
| `create` | Create a scenario (flags or `--from-file <path>`) |
| `validate` | Dry-run a request file without persisting |
| `ls` | List scenarios, newest first; archived excluded by default |
| `show` | Print a scenario as JSON; `--toml` emits a `--from-file`-compatible file |
| `clone` | Clone a scenario with optional field overrides (`--name`, `--asset`, `--from`, `--to`, `--warmup-bars`) |
| `inspect` | Emit the plain-text summary card (`--card` required) |
| `select` | Stateless filter: return a comparable id set for experiment runs |
| `classify` | Auto-derive regime, volatility, and direction labels from the bar window |
| `set-regime` | Manually set one or more regime labels |
| `archive` | Soft-delete a scenario (hidden from `ls` by default) |
| `rm` | Hard-delete a scenario and its associated data |
| `tree` | Print the clone ancestry tree for a scenario |

---

## See also

- [CLI Reference](/docs?slug=cli-reference) — full flag reference for every `xvn` command.
- [Experiments](/docs?slug=experiments) — running strategies against scenario sets.
