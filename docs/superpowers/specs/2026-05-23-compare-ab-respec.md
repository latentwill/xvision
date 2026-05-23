# Compare AB respec — post Charts v2

**Author:** brainstorming session 2026-05-23
**Status:** Spec drafted. **Implementation hold: 48 hours from spec
landing.** Do not open contracts before the hold lifts.
**Supersedes:** `team/intake/2026-05-19-compare-ab-evaluations.md` as the
authoritative scope source. Intake stays for raw-ask provenance.

## 1. Why now

Charts v2 wave (B0–B4) merged 2026-05-23. The intake file gated this
feature on "F33 chart rework" — that gate is lifted. Two things changed
that this spec has to account for:

1. **Visual language exists.** Theme rotation tokens, `Strategy.color`
   (manifest field, no DB migration), `UplotCompareOverlayPane`,
   `ChartFrame`, `Topbar`, kline pixel-anchor, deterministic legend
   palette. All shipped under `frontend/web/src/components/chart/v2/`.
2. **The `/charts/compare` slot is empty.** B2 (#560) merged the
   `CompareChartV2` lab primitive but **never wired
   `frontend/web/src/routes/charts/ChartsCompare.tsx`** — the route
   still renders the B0 "coming soon" `EmptyState`. The B2-plan
   primitives (`ComparisonABDashboard`, `StrategyRosterPills`,
   `StrategyCardGrid`, `StrategyCard`, `MiniSparkline`,
   `LeadCardChrome`, `useChart2Roster`, `MultiStrategyEquityPane`)
   were specified but not built. Wave 1 below picks them up.

Meanwhile `/eval-runs/compare` (582 LOC, `frontend/web/src/routes/eval-compare.tsx`)
is the working surface and where 8 of the 10 intake asks land.

## 2. Topology decision

**Two surfaces, shared primitives, shared selection hook.**

| Surface | Framing | Persistence |
|---|---|---|
| `/eval-runs/compare?ids=…` | Run-centric. "Which of *these specific completed runs* won?" Findings, deltas, per-arm tables. Default landing from `/eval-runs`. | URL only |
| `/charts/compare?ids=…` | Strategy-centric. "Which of *my strategies* is winning?" Roster pills + card grid + hero overlay; latest backtest per strategy. Default landing from `/charts`. | URL only |

Same engine endpoint (`compare_runs`), same `ComparisonReport` payload.
Both surfaces are composed from the same v2 primitives so visual identity
is unified; the difference is information density and framing.

**Programmatic surface (CLI / MCP) returns data by default; chart
rendering is a deferred follow-up (see §6).** `xvn ab-compare …` and
the engine `ComparisonReport` already return JSON. A future `--export
png|svg|pdf` flag lands when the server-side render path is built
(deferred to v3 with #9 share image).

**Both URL surfaces stay id-keyed.** Strategy names appear in every
*display* but the URL contract, CLI argument contract, and engine
endpoint contract remain ULID-keyed. Ids are never unreachable. See §4
"Names without losing ids" for the discipline.

## 3. Wave sequencing

Approved 2026-05-23. Each wave is its own contract / plan; this spec
fixes the order, not the contract bodies.

```
Wave 1  Visual unification + readable labels        (foundation)
Wave 2  Analytical depth                            (engine schema)
Wave 3  Operator velocity                           (workflows)
Wave 4  Surface adaptations                         (mobile + traces)
v3      Live compare, capsule bridge, share image   (deferred)
```

### Wave 1 — Visual unification + readable labels

The foundation wave. Re-skins the working `/eval-runs/compare` with v2
primitives, fills in the empty `/charts/compare` slot with the dropped
B2 composition, and makes labels readable everywhere without breaking
id addressability.

Deliverables:

- **Re-skin `/eval-runs/compare`** with v2 primitives:
  - Replace `CompareChart` with `UplotCompareOverlayPane` (already
    used in the lab; deterministic per-position palette via the
    existing `CURVE_PALETTE` constant).
  - Replace the existing topbar usage with the shared `Topbar` from
    `@/components/shell/Topbar` (no change — already used).
  - Adopt `Strategy.color` from the manifest where strategies opt in;
    fall back to the position-keyed palette.
  - Adopt `ChartFrame` for the equity card chrome.
- **Build `/charts/compare`** — replace B0 `EmptyState` placeholder
  with `ComparisonABDashboard` per the dropped B2 plan
  (`docs/superpowers/plans/2026-05-23-charts-section-b2-comparison-ab.md`),
  pointed at `compare_runs` rather than a strategy-rotation fixture.
  New files (revived from B2 plan):
  - `frontend/web/src/components/chart/v2/surfaces/ComparisonABDashboard.tsx`
  - `frontend/web/src/components/chart/v2/primitives/StrategyRosterPills.tsx`
  - `frontend/web/src/components/chart/v2/primitives/StrategyCardGrid.tsx`
  - `frontend/web/src/components/chart/v2/primitives/StrategyCard.tsx`
  - `frontend/web/src/components/chart/v2/primitives/MiniSparkline.tsx`
  - `frontend/web/src/components/chart/v2/primitives/LeadCardChrome.tsx`
  - `frontend/web/src/components/chart/v2/primitives/MultiStrategyEquityPane.tsx`
- **Extract `useCompareSelection`** at
  `frontend/web/src/components/chart/v2/hooks/useCompareSelection.ts`.
  URL-synced via `useSearchParams` (`?ids=ULID,ULID,…`), min-2 invariant,
  add / remove / toggle / reorder, max 10 (current engine cap). Both
  surfaces consume it.
- **Engine: add `strategy_name: Option<String>` to
  `ComparisonRunSummary`.** Populated server-side from the loaded
  `Strategy` manifest. Lets CLI / MCP consumers get readable labels
  without resolving names client-side. Defaults to `None` so older
  payloads still deserialize.
- **#10 name resolution audit.** See §4.

Acceptance:
- Both `/eval-runs/compare` and `/charts/compare` render real data
  against `compare_runs(ids)` with id-set in URL.
- Adding/removing a roster pill on `/charts/compare` updates URL and
  re-renders without remount flicker (instance-key assertion).
- `LeadCardChrome` only ever wraps `selectedIds[0]`.
- No surface displays a raw ULID where a name is available.
- Every surface that hides an id behind a name exposes the id within
  one interaction (hover/secondary line/copy affordance).
- `npm run typecheck && npm test && npm run build` clean.

### Wave 2 — Analytical depth

Extends `ComparisonReport` and `ComparisonRunSummary`. Both surfaces
render the new fields; CLI prints them in `--json` and table modes.

- **#5 statistical confidence on deltas.** Extend
  `crates/xvision-engine/src/eval/compare.rs::Finding` with
  `{ sample_size: u32, effect_size: Option<f64>, ci_low: Option<f64>,
  ci_high: Option<f64>, p_value: Option<f64>, method: ConfidenceMethod }`.
  `ConfidenceMethod` enum: `Bootstrap | Welch | PermutationTest | None`.
  None ships first if the others slip; the schema is the contract.
  Render a confidence ribbon under each finding (low/mid/high tone
  derived from CI width relative to effect size).
- **#3 per-agent metrics.** Extend `ComparisonRunSummary` with
  `agent_breakdown: Vec<AgentRefSummary>`. Fields per slot:
  `{ role: String, agent_id: String, agent_name: Option<String>,
  latency_p50_ms: Option<u32>, latency_p95_ms: Option<u32>,
  tokens_in: u64, tokens_out: u64, error_count: u32,
  intervention_count: u32 }`. Render as an expandable per-arm panel on
  both surfaces; CLI prints under `xvn ab-compare … --json`.

Acceptance:
- ts-rs exports the new types; existing dashboard `eval-compare` test
  passes after type widening.
- CLI `xvn ab-compare --json` includes the new fields; table mode
  shows the headline confidence + agent counts.
- A finding marked `method: None` renders without the confidence ribbon
  (no UI breakage from partial backfill).

### Wave 3 — Operator velocity

- **#2 promote / demote arms inline.** Each arm card on
  `/eval-runs/compare` and `/charts/compare` gets a `Promote` /
  `Demote` action chip. Wires through existing strategy mutation
  endpoints — no new write-path schema. Confirms inline (toast +
  optimistic update), no popup (workspace rule).
- **#6 compare templates.** Named saved selection sets. New artifact
  shape: `{ name, ids[], scope: 'run' | 'strategy', created_at }`.
  Filesystem-backed under `compare-templates/` (sibling to
  `strategies/`); list + load + save + delete CLI verbs:
  `xvn compare templates {list,save,load,delete}`. Dashboard adds a
  "Save current selection" affordance to both compare surfaces and a
  template picker at the top of each route.

Acceptance:
- Promote action on an arm updates the underlying strategy and the
  card reflects the new champion state without a route change.
- A saved template round-trips through CLI and dashboard. Template
  list survives a process restart.
- No popup / modal / sheet introduced.

### Wave 4 — Surface adaptations

- **#8 mobile compare view.** Add `eval-compare-mobile.tsx`
  alongside the existing `eval-runs-detail-mobile.tsx` pattern. Uses
  `MListCard` for per-arm rows; `MListSheet` for arm-detail expansion
  (the operator-approved mobile-only exception to the no-popup rule).
- **#4 side-by-side trace dock.** Split-pane variant of the existing
  trace dock. New `TraceDockSplit` primitive that hosts two
  `TraceDock` instances side-by-side, keyed to the two leftmost arms.
  Only available when ≥2 arms are inflight (otherwise no value over
  the existing single-arm trace dock).

Acceptance:
- Phone breakpoint renders `eval-compare-mobile` end-to-end; no
  horizontal scroll on the compare overlay.
- Split trace dock shows two arms simultaneously, scroll-independent,
  with synchronized cycle markers.

### v3 — Deferred

Logged here for traceability; do not contract until v3 wave opens.

- **#1 live AB compare for in-flight runs.** Depends on streaming
  `ComparisonRunSummary` shape (engine work) and the multi-eval
  capsule's eventual SSE bridge. Today `compare_runs` filters to
  completed runs; v3 lifts that filter and reshapes the payload to
  surface `status: 'running' | 'completed'` per arm with partial
  fields where in-flight.
- **#7 capsule → compare bridge.** Depends on multi-eval capsule
  exposing a multi-select handle (today only `onSwitchFocus(r)`
  single-row). The bridge is a one-line nav: selected → `/eval-runs/compare?ids=…`.
- **#9 share image — CLI and dashboard.** Server-side raster path
  (`resvg` or `plotters`) so CLI agents can request a chart without a
  browser. Deterministic per id-set for caching. `xvn ab-compare …
  --export png|svg|pdf --out <path>`. Dashboard "Download share
  image" / "Copy share link" with a 1200×630 OG-card variant. No
  operator PII. **Operator explicitly deferred 2026-05-23.**

## 4. Names without losing ids

Ask #10 verbatim said "use strategy names, not ids, in compare chart
labels." Operator amendment 2026-05-23: **id cannot be unreachable.
URL uses id, CLI uses id, but name should be readable everywhere.**

### Display rule

| Surface | Default label | How id stays reachable |
|---|---|---|
| Equity-curve legend | Strategy name | Hover tooltip shows id; legend chip carries `title={id}` |
| Equity-curve tooltip (cursor) | Strategy name | Secondary line shows `ULID0..7…` (first 8 chars) |
| Per-arm card head | Strategy name (Cormorant) | `.caps` micro-line under name shows `ULID0..7…` |
| Findings list | Strategy name in copy | Each finding has a `data-arm-id={id}` attribute; right-click copy |
| Table headers (dashboard) | Strategy name | Sub-header row carries `ULID0..7…`; full id via row hover |
| CLI table headers | Strategy name | Adjacent `(ULID0..7…)` column |
| CLI `--json` output | Both | `name` field added; `id`/`agent_id` unchanged |
| Share image (v3) | Strategy name only | Id rendered in image metadata, not visible |
| URL (`?ids=…`) | Id only | Unchanged — id is the addressing primitive |

### Collision rule

When two arms resolve to the same `Strategy.name`:

- **Disambiguate by version or timestamp**: append `· v2` if the
  strategy has a version field, otherwise `· 2026-05-19 14:32` (cycle
  start timestamp, minute precision).
- **Never fall back to id-as-label.** Id always remains available via
  the affordances above.

### Implementation discipline

`frontend/web/src/lib/run-display.ts` already exports
`displayStrategyName` and `displayScenarioName`. They take the id and
the strategy/scenario list. Wave 1 audits every label site in:

- `frontend/web/src/routes/eval-compare.tsx`
- `frontend/web/src/routes/charts/ChartsCompare.tsx` (post wave-1
  build)
- All v2 chart primitives that emit a label
- `crates/xvision-cli/src/commands/ab_compare.rs`

Anywhere a raw id is rendered without going through these helpers is a
wave-1 regression. Wave 1 adds `scripts/check-compare-labels.sh` —
greps the four files / surface families above for ULID-shaped string
literals being passed to JSX or println without `displayStrategyName`
on the path. Wired into pre-commit and CI.

## 5. Engine schema deltas (waves 2+)

Preview here so wave-1 work doesn't paint itself into a corner.

```rust
// Wave 2 — Finding extension
pub struct Finding {
    // existing fields ...
    pub sample_size: u32,
    pub effect_size: Option<f64>,
    pub ci_low: Option<f64>,
    pub ci_high: Option<f64>,
    pub p_value: Option<f64>,
    pub method: ConfidenceMethod,
}

pub enum ConfidenceMethod {
    Bootstrap,
    Welch,
    PermutationTest,
    None,
}

// Wave 2 — Per-agent breakdown on each ComparisonRunSummary
pub struct AgentRefSummary {
    pub role: String,
    pub agent_id: String,
    pub agent_name: Option<String>,
    pub latency_p50_ms: Option<u32>,
    pub latency_p95_ms: Option<u32>,
    pub tokens_in: u64,
    pub tokens_out: u64,
    pub error_count: u32,
    pub intervention_count: u32,
}

pub struct ComparisonRunSummary {
    // existing fields ...
    pub strategy_name: Option<String>,   // wave 1 — name field on the wire
    pub agent_breakdown: Vec<AgentRefSummary>,  // wave 2 — defaults to empty
}
```

Wave 1 only adds `strategy_name`. Waves 2 fields land with their own
contracts; defaulting to `Vec::new()` and `Option::None` lets older
CLI versions read newer payloads without breaking.

## 6. Out of scope

- Replacing `compare_runs`. Evolution, not rewrite. ts-rs payload
  shape stays additive.
- Cross-strategy NFT-promotion gating (separate identity track).
- Charting backend rewrites — reuse `ChartEquityPoint` /
  `ComparisonEquityCurve`.
- Server-side image rendering — explicitly v3.
- Live in-flight compare semantics — explicitly v3.
- Capsule bridge — depends on capsule multi-select, not yet shipped.

## 7. Sources

- Intake: `team/intake/2026-05-19-compare-ab-evaluations.md` (10 asks).
- Charts v2 foundation spec: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md`.
- Dropped B2 plan (revived in Wave 1): `docs/superpowers/plans/2026-05-23-charts-section-b2-comparison-ab.md`.
- Workspace rule (no popup): `/Users/edkennedy/Code/xvision/CLAUDE.md` "Frontend UI rule: no popups".
- Terminology lock (`cycle_id`, `Strategy`, `Agent`): `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`.
- Existing surface: `frontend/web/src/routes/eval-compare.tsx` (582 LOC).
- Engine endpoint: `crates/xvision-engine/src/eval/compare.rs`.
- Existing CLI: `crates/xvision-cli/src/commands/ab_compare.rs` (271 LOC).
- Name-resolution helpers: `frontend/web/src/lib/run-display.ts`.

## 8. Implementation hold

**48 hours from spec landing.** Conductor: do not author wave-1
contracts before the hold lifts. Hold rationale: respec was bundled
into the Charts v2 post-merge sweep and the operator wants room to
adjust scope before the team picks it up.
