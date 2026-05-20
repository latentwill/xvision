# Scenarios

A `Scenario` defines the market window an evaluation runs against. It is
strategy-agnostic — the same scenario can drive any number of strategies
through the same bars, fills, and execution model.

## What a scenario is

A scenario pins every variable that affects the bar stream and the simulated
fills: which asset, which date range, which granularity, starting capital, fees,
slippage, latency, and how many pre-window warmup bars to pre-fetch. Once
saved, a scenario is immutable by id — cloning produces a new id.

## Anatomy

| Field | Type | Notes |
|---|---|---|
| `id` | string | ULID, immutable |
| `display_name` | string | Operator-visible label |
| `description` | string | Free text |
| `tags` | string[] | Arbitrary labels |
| `notes` | string? | Long-form annotations |
| `asset_class` | enum | e.g. `Crypto`, `Equity` |
| `asset` | AssetRef[] | Symbols in the window |
| `quote_currency` | enum | e.g. `Usd` |
| `time_window` | `{ start, end }` | ISO timestamps, inclusive |
| `granularity` | enum | `1m` `5m` `15m` `30m` `1h` `4h` `1d` `1w` `1mo` |
| `timezone` | string | IANA timezone for the window |
| `calendar` | CalendarRef | Session-hours calendar |
| `data_source` | enum | Bar data provider |
| `venue` | VenueSettings | Exchange / venue config |
| `replay_mode` | enum | Replay behaviour during eval |
| `capital` | `{ initial, currency }` | Starting capital in `currency` |
| `warmup_bars` | u32 | Context bars pre-fetched before `time_window.start`; default 200 |
| `bar_cache_policy` | enum | How the engine caches the bar fetch |
| `regime_label` | string? | Broad market character — see Regime labels |
| `volatility_label` | string? | Per-bar volatility bucket — see Regime labels |
| `trend_direction` | string? | Net price direction — see Regime labels |
| `regime_derived` | bool | `true` = auto-derived; `false` = operator-set |
| `source` | ScenarioSource | `User`, `Clone`, … |
| `parent_scenario_id` | string? | Set when cloned |
| `created_at` | timestamp | |
| `created_by` | string | |
| `archived_at` | timestamp? | Set when archived |

### Warmup bars

`warmup_bars` controls how many bars are pre-fetched from immediately before
`time_window.start`. These bars are available to indicators and the trader's
`bar_history` slice at decision bar 1 so lookback windows have real data from
the start. The default is `200`, matching typical indicator lookbacks. The
dashboard's "Estimated bars to fetch" display sums `decision_bars + warmup_bars`.

## Regime labels

Wave-C adds three structured label fields to every scenario. They can be set
automatically via `xvn scenario classify` or overridden manually via
`xvn scenario set-regime`.

| Field | Valid values | Meaning |
|---|---|---|
| `regime_label` | `trend` `chop` `crash` `expansion` `recovery` | Broad market character for the window |
| `volatility_label` | `low` `normal` `high` `extreme` | Per-bar log-return volatility bucket |
| `trend_direction` | `up` `down` `sideways` | Net OLS-slope direction over the window |

All three fields are `None` until set. `regime_derived = true` means the labels
were produced by the classify heuristic and can be refreshed. `regime_derived =
false` means an operator set them manually; `classify` will not overwrite them
unless `--force` is given.

**Derivation heuristics** (for reference; the heuristics are conservative and
meant to be overridden for non-crypto assets):

- `trend_direction`: OLS slope of close prices, normalised by the first close.
  Greater than `+0.0005/bar` → `up`; less than `-0.0005/bar` → `down`;
  otherwise `sideways`.
- `volatility_label`: standard deviation of per-bar log-returns.
  Less than 0.5 %/bar → `low`; < 2 % → `normal`; < 5 % → `high`; ≥ 5 % → `extreme`.
  Thresholds are calibrated on hourly BTC/ETH; override for other markets.
- `regime_label`: composed from the other two plus a max-drawdown test.
  Max drawdown > 25 % → `crash`; uptrend with V-shape dip → `recovery`;
  uptrend without dip → `expansion`; sideways → `chop`; downtrend below crash
  threshold → `trend`.

At least 2 bars are required; fewer bars produce all-`None` output.

## Author flow

### Dashboard

Navigate to `/scenarios/new`. Fill in asset, date range, and granularity; the
"Estimated bars" counter updates live. If it reads 0 after setting the window,
the form will refuse to submit — zero-bar scenarios are rejected at the API.
Click **Save** to persist the scenario. The detail route shows the bars-cache
status and an asset chart over the window.

