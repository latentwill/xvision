# Dashboard — surface inventory

> **🗄️ ARCHIVED — 2026-05-10.** This document is preserved for historical
> reference only. Treat it as design intent, not a live spec. New work on
> the dashboard surface should refer to the documents below.
>
> **Subsumed by:**
> - **UI design lock:** `docs/design/ui-elements.md` (v0.2 — five archetypes + sitemap, current source of truth for the dashboard surface).
> - **Eval engine panels:** `docs/superpowers/specs/2026-05-08-eval-engine-design.md` + `docs/superpowers/plans/2026-05-08-eval-engine-plan.md` (the H/T/A/E/R/O panel families from §3 land here).
> - **Dashboard implementation:** `docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md` (Wizard / Inspector / Live cockpit / Templates grid — the v1 routes that replace this v0 panel inventory).
> - **Marketplace surface:** `docs/superpowers/specs/2026-05-09-marketplace-plugin-design.md` (deferred to Plan 5; the leaderboard / per-trade ledger panels resurface there).
>
> **What's still useful here:** the priority-ranked thinking in §1 ("Why this
> dashboard exists") and the panel-by-panel reasoning in §3–§7 are valuable as
> design history when picking up post-v1 work — they explain *why* certain
> panels were chosen.
>
> **What's obsolete:** Panels referencing `vectors_on`/`vectors_off`,
> `active_vectors`, `vector_config_hash`, and the per-axis (Conviction/Patience/
> Risk/Trend) cuts are obsolete in xvision per ADR 0011 — the CV substrate moved
> to xvision-play. The strategy-level Δ-Sharpe / leaderboard / per-trade ledger
> panels (V1, V2, V3 retargeted at `arm_name` not `vector_config_hash`) carry
> forward via the eval engine and marketplace plans linked above.

## Current Chart Surfaces

As of the 2026-05-14 Alpaca and TradingView implementation slices, the current
dashboard chart surfaces are:

- Run detail: candles, server-computed indicators, trade/veto/hold markers,
  position bands, equity, drawdown, and volume.
- Compare: up to 10 run equity curves, with a shared-scenario price backdrop
  when all runs use the same scenario.
- Scenario detail: price preview plus cache status.
- Strategy detail: completed-run equity curves grouped by scenario.
- Live cockpit: run chart snapshot plus SSE updates.
- Scenario wizard: transient preview from form values.

> Working doc · 2026-05-04 · Draft v0.1
>
> Scope: enumerate every panel xvision's dashboard could reasonably show,
> evaluate which ones the thesis actually needs, map each to a data source,
> and call v1 / v2 scope explicitly so demo-week doesn't accidentally inherit
> the full surface.

---

## 1. Why this dashboard exists

Xvision's reason to exist is to defend one number: **Δ-Sharpe of vectors-ON
minus vectors-OFF on a fixed setup population, with a 95% CI that excludes
zero, validated across at least one bull and one bear regime.** Every other
panel has to justify itself either as (a) a credibility prop for that
headline number, (b) a diagnostic that lets us debug when the headline is
disappointing, or (c) an operational read on the running system.

That priority ranking matters because the four ideas in the brief are a
mixed bag:

- **(1) Agent prompts/settings/model harness** — credibility prop. Reviewers
  need to see what the experiment is *of*. High value, low cost.
- **(2) Historical trades + chart with buy/sell markers + list** — both
  credibility and diagnostic. Users will not trust a Sharpe number without
  seeing the underlying decisions.
- **(3) A/B evaluation across multiple agents/strategies** — *this is the
  product*. It is the headline number. Top priority.
- **(4) Vector usage, strength, win-rate vs baselines** — unique-to-xvision
  diagnostic. Without it, vectors-on vs vectors-off becomes a black box even
  to us.

The panels below add what the four bullets are missing, then re-rank.

---

## 2. Panel inventory

Grouped by audience. For each: *purpose · data source · v1/v2 · open
questions*.

### 2.1 Headline experiment (the demo opens here)

**H1 — Δ-Sharpe headline tile.** Single big number with a 95% CI bracket.
Subtitle shows N paired trades, regime span, vector manifest hash. Color is
green if CI excludes zero in the right direction, amber if it straddles,
red if it excludes zero in the wrong direction.

- *Source:* eval run `ComparisonReport` JSON output → `reports/headline_*/*.json`,
  Δ-Sharpe row.
