# Global / Pair Signal Producer + Cross-Asset Selector — Design Spec

**Date:** 2026-05-25
**Status:** Design spec. Phase 4 of `2026-05-25-multi-asset-followups.md`.
**Scope of THIS track:** design + contract. The runtime executor wiring is a
follow-up implementation step (see "Implementation plan" — it changes the
sensitive multi-asset fan-out loop and should land on its own, TDD-first).

## Problem

`SignalScope` (`Global` / `Asset` / `Pair` / `Custom`) and `FilterSignal.scope`
already exist (`crates/xvision-engine/src/agent/dispatch_capability.rs`) and are
serialization-tested. But:

1. The filter dispatcher **hardcodes `SignalScope::Global`** in three places
   (`dispatch_filter` DSL + parse-error branches, `filter_dispatch::parse_filter_response`)
   — a filter cannot yet *declare* its scope.
2. The multi-asset backtest fan-out (`backtest.rs`, the
   `for (&ts, assets_at_ts) in &timeline { … for asset … }` loop, ~L767) runs
   the **whole pipeline once per (timestamp, asset)**. A `Global` filter is
   therefore recomputed `N` times per bar for an `N`-asset universe — redundant
   and, for an LLM filter, `N×` the cost.

The Phase 4 producer makes scope a declared, runtime-honored property:
`Global` filters run **once per timestamp** and their signal fans into every
per-asset trader call at that timestamp.

## 1. Scope declaration in config

A Filter declares its output scope on its `AgentRef` / slot config, defaulting
to `Asset` (current fan-out asset) for backward-compat with today's per-asset
behavior — **note:** today's hardcode is `Global`, but per-asset is the correct
default for a fan-out filter; the migration must set existing filters explicitly
so behavior is unchanged.

- Add an optional `signal_scope: Option<SignalScopeDecl>` to the filter's slot
  config (`AgentSlot` / `LLMSlot`), where `SignalScopeDecl` is the
  config-facing projection of `SignalScope` (`global` | `asset` | `pair` |
  `custom`). `Asset`/`Pair` bind to the runtime asset context; only `Global`
  and `Custom(name)` are fixed at config time.
- The dispatcher tags the produced `FilterSignal.scope` from this declaration
  (resolving `Asset` → `Asset(current_asset)`, `Pair` → `Pair(a, b)` from the
  pair context) instead of the current hardcoded `Global`.
- ts-rs export + validation: an unknown scope string is a draft-time validation
  error, not a silent `Global`.

## 2. Global filter execution (the producer)

In the fan-out loop, **partition filters by declared scope** before the
per-asset inner loop:

- **Global** filters: evaluate **once per timestamp**, before the per-asset
  loop. Cache the resulting `FilterSignal` keyed by `(producer_role, ts)`.
- **Asset** filters: evaluate inside the per-asset loop as today (once per
  `(ts, asset)`), tagged `Asset(current)`.
- **Pair** filters: evaluate once per active pair per timestamp (see §4).

Each per-asset trader briefing then merges: that asset's `Asset` signals + all
`Global` signals for the timestamp + any matching `Pair` signals. The
briefing's `filter_signals[name]` surface is unchanged; only the *population*
path differs.

**Non-redundant-recompute invariant:** a `Global` filter's producer is invoked
exactly once per timestamp regardless of `N`. Test with a counting mock
dispatch over a 2-asset, 3-bar run: a `Global` filter must record exactly 3
invocations (not 6); an `Asset` filter records 6.

## 3. Edge predicates + DSL bridge

Edge predicates read `FilterSignal.payload` regardless of scope, so they work
unchanged. The DSL bridge (`xvision_filters::runtime::dsl_to_filter_signal`)
must carry the declared scope through instead of stamping `Global`.

## 4. Pair signals

- A `Pair(a, b)` filter is evaluated once per active unordered pair per
  timestamp; its bar-cache key includes both symbols (pair-specific key) so two
  different pairs don't collide.
- A pair signal appears in a trader briefing **only when the current asset
  context matches** the pair (i.e. the trader for `a` or `b` sees it; the
  trader for an unrelated `c` does not). This is the "scope matches the current
  asset context" acceptance rule.

## 5. Cross-asset selector capability

A `Selector` capability (new `Capability` variant, or a `Filter` whose output
is a ranking) runs **once per timestamp before the Trader fan-out** and emits an
ordered/filtered subset of the active assets. The executor then fans the Trader
out over only the selected subset (a per-timestamp dynamic narrowing, distinct
from the static `assets_subset` run option). Selector output is recorded so a
run shows which assets were considered vs traded each bar.

## 6. Observability labels

Every emitted signal/decision trace carries:
- `signal_scope` (`global` / `asset:BTC` / `pair:BTC-ETH` / `custom:name`),
- `active_assets` for the timestamp,
- selector output (considered set → selected set) when a selector ran.

This keeps `noop_skip` / graph-gated / selector-narrowed decisions
distinguishable in the eval UI (per the engineering invariant on decision
provenance).

## Implementation plan (follow-up, TDD-first)

1. Add `signal_scope` config + `SignalScopeDecl` (+ ts-rs, validation). Tag
   `FilterSignal.scope` from it in `dispatch_filter` / `filter_dispatch` /
   the DSL bridge. **Migration: stamp existing filters with their current
   effective scope so behavior is unchanged.**
2. Partition filters by scope in `backtest.rs`; hoist `Global` evaluation out of
   the per-asset loop with a `(role, ts)` cache; merge into per-asset briefings.
   Add the counting-mock non-redundant-recompute test.
3. `Pair` evaluation + pair-specific cache keys + scope-matched briefing
   selection + tests.
4. Selector capability + per-timestamp narrowing + provenance recording.
5. Observability labels.

## Acceptance (from the plan)

- Multi-filter per-asset isolation tests remain green (existing
  `agent_graph_filter_multi_signal` + `multi_asset_backtest`).
- New tests prove `Global` signals are not recomputed redundantly per asset
  (counting mock).
- Pair/global signals appear in trader briefings only when scope matches the
  current asset context.

## Non-goals

- No live multi-asset (gated by the cline-live L2 plan;
  `docs/superpowers/notes/2026-05-25-live-multi-asset-invariants.md`).
- No new execution/capital modes (Phase 3 spec:
  `2026-05-25-execution-capital-modes.md`).
