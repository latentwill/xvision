# Strategies

A living compendium of trading strategies to test in xvision — both
already-implemented and candidates queued for future evaluation.

This folder is the **strategy backlog**. It is not a code directory; the
runnable `Strategy` trait implementations live in
`crates/xvision-eval/src/baselines/` and `crates/xvision-trader/`. Use this
folder to capture *intent*: thesis, parameters, data dependencies, and
expected behaviour before any code is written.

## Format

Each strategy gets its own markdown file: `<slug>.md`. Required sections:

- **Thesis** — one paragraph. What market inefficiency or behaviour is this
  strategy trying to exploit?
- **Inputs** — price/indicator/onchain panels consumed (`PriceFrame`,
  `IndicatorPanel`, `OnchainPanel`, etc.).
- **Parameters** — tuneable knobs and their default ranges.
- **Decision rule** — pseudocode for the buy/sell/flat logic.
- **Expected regime** — when should this work? (trending, ranging, high-vol,
  funding-skewed, etc.)
- **Data dependencies** — feeds, API keys, infra not already in the repo.
- **Status** — `idea` | `queued` | `implemented` | `live` | `deprecated`.
- **References** — links to ADRs, FOLLOWUPS items, papers, or prior work.

## Index

### Implemented (in `crates/xvision-eval/src/baselines/`)

- `always_long` — sanity floor, always buy.
- `always_short` — sanity floor, always sell.
- `buy_and_hold` — single entry, hold for window.
- `ma_crossover` — fast/slow moving-average crossover.
- `macd_momentum` — MACD signal-line crossover.
- `random_direction` — control arm, deterministic-seeded coin flip.
- `rsi_mean_reversion` — RSI oversold/overbought reversion.
- `trader_arm` — LLM-driven (xvision-trader, see ADR 0010 / 0011).

### Queued — Classical TA (FOLLOWUPS SLF7 / F15)

- [`EMA/`](EMA/README.md) — full EMA strategy compendium (atlas of 6 perspective-pages, 8 scoped + 5 idea-pool entries).
- [`bollinger/`](bollinger/README.md) — full Bollinger compendium (spectrum × scale matrix, 9 scoped + 7 idea-pool entries; symmetry-breaking pivot).
- [`rsi_volume/`](rsi_volume/README.md) — RSI + volume two-source verification (map of regions, 8 scoped + 10 idea-pool entries; medical-consensus template).
- `donchian_breakout` — N-bar high/low channel breakouts.
- `fibonacci_retracement` — pullback entries on golden ratios.
- `ma_triple_screen` — Elder triple-screen (trend / oscillator / entry).

### Queued — Onchain / Mantle-native (FOLLOWUPS SLF6 / F14)

- `funding_rate_fader` — fade extreme perp funding.
- `liquidation_cascade_fader` — buy capitulation-flagged liquidations.

### Queued — Idea generators (meta-strategies)

- [`slot_machine/`](slot_machine/README.md) — LLM + ideonomy-driven variant
  generator. Two modes: pull-the-lever factory that emits candidate strategy
  bundles, and tradeable arm that samples a variant per window. Evaluated by
  hit rate + diversity score, not a single equity curve. Depends on the
  strategy creation engine MVP. Design:
  [`docs/superpowers/specs/2026-05-08-slot-machine-design.md`](../docs/superpowers/specs/2026-05-08-slot-machine-design.md).

### Idea pool

(Add candidate strategies here as one-line entries; promote to a full file
when ready to scope.)

- _(empty — add ideas)_

## Workflow

1. **Idea** → add a one-line bullet in the idea pool above.
2. **Queue** → write a full `<slug>.md` using the format above. Status: `queued`.
3. **Implement** → land a `Strategy` impl in `xvision-eval` or `xvision-trader`.
   Update status to `implemented` and link the crate path.
4. **Evaluate** → run through `xvn ab-compare` and the Strategy Loom evening
   cycle. Record receipts via `xvision-identity` (ADR 0008).
5. **Deprecate** → if a strategy is consistently dominated, mark `deprecated`
   with a short post-mortem; do not delete the file.

## See also

- `decisions/0010-hackathon-pivot-strategy-loom.md` — Strategy Loom + ERC-8004
  marketplace context.
- `decisions/strategy-choices.md` — deferred tactical choices that touch
  strategy semantics.
- `FOLLOWUPS.md` — SLF queue (Strategy Loom follow-ups), including SLF6
  (onchain baselines) and SLF7 (TA baselines).
- `architecture.md` §7 — `Strategy` trait surface and arm wiring.