- *v1:* yes, this is the demo's first frame.
- *Open:* what's the rule for picking which manifest hash to render when
  multiple production runs exist? Probably "latest run that passed F1
  directional gate."

**H2 — Anti-overfit gate verdict.** Per `architecture.md` §9.2 and the
`decisions/strategy-choices.md` re-frame, the gate is reportable not
blocking in v1. Surface as a named verdict — `PassesBothRegimes |
SingleRegimeEvidence | Fails` — with the regimes the Δ-Sharpe was positive
on listed underneath.

- *Source:* eval crate's regime-stratified Δ-Sharpe rollup.
- *v1:* yes. Without this badge the headline tile lies — a positive
  Δ-Sharpe on a single-regime span is the NexusTrade hill-climbing trap and
  the doc explicitly calls it out.
- *Open:* render NexusTrade's $676 cautionary blurb on hover, or just link
  to the strategy-choices ADR? Probably link.

**H3 — Paired-trade scatter.** One dot per setup; x = vectors-OFF return,
y = vectors-ON return. Diagonal reference line. Color by regime. The
visual reviewers will spend the most time on after the headline tile.

- *Source:* `executions.realized_pnl` joined to `setups.created_at` and
  the regime tag from briefings.
- *v1:* yes if `xvision-eval` already computes paired returns; cheap to add
  if not.

**H4 — Decision divergence rate.** % of paired setups where vectors-on and
vectors-off produced *different* `action` values, broken down by regime.
This is the "vectors are doing something" signal. If divergence is < 5%
the Δ-Sharpe number is mechanically untrustworthy.

- *Source:* paired `decisions.decision_json.action` column compared across
  arms.
- *v1:* yes — already in the metrics block in `architecture.md` §9.2.

### 2.2 Agent configuration (user idea #1, expanded)

**A1 — Agent identity card per arm.** One card per running arm: arm name
(`vectors_off`, `vectors_on`, `vectors_random`, `vectors_orthogonal`), the
ERC-8004 token ID + Mantle contract addr from
`identity/<arm>.agent.json`, the `code_commit` (git rev), the active vector
manifest hashes, the model name + quantization, the harness backend
(candle / mlx / mistralrs).

- *Source:* `identity/*.agent.json` + `config/default.toml` + runtime
  `harness_info`.
- *v1:* yes. This is what reviewers click through to before they trust H1.
- *Open:* Mantle Reputation Registry score lives here too — see R1.

**A2 — Stage 1 (Intern) settings card.** Provider (`anthropic | openai |
local`), base URL, model, system prompt hash + diff vs. previous, cache
hit rate, average latency + token spend. A "show prompt" disclosure
expands the full system + user template.

- *Source:* `intern_provider`, `intern_model` from briefing rows + traces
  `attrs_json` (`gen_ai.usage.*`).
- *v1:* yes — this is what "any other agent attached (??)" maps to most
  cleanly. There is exactly one Intern.

**A3 — Stage 2 (Trader) settings card.** Model path, GGUF quant, tokenizer
path, no-thinking mode flag, layers hooked, alpha schedule, gating mode
(`entropy_v1 | cast_post_v1`), prompt hash, vector manifest hashes per
slot. This is the experiment's surface area; if anything moves here the
Δ-Sharpe stops being comparable.

- *Source:* runtime config snapshot persisted at `run_id` start +
  manifest sidecars under `data/vectors/`.
- *v1:* yes.
- *Open:* this is also where the harness type (candle vs mlx) is shown —
  ADR 0007 routes are not interchangeable.

**A4 — Risk layer settings card.** All seven rules from architecture §5
listed with their current numeric values, plus a "rules edited at"
timestamp. A change to the risk layer is silently a change to the
experiment, and reviewers should see it.

- *Source:* `config/risk.toml`.
- *v1:* yes.

**A5 — Boundary probe corpus version.** v1 ships a minimal corpus per
implementation-plan §8.5. Show: probe set name, n probes, last
re-evaluation timestamp, decision-flip count vs. baseline, capability-floor
delta. Wired up only after F13 / M14 lands.

- *Source:* eval harness JSON (xvision-harness retired 2026-06).
- *v1:* stub only — corpus is curated post-headline.

### 2.3 Trade history (user idea #2, expanded)