### CLI

Create a scenario from a JSON or TOML file containing a `CreateScenarioRequest`:

```
xvn scenario create --from-file path/to/scenario.toml
```

Dry-run validation without persisting:

```
xvn scenario validate --from-file path/to/scenario.toml
```

The `--from-file` flag loads the full request; individual `--asset`, `--from`,
`--to`, `--granularity` flags are also available for ad-hoc creation.

## Classify and label

Auto-derive regime labels from the scenario's bar window:

```
xvn scenario classify <id>
xvn scenario classify <id> --force
xvn scenario classify --all
```

`classify` fetches the cached bar window and runs the heuristics described
above. If the scenario already has operator-set labels (`regime_derived =
false`), classify skips it unless `--force` is given. `--all` classifies every
scenario that has a `NULL` `regime_label` and no operator override.

To set or override labels manually:

```
xvn scenario set-regime <id> --regime expansion --volatility high --direction up
xvn scenario set-regime <id> --regime crash
```

Flags for `set-regime`:

| Flag | Accepts |
|---|---|
| `--regime` | `trend` `chop` `crash` `expansion` `recovery` |
| `--volatility` | `low` `normal` `high` `extreme` |
| `--direction` | `up` `down` `sideways` |

At least one flag is required. Unspecified labels are left unchanged.
`set-regime` always sets `regime_derived = false`, preventing future
`classify` runs from overwriting the values.

## Select

`xvn scenario select` is a stateless selector that filters the saved scenario
library and returns a comparable set of scenario ids. It does not mutate
anything; the output list is the input shape `xvn experiment run` consumes.

```
# Mode A — target a decision count
xvn scenario select --assets ETH/USD,BTC/USD --timeframe 4h --target-decisions 49 --count 4

# Mode B — normalise by shared decision count
xvn scenario select --same-decisions --max-decisions 200 --count 4 --json
```

**Mode A (`--target-decisions <N>`)**: selects scenarios within ±10 % of N
decisions. Useful when you want a fixed decision budget.

**Mode B (`--same-decisions` + `--max-decisions <N>`)**: finds the largest
decision count shared by at least `--count` scenarios, capped at N. Useful
when comparing strategies over identical decision budgets without picking a
number manually.

Filter flags:

| Flag | Effect |
|---|---|
| `--assets <sym,...>` | Restrict to scenarios whose asset matches any symbol |
| `--timeframe <gran>` | Restrict to a single granularity (e.g. `4h`) |
| `--regimes <label,...>` | Restrict to scenarios whose `regime_label` matches |
| `--target-decisions <N>` | Mode A: select within ±10 % of N decisions |
| `--same-decisions` | Mode B: find a common decision count (requires `--max-decisions`) |
| `--max-decisions <N>` | Mode B: cap on the common decision count |
| `--count <N>` | Max scenarios to return (default 4) |
| `--json` | Emit results as JSON |

When `--json` is set, output is a JSON array of objects with `id`, `name`,
`asset`, `timeframe`, and `decision_count` fields.

## Inspect

Emit a plain-text summary card for a scenario:

```
xvn scenario inspect <id> --card
```

The `--card` flag is required. The card contains:

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

`(auto)` means `regime_derived = true`; `(operator)` means manually set.
`previous_runs` shows how many eval runs have used this scenario and the best
`total_return_pct` across them.

## CLI parity

Every `xvn scenario` verb in one place:

| Verb | Effect |
|---|---|
| `create` | Create a new scenario (flags or `--from-file <path>`) |
| `validate` | Dry-run a `CreateScenarioRequest` file without persisting |
| `ls` | List scenarios, newest first; archived excluded by default |
| `show` | Print a scenario by id (JSON or TOML with `--toml`) |
| `clone` | Clone a scenario with optional field overrides |
| `inspect` | Emit the plain-text summary card (`--card` required) |
| `select` | Stateless selector: filter library, emit comparable id list |
| `classify` | Auto-derive `regime_label` / `volatility_label` / `trend_direction` from bars |
| `set-regime` | Manually set one or more regime labels (`--regime` / `--volatility` / `--direction`) |
| `archive` | Soft-delete a scenario (hidden from ls by default) |
| `rm` | Hard-delete a scenario and its associated data |
| `tree` | Print the clone ancestry tree for a scenario |

## See also

- [CLI Reference](/docs?slug=cli-reference) — full flag reference for every `xvn` command.
- [Experiments](/docs?slug=experiments) — running strategies against scenario sets.
