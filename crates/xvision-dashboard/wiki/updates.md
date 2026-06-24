# What's New

## 2026-06-23 — Live run observability

### Bar storage & chart candles

Live and forward-test runs now persist every OHLCV bar to the database as
it arrives, even during warmup. The eval run chart renders candles
immediately — you no longer have to wait for the first decision to see the
price backdrop.

Behind the scenes: a new `eval_run_bars` table records each bar's OHLCV
as the live executor processes it. The chart API reads from this table
directly, so the frontend chart always has bar data. Bars are also
streamed live via SSE so the chart updates in real time.

### Delayed decision tracking

When the LLM takes longer than one bar period to respond, the decision is
flagged as **delayed** and counted separately in the run summary. This
adds three new fields to every eval run:

| Field | Meaning |
|---|---|
| `skipped_dispatches` | Bars where dispatch was skipped because the agent was still processing a previous bar |
| `delayed_decisions` | Decisions accepted but flagged stale because the model response spanned ≥1 bar period |
| `forced_cancels` | Agents cancelled by the `--max-agent-ms` deadline |

A delayed decision is NOT rejected — it's still recorded and its PnL
counts. The flag is a signal quality indicator: a run with 50% delayed
decisions suggests the model is too slow for the strategy's bar cadence.

### Unrealized PnL display

The eval run detail page now shows unrealized PnL — the mark-to-market
value of open positions — next to the Sharpe ratio. Previously, negative
Sharpe from unrealized losses was visible but the dollar amount wasn't.
The number comes from the server-side book computation (`equity - initial
- realized`), updated every bar in live mode.

### Forward-test UI fixes

- **Filter warmup**: warmup bars are now pushed through the filter's
  indicator engine so the filter exits its Warming state before the first
  tradable bar. Previously, filters that required indicator lookback
  (e.g. RSI-14) would stay in Warming indefinitely.
- **Filter timeline colors**: `in_position` (amber) and `cooldown` (blue)
  ticks are now visually distinct — previously both were near-identical
  grays. See [Firing Conditions](/docs?slug=firing-conditions) for what
  each state means.
- **`delayed` column**: the `eval_decisions.delayed` column now defaults
  to `false` correctly in all code paths.

## 2026-06-13 — Multi-timeframe + Live deployments

### Multi-timeframe strategies

Strategies can now declare multiple timeframes in their manifest. The
agent receives bar history for each declared timeframe, enabling
multi-timeframe analysis in a single dispatch. Configure via the
`timeframe_requirements` array in the strategy JSON.

### CT5 live deployment foundation

- `eval_runs.source` discriminator: `human` (operator-queued) vs
  `optimizer` (autooptimizer-launched). Drives the Cancel-gate in live
  deployments.
- `eval_runs.unrealized_pnl_usd`: mark-to-market PnL persisted every bar
  by the live executor.
- `live_run_state` table: per-run execution-layer truth (equity, realized
  PnL, drawdown, daily loss remaining) written on every bar tick.
- Live deployment dashboard: `/live` surface with per-strategy charts,
  position tables, venue account panels, and capital risk strips.

## Earlier

### Strategy optimizer (2026-06-08)
Autonomous strategy iteration with genetic algorithms, holdout evaluation,
and automated sensitivity probes. See [Optimizer](/docs?slug=optimizer).

### Agent firing filters (2026-05-24)
Condition-tree gates that suppress trader dispatch based on indicator
conditions, reducing LLM cost. See [Firing Conditions](/docs?slug=firing-conditions).

### Agent graph composition (2026-05-22)
Capability-first agent model with pipeline wiring, edge predicates, and
role-based agent selection. See [Agents](/docs?slug=agents).

### In-app docs (2026-05-20)
This wiki shipped as compiled-in markdown pages served from the binary —
no runtime filesystem reads.