**T1 — Equity curve, paired.** Two lines, vectors-on and vectors-off,
plotted on the same setup population. Setup boundaries shown as faint
verticals. Drawdown shading underneath each curve. Toggle to show the
random and orthogonal control arms when M2 has produced those vectors.

- *Source:* `executions.realized_pnl` cumsum per `arm_name`.
- *v1:* yes. The four-arm overlay is gated on F2.

**T2 — Price chart with decision overlay.** Per asset, OHLCV with markers
at every `cycle_id`. Marker shape encodes action (`buy = ▲`, `sell = ▼`,
`flat = ○`, `close = ✕`); marker color encodes which arm; tooltip shows
the briefing summary, the trader summary, the active vector alphas, and
the realized PnL of the closed trade.

- *Source:* `data/bars/btc_2022_2024.json` joined to `setups.created_at`
  and `decisions.decision_json`.
- *v1:* yes — this is the demo's "show me the trades" frame.
- *Open:* paper-trading vs backtest — same panel, different data source;
  v1 backtest only.

**T3 — Trade ledger.** Sortable table: timestamp, asset, arm, briefing
summary, action, size_bps, stop, take-profit, risk verdict (passed /
modified / vetoed), realized PnL, holding period, regime, manifest hash,
ERC-8004 validation tx (when on-chain).

- *Source:* full join of `setups + briefings + decisions + risk_outcomes
  + executions`.
- *v1:* yes — this is the panel reviewers will export for offline
  inspection.

**T4 — Briefing → decision divergence row.** For each setup show the
Intern's bull/bear/flat case headers next to the four arms' decisions
side-by-side. Reveals when the Intern emits "this is a bear setup" and
the vectors-on Trader still goes long — that's where vectors are visibly
overriding the textual evidence and is the most viscerally compelling
demo moment we have.

- *Source:* `briefings.briefing_json` + per-arm `decisions`.
- *v1:* yes — this is the panel that *sells* the thesis.
- *Open:* layout. A horizontally-scrolling row per setup probably reads
  better than a stacked table.

### 2.4 A/B comparison (user idea #3, expanded)

**E1 — Arm comparison table.** One row per arm, columns:
Δ-Sharpe-vs-baseline, raw Sharpe, max DD, profit factor, win rate,
decision divergence vs OFF, mean latency, mean cost / decision, cache hit
rate. Sort by any column.

- *Source:* `xvision-eval` rollup JSON.
- *v1:* yes.

**E2 — Pairwise bootstrap confidence intervals.** A matrix showing the
95% CI on Δ-Sharpe between every pair of arms (OFF vs ON, OFF vs RANDOM,
ON vs ORTHOGONAL, etc.). The full set of "is this perturbation actually
doing something" tests in one frame. Cells colored by whether the CI
excludes zero.

- *Source:* paired bootstrap (10k resamples) per `architecture.md` §9.2.
- *v1:* yes — already a documented requirement.

**E3 — Regime-stratified Δ-Sharpe.** Faceted small-multiples: one panel
per regime in `{trending, choppy, high_vol, low_vol}`, each showing the
arm comparison. This is what feeds H2's verdict and is the primary
diagnostic when H2 shows `SingleRegimeEvidence`.

- *Source:* eval crate stratification, regime tag from briefings.
- *v1:* yes.

**E4 — Baseline horse-race.** Δ-Sharpe of the LLM agent (per arm) vs each
classical baseline (RSI, MA-cross, MACD, Donchian, Bollinger, Fibs,
buy-and-hold, random) and (when M15 lands) the onchain baselines. This is
the "external bar" panel — vectors-on beating vectors-off is necessary
but not sufficient; both have to clear the technical and onchain
baselines.

- *Source:* same eval crate output, baselines arm.
- *v1:* yes for technical baselines (already in
  `xvision-eval/src/baselines/`); onchain panels are stub-only until F14.

### 2.5 Vector mechanics (user idea #4, expanded)

This is the section where xvision's dashboard becomes *actually different*
from a normal trading dashboard. Most of these panels exist nowhere else.

**V1 — Active vector alpha histogram.** For each axis (Conviction in v1;
Patience / Risk / Trend extracted-only), histogram of `active_vectors`
magnitudes across all decisions in the run. Shows whether confidence
gating is firing — a peaked histogram at 0 means the entropy gate killed
the vector for most decisions; a peaked histogram at the unbounded
magnitude means the gate is never firing and the alpha schedule is
constant.

