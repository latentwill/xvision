# Slot Machine — Idea Generator

A meta-strategy that uses LLM-driven *ideonomy* operators (expand, vary, invert, recombine, juxtapose, specialize, generalize) — the operator catalog from the `ideonomy-plain` Claude skill — to mutate a base trading thesis into a pool of variant strategies. Two modes:

- **Generator mode** — pull the lever N times to emit N candidate strategy bundles, routed into the eval engine.
- **Arm mode** — sample one variant per rebalance window from a pre-generated pool and trade it. Functions as a control arm in `xvn ab-compare`.

Status: **idea**. Depends on the strategy creation engine MVP shipping. Full design spec: [`docs/superpowers/specs/2026-05-08-slot-machine-design.md`](../../docs/superpowers/specs/2026-05-08-slot-machine-design.md).

## Why it matters

Unlike normal strategies, the slot machine is evaluated by a **distribution** of outcomes — not a single equity curve. Two metrics define quality:

1. **Hit rate** — fraction of variants that beat baselines.
2. **Diversity score** — variance of per-variant returns.

A slot machine that emits a tight cluster of near-identical variants is broken. One that emits a wide distribution with a heavy positive tail is the goal. The right tail is where novel alpha lives.

## Format for scoped slot-machine instances

Each scoped instance gets its own `<base_thesis>_slot.md` file in this folder. Required sections (extends the strategy template format from [`../README.md`](../README.md)):

- **Base thesis** — the seed strategy idea the slot machine mutates. One paragraph.
- **Operator subset** — which ideonomy operators are enabled for this instance, and why (some operators don't make sense for some theses).
- **Pull budget** — default `K` per generator run, max budget for the variant pool.
- **Inputs** — same as a normal strategy file (price/indicator panels the variants will be allowed to consume).
- **Variant constraints** — hard rules every emitted variant must satisfy (e.g. must declare a stop-loss, must trade on the same timeframe as the base).
- **Expected regime** — when should the *family* of variants work? Individual variants may specialize further.
- **Status** — `idea` | `queued` | `implemented` | `live` | `deprecated`.
- **References** — links to the design spec, related strategy files, prior pull-receipt logs.

## Index

### Queued instances

_(empty — add scoped instances below as `<base_thesis>_slot.md` once the engine MVP ships)_

### Idea pool

- `mean_reversion_slot` — first scoped instance to build. Operates on the engine MVP's only template, so it becomes useful as soon as the engine ships. Operator subset: `vary`, `expand`, `specialize`, `invert` (RSI mean-reversion ↔ RSI momentum).
- `funding_skew_slot` — generator over funding-rate-fader theses. Operator subset: `expand` (add OI confirmation, add basis confirmation), `specialize` (per-venue, per-asset class), `juxtapose` (require funding skew AND OI move).
- `nansen_cohort_slot` — generator over smart-money cohort theses (see [`../nansen/`](../nansen/)). Operator subset: `recombine` (splice cohort signals), `specialize` (per-cohort tier), `invert` (fade smart-money instead of follow).
- `bollinger_slot` — generator over Bollinger Band theses (see [`../bollinger/`](../bollinger/)). Operator subset: `vary` (band width, lookback), `recombine` (splice with RSI/volume confirms), `specialize` (per regime).

## Workflow

1. **Idea** — add a one-line bullet to the idea pool above.
2. **Queued** — promote to a full `<base_thesis>_slot.md` once the operator subset and variant constraints are scoped.
3. **Implement** — depends on engine MVP + slot-machine implementation plan landing. The template lives in `crates/xvision-engine/src/templates/slot_machine.rs`.
4. **Evaluate** — runs through `xvn ab-compare --slot-machine <id>`, which reports hit rate, diversity score, and per-variant metrics.
5. **Promote winning variants** — variants that consistently beat baselines get extracted to their own strategy files in the appropriate sibling folder (e.g. `rsi_volume/`, `nansen/`).

## See also

- [`../README.md`](../README.md) — strategies index, base format.
- [`../../docs/superpowers/specs/2026-05-08-slot-machine-design.md`](../../docs/superpowers/specs/2026-05-08-slot-machine-design.md) — full design spec.
- [`../../docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md`](../../docs/superpowers/specs/2026-05-08-strategy-creation-engine-design.md) — engine that the slot machine plugs into.
- [`../../docs/superpowers/plans/2026-05-08-strategy-creation-engine-mvp.md`](../../docs/superpowers/plans/2026-05-08-strategy-creation-engine-mvp.md) — engine MVP implementation plan that must ship before the slot machine.
