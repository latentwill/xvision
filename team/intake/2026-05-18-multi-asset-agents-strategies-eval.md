# Intake — 2026-05-18 — Multi-asset across Agents / Strategies / Eval

Follow-up from operator (2026-05-18). Forward-looking feature scope, not
a current-wave contract. Captured here so the conductor can decompose
once the architectural decisions land.

The single-asset assumption is baked across three layers — Agents,
Strategies, and Eval — and they have to evolve together. Today a
scenario is one symbol, one venue, one granularity. Strategies and
agents inherit that assumption (an intern/trader looks at one symbol;
risk gates on one position). Eval rolls up per-run scalars over one
symbol's bars.

## Already on the board (don't respawn)

- `team/decisions.md` **D2 — Multi-asset scenarios** is open and still
  needs operator answers on (a) parallel-runs vs. multi-symbol feed,
  (b) data-ingestion gating, (c) Scenario v1 extend vs. v2 type.
- The eval-review-agent + scenario-warmup-bars tracks already in
  flight assume single-asset; their contracts do not need to be
  re-scoped here. The multi-asset wave is **next**, not now.
- `team/intake/2026-05-17-qa-operator-fix-sprint.md` carries a stray
  "Follow up — multi asset strategies on scenarios?" note; this intake
  supersedes that note.

## Raw items (forward-looking — not contracts yet)

1. **Multi-asset scenarios.** A Scenario can carry N asset legs
   (basket, pair, cross-venue), each with its own bar feed, with one
   shared time window + granularity. Open: parallel-runs-then-join
   vs. one merged bar stream the strategy sees as multi-symbol. See
   D2.

2. **Multi-asset agents.** Trader / intern / risk slots need to be
   able to reason over more than one symbol per decision cycle. Open:
   does the slot prompt template see all legs at once (one bar
   bundle per cycle), or do we run the slot N times per cycle (one
   per leg) and aggregate? The terminology rename keeps `AgentRef`
   as a per-slot reference; multi-asset is a new shape under it, not
   a rename.

3. **Multi-asset strategies.** A `Strategy` needs to carry which legs
   each `AgentRef` is wired to, plus cross-leg routing (does the
   trader see all legs and emit per-leg decisions, or one
   portfolio-level decision the executor splits?). Open: schema
   extension vs. v2 `Strategy` type. Coordinates with the
   strategies-refactor-agent-composition work already landed.

4. **Multi-asset eval.** Per-run metrics need to roll up across legs
   (basket equity, per-leg attribution, correlation regimes). The
   eval-review-agent payload collector and `xvn eval` JSON output
   need to grow per-leg fields. Backward-compat: single-asset evals
   stay shaped as today (one leg = the current shape).

## Open dependencies before decomposition

- D2 operator answers (parallel-runs vs. merged feed; data-ingestion
  prerequisite; v1 extend vs. v2 type). Without these the work
  cannot be split into tracks.
- Bar-cache substrate review: today's `bar_cache_policy.cache_key`
  is per-symbol; multi-asset needs either a tuple key or a
  per-leg bag indexed by symbol.
- Risk-gate shape: today's `RiskDecision` assumes one position. A
  basket trader will emit multiple `TraderDecision`s per cycle (or
  one structured decision) — risk needs to gate the bundle.

## Likely track decomposition (sketch only — do not contract yet)

| Layer | Track (provisional) | Lane |
|---|---|---|
| Data | `multi-asset-scenario-schema` | foundation |
| Data | `multi-asset-bar-cache` | foundation |
| Engine | `multi-asset-agent-slot-shape` | foundation |
| Engine | `multi-asset-strategy-routing` | integration |
| Engine | `multi-asset-risk-gate-bundle` | integration |
| Eval | `multi-asset-eval-rollup` | integration |
| UI | `multi-asset-scenario-authoring` | leaf |
| UI | `multi-asset-run-detail` | leaf |

That's 8 tracks if it lands as one wave; more likely two waves
(data + engine, then eval + UI) once D2 resolves.

## Out of this intake

- Cross-venue execution (one strategy submits to two brokers
  simultaneously) — different problem; punt to its own intake.
- Portfolio-construction agents (optimizer-style allocation) —
  different problem; punt.
- Marketplace listings for multi-asset strategies — pricing /
  NFT side; punt.

## Next steps

1. Operator answers D2 questions in `team/decisions.md`.
2. Conductor opens a tiny ADR (`decisions/`) capturing the
   shape choice (parallel-runs vs. merged feed; v1 extend vs.
   v2 type).
3. Conductor decomposes this intake into contracts; first wave
   is data-layer foundation tracks.