- *Source:* `decisions.decision_json.active_vectors`.
- *v1:* yes — this is the diagnostic for "is the gate working."

**V2 — Per-axis win rate.** Bucket decisions by the active alpha on each
axis (e.g. Conviction ∈ {[-1, -0.5), [-0.5, 0), [0, 0.5), [0.5, 1.0]})
and compute realized win rate + mean return per bucket. The shape of the
curve tells us whether the vector is monotonically helpful, helpful only
above a threshold (Mitra non-monotonicity), or actively harmful.

- *Source:* `decisions.active_vectors` joined to `executions.realized_pnl`.
- *v1:* yes — this is the panel that lets us recommend an alpha schedule
  for v2.
- *Open:* small-N caveat — with 100 trades the buckets get thin fast.
  Probably show CI bands.

**V3 — Vector vs baseline lift.** For each baseline strategy in §2.4 / E4,
the Δ-return when vectors-ON disagrees with the baseline vs when it
agrees. Reveals whether vectors add value *at decision time* or only
through entry filtering.

- *Source:* baseline outputs from `xvision-eval/src/baselines/`.
- *v1:* yes — cheap composition over E4.

**V4 — Confidence gate trace.** Time series of `xvision.gating.entropy`
and `xvision.gating.applied_magnitude` per decision over the run.
Annotated with regime transitions. Shows whether the gate's entropy
threshold is sane in production data — a flat line at full magnitude
means the gate isn't gating.

- *Source:* `traces` table, attrs_json keys.
- *v1:* yes. The trace fields are mandated by `architecture.md` §7.6.

**V5 — Layer introspection viewer.** When a run was launched with
`introspect.enabled = true`, render the per-decision JSON snapshot:
per-layer residual norms (pre/post hook), activation diff, vector–residual
cosine, logit lens decision-token probabilities at each hooked layer,
decision-token entropy at the gate point. This is the panel the spike
validation gate (Phase 0.3) and any "is this vector real" investigation
will live in.

- *Source:* `xvision-introspect` JSON + `notebooks/inspect_vector.py`.
- *v1:* opt-in tab only — off in production hot path. The PoC is
  re-rendering the existing notebook plots inline.

**V6 — Magnitude sweep.** Render the offline magnitude-sweep result for
each axis: alpha on x, mean directional-match-rate on y. The
non-monotonicity Mitra cites should be visible as a humped curve. The
v1 spike validation gate explicitly requires this be plottable.

- *Source:* offline introspection sweep JSON; one-off, regenerated when
  vectors are re-extracted.
- *v1:* yes — required by Phase 0.3 acceptance criteria.

### 2.6 Risk layer (new — not in the four bullets)

**R0 — Risk verdict pie.** Approved / modified / vetoed % across the run,
sliced by arm. A vectors-on agent that gets modified or vetoed twice as
often as vectors-off is informative — it means vectors are pushing the
agent into regions a human risk manager would also reject, and the
unmodified Δ-Sharpe overstates the true vector contribution.

- *Source:* `risk_outcomes.risk_decision_json`.
- *v1:* yes — explicitly called out in `architecture.md` §5 ("vetoes are
  valuable signal").

**R1 — Veto reason histogram.** Why the risk layer rejected. One bar per
rule (`max_position_size`, `daily_loss_circuit`, `correlation_cap`, …).
Shaped by arm.

- *Source:* `risk_outcomes.veto_reason`.
- *v1:* yes.

**R2 — Risk-adjusted Δ-Sharpe.** Reruns H1 but only over setups that
*passed risk unmodified for both arms*. Strips out the vetoes-as-policy
confound. If H1 says vectors-on wins by Δ=0.4 and R2 says it wins by
Δ=0.1 over un-vetoed trades, the gap is "vectors-on takes worse trades
but the risk layer cleans them up" — a real and reportable result, just
a different one than the headline implies.

- *Source:* same as H1, filtered.
- *v1:* yes — cheap, high-signal.

### 2.7 On-chain (ERC-8004 — Mantle hackathon hook)

**O1 — Identity registry view.** Per-arm: token ID, contract addr (linked
to the Mantle block explorer), mint tx hash, `vector_manifest_cid` IPFS
link, `code_commit` (linked to GitHub).

- *Source:* `identity/<arm>.agent.json` + Mantle RPC.
- *v1:* yes — the hackathon submission's "show me your on-chain proof"
  panel.

**O2 — Reputation registry feed.** Latest reputation entries for each arm
NFT: Δ-Sharpe, regime context, manifest hash, run_id, timestamp, on-chain
tx. Sortable by timestamp.

- *Source:* Reputation Registry contract events on Mantle, queried via
  `alloy`.
- *v1:* yes — this is novel and it's what makes Mantle's ERC-8004
  deployment load-bearing for the submission.
- *Open:* indexer choice — `mantle-data-indexer` skill names a few; pick
  one and standardize.

**O3 — Validation registry trade feed.** Stream of validation proofs as
they post: cycle_id, action, active vector alphas, vector manifest hash,
vectors_enabled flag, trade result hash, run_id, on-chain tx hash.
Reviewer can click any row to verify on Mantle Etherscan.

- *Source:* Validation Registry contract events on Mantle. Posted by
  `xvision-execution` after every closed Orderly position
  (`architecture.md` §6.1).
- *v1:* yes — this is the prospective-stance audit trail and it's the
  novel piece of the demo.
- *Open:* paper-trading rows have no on-chain row; show an "off-chain"
  badge instead of empty.

### 2.8 Operational health

**Op1 — Run status header.** Run ID, started-at, current stage, elapsed,
ETA. Backend (mlx-spike / candle / mistralrs). GPU/CPU utilization if
the harness exports it.

- *Source:* runtime in-memory.
- *v1:* yes if there's a long-running run; nice-to-have.

**Op2 — Cost & latency by stage.** Stacked bar: Stage 1 token cost +
latency, Stage 2 wall time, Stage 3 round-trip latency, on-chain post
gas. Aggregated per setup, plotted over time.

- *Source:* `traces` `gen_ai.usage.*` + custom `xvision.*.duration_ms`.
- *v1:* yes — the Anthropic-Haiku-vs-Opus tradeoff in §M8 is a real
  budget question and this panel makes it visible.

**Op3 — Error & parse-failure rate.** % of setups where Stage 1 returned
a schema-invalid briefing, Stage 2 returned a parse-error decision, the
risk layer crashed, or the executor errored. Sliced by arm and stage.

- *Source:* `decisions.parse_success` + traces `error` attribute.
- *v1:* yes — without this Op2's latency numbers are biased by silent
  retries.

**Op4 — Cache hit rate.** Briefings cache hit % (per Tier 1 fix #1) over
the run. A drop here means the cache key is invalidating spuriously and
inflating Stage 1 cost.

- *Source:* trace event count: `briefing_cache_hit` vs
  `briefing_cache_miss`.
- *v1:* yes — already a known metric per the F3 implementation plan.

### 2.9 Cross-run / longitudinal

**X1 — Run history table.** Every `run_id` ever recorded, sorted
descending: started-at, arms compared, headline Δ-Sharpe, anti-overfit
verdict, manifest hash, code commit, notes. Click into any row to load
that run as the "current" frame for the dashboard.

- *Source:* `traces` joined to `executions` aggregates per `run_id`.
- *v1:* yes — without this the dashboard re-renders only the latest run
  and we lose history.

**X2 — Vector manifest provenance graph.** For each manifest hash, the
edges: which spec it was extracted from, which model it targets, which
contrast pair set, what runs have used it, what reputation has accrued
to it on Mantle. Read this as a build-graph view of "where did this
disposition come from."

- *Source:* manifest sidecars + `decisions.vector_config_hash` joined to
  `runs`.
- *v1:* stretch — useful for the Karpathy v2 self-improvement story but
  not the v1 demo. Defer.

---

## 3. Re-ranked priority for v1 demo

Cut to the panels that actually fight for the demo's first 10 minutes of
attention. Order = render order on the dashboard's landing tab.

1. **H1** — Δ-Sharpe headline tile.
2. **H2** — Anti-overfit verdict.
3. **A1 / A2 / A3** — agent identity + Intern + Trader cards (one row,
   three cards).
4. **H3** — paired-trade scatter.
5. **H4** — decision divergence rate.
6. **T4** — briefing → decision divergence row (the "vectors override the
   evidence" demo moment).
7. **V1 + V2** — vector alpha histogram + per-axis win rate bucketed by
   alpha.
8. **E1 / E2** — arm comparison table + pairwise CI matrix.
9. **R0** — risk verdict pie.
10. **O1 / O3** — identity card on-chain + validation registry feed.

Everything else moves to secondary tabs:

- **Trades tab:** T1, T2, T3.
- **Vectors tab:** V3–V6.
- **Risk tab:** R1, R2.
- **A/B tab:** E3, E4.
- **On-chain tab:** O2.
- **Ops tab:** Op1–Op4, X1.

Anything not ranked is v2 or stretch — A4 (risk settings card), A5
(probe corpus), V5 (layer introspection viewer), X2 (manifest
provenance graph).

---

## 4. Data sources — at-a-glance

| Panel(s) | Source |
|---|---|
| H1, H2, H3, H4, E1–E4 | Eval run `ComparisonReport` JSON in `reports/headline_*/` |
| A1, O1, O2, O3 | `identity/<arm>.agent.json`, Mantle RPC via `alloy`, Mantle indexer |
| A2, A3, A4 | `config/*.toml`, runtime config snapshot |
| T1, T2, T3 | `setups + briefings + decisions + risk_outcomes + executions` join in SQLite |
| T4 | `briefings.briefing_json` + per-arm `decisions.decision_json` |
| V1, V2, V3 | `decisions.decision_json.active_vectors` joined to `executions.realized_pnl` |
| V4 | `traces` rows with `xvision.gating.*` attributes |
| V5, V6 | `xvision-introspect` JSON + `notebooks/inspect_vector.py` |
| R0, R1, R2 | `risk_outcomes.risk_decision_json` |
| Op1–Op4 | `traces` + runtime metrics export |
| X1, X2 | `traces` joined to `executions` per `run_id`; manifest sidecars |

All of the SQLite-backed panels can be served straight from the existing
`xvision_core::store::Store` — the schema in `migrations/0001_init.sql`
already has everything except the optional gating-trace columns.

---

## 5. Implementation shape (proposed)

The dashboard itself is **out of v1 scope per `architecture.md` §11**
("Dashboard with historical data UI" → deferred). For the hackathon the
deliverables collapse to:

1. **Markdown report** generated by `xvn report` (Phase 10.2, landed) that
   renders a static version of the v1-priority panels above. This is what
   the headline run actually produces and what reviewers download.
2. **Live demo HTML page** (stretch) that wraps the JSON output of
   `xvn eval compare` and renders it inline using Chart.js + Grid.js — the
   same artifact-shape this Cowork environment supports. Reload pulls a
   fresh JSON.
3. **Deferred:** a long-running web app reading SQLite + streaming
   `traces` + querying Mantle. v2 work, only justifies itself once
   forward paper trading or post-hackathon ops are running.

**Trigger to lift to a real app:** forward paper trading lands (Phase
11.1) and we want the validation registry feed (O3) updating in
real-time during the demo. Until then, the Markdown report is sufficient.

---

## 6. Open questions

- **Layout for T4 (briefing-to-decision divergence).** The pitch is
  "scroll left-to-right through setups, see the cases, see how each arm
  decides." This is the demo's most viscerally compelling frame and
  needs design attention.
- **What lives on the landing page vs in tabs.** The §3 ranking is a
  guess — once the headline run produces a real number, A/B test layouts
  with two reviewers and pick.
- **Do we surface the spike validation gate result anywhere?** Phase 0.3
  passed empirically (per ADRs); the harness re-runs it under F1. The
  gate itself probably belongs in V6 / the Vectors tab footer rather
  than its own panel.
- **Streaming vs snapshot.** Backtest runs are batch; forward paper is
  streaming. The dashboard should support both shapes — minimum viable
  is "snapshot for v1, streaming for v2."
- **How much do we expose about confidence-gate internals to non-technical
  reviewers?** V4 is honest; it's also potentially confusing. Maybe a
  "show internals" toggle that hides V4–V6 by default.

---

*Document version: 0.1 · 2026-05-04. Lives at
`/Users/edkennedy/Code/xvision/docs/dashboard.md`. Cross-references:
`architecture.md` §§5, 6.1, 7.5.1, 7.6, 9; `MANUAL.md` M2/M4/M7;
`FOLLOWUPS.md` F1/F2/F3/F4/F13/F14; `crates/xvision-core/migrations/0001_init.sql`.*
