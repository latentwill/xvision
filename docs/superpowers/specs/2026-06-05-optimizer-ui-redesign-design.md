# Optimizer UI Redesign — Design Spec

**Date:** 2026-06-05
**Status:** Approved design → implementation planning
**Scope:** Operator-facing Optimizer surface in the dashboard SPA (`features/autooptimizer/`),
plus the backend extensions required to make it full-fidelity.
**Design source:** `docs/design/XVN_optimizer/ar-home.jsx`, `ar-cycle.jsx`, `ar-variant.jsx`,
`ar-shared.jsx`, `ar-canvas.jsx` (the settings panel, `ar-settings.jsx`, is explicitly out of scope).

---

## 1. Problem

The current Optimizer surface (`/autooptimizer`) is **five flat tabs** — `Live`, `Genealogy`,
`Diff`, `Ladder`, `Provenance` — with real redundancy:

- `Genealogy` is a flat experiment list whose only action is to navigate to `Diff`; the two are
  one feature split across two tabs.
- `Ladder` and `Provenance` render the same experiment-writer scoreboard twice (Provenance just
  adds lineage grouping).
- `Live` is a do-everything dashboard (controls + event log + kept + lineages + recent cycles).

The tab model gives no sense of *where you are* or *how things relate*. The approved mockups
replace it with a **three-screen drill-in hierarchy** (Home → Cycle → Experiment) carrying a
much richer gold-on-dark visual system.

Terminology is already largely compliant with the
`2026-05-27-autooptimizer-terminology-lock.md` (Experiment / Kept / Suspect / Rejected /
Experiment writer). This redesign keeps that and extends it to the new surfaces.

## 2. Goals

- **A. Terminology** — every operator-facing label conforms to the terminology lock; the new
  surfaces (gate buckets, attesters, flight recorder, evening summary) get lock-conformant names.
- **B. Layout** — match the mockups' three-screen IA and panel composition; delete the tab bar.
- **C. Navigability** — a breadcrumb hierarchy where each screen has one clear purpose and the
  relationships (cycle → its experiments → an experiment's diff/regimes/trace) are spatial, not
  hidden behind sibling tabs.
- **D. Visual quality** — adopt the mockups' design language as reusable themed primitives.

### Non-goals (this pass)

- Gen-art identity pipeline — replaced by a deterministic **hash-sigil** identicon.
- On-chain / Marketplace publish flow — cross-links are stubbed; "publish to chain" is an
  empty-state, not wired.
- The optimizer **settings** panel (`ar-settings.jsx`).

## 3. Decisions (locked during brainstorming)

| Decision | Choice |
|---|---|
| Fidelity | **Full fidelity (FE + BE)** — build backend for the data-hungry panels. |
| Writer ladder home | **Panel on Optimizer Home** (merge Ladder + Provenance into one). |
| Speculative surfaces in scope | **Attesters + sign-off receipts**, **Flight recorder**. |
| Speculative surfaces out | Gen-art, on-chain/Marketplace publish. |
| Sequencing | **Approach A — IA-first**, then light up panels behind the shipped shell. |
| Regime matrix | **Configurable regime set** (default 3–5), gate → `PassesBothRegimes`, Suspect = `SingleRegimeEvidence`, operator can shrink the set to bound cost. |

## 4. Information architecture

Three screens, breadcrumb navigation, no tab bar.

```
Optimizer Home  /optimizer
  └─ Cycle       /optimizer/cycle/:cycle_id
       └─ Experiment  /optimizer/experiment/:bundle_hash
```

- The operator route becomes **`/optimizer`**; `/autooptimizer` redirects to it.
- Dev-surface names are unchanged: HTTP `/api/autooptimizer/*`, the `features/autooptimizer/`
  folder, Rust `autooptimizer/` module, `autooptimizer_*` tables. (Per the lock, never collapse
  the codename to bare `optimizer` — DSPy owns that token.)

### Tab → screen/panel fold-in map

| Old tab | New home |
|---|---|
| `Live` | **Optimizer Home** (live-cycle hero + event feed + active lineages + recent cycles). |
| `Genealogy` | The **Cycle** screen's experiments table is the per-cycle genealogy; a cross-cycle **lineage tree** is a dedicated inline view linked from the cycle. |
| `Diff` | Inlined as the **"What this experiment changed"** panel on the **Experiment** screen. |
| `Ladder` | **Experiment-writers panel** on Optimizer Home. |
| `Provenance` | Merged into the same Experiment-writers panel (grouping becomes an expand within it). |

### 4.1 Optimizer Home — `/optimizer`

Regions (all full-width, single column; ✅ = wired to existing data, ⏳ = lights up in a later phase):

- **Header** — `OPTIMIZER` eyebrow, run status pill, headline ("Tonight's run in progress · N
  cycles · M lineages"), week rollups (experiments / kept / tokens / $). Actions: *Configure
  loop*, *What is this?*, *Trigger off-cycle run*. ✅ (status + rollups from cycles/cost APIs).
- **In-flight cycle hero** — progress dial, parent identity (hash-sigil), elapsed/ETA, 2×2 stat
  grid. ✅ dial + stats from SSE/cycle; ⏳ the eval-matrix heatmap (Phase 2) and the "kept" rail
  (✅ from lineage active nodes).
- **Active lineages grid** — one card per recent lineage/cycle: sigil, name, status, parent
  Sharpe, micro-stats (experiments / kept / model). ✅
- **Experiment-writers panel** (Ladder + Provenance merged) — scoreboard ranked by accept-rate /
  avg ΔSharpe; each writer row expands to its recent experiments. ✅ from `/ladder` + `/lineage`.
- **Recent cycles table** — id, lineage·parent, experiments, gate ✓, kept, top ΔSharpe, tokens·$,
  when; rows link to the Cycle screen. ✅

### 4.2 Cycle detail — `/optimizer/cycle/:cycle_id`

- **Breadcrumb** — `OPTIMIZER › cycle › <cycle_id>`.
- **Cycle hero** — parent identity, status + progress dial, counter grid (experiments / gate ✓ /
  kept / top Δ / $ spend), action stack (*Sign off summary*, *Pause*, *Skip queued*; *Open in
  Marketplace* stubbed). ✅ counters + cost; ⏳ sign-off (Phase 4).
- **Anti-overfit gate explainer** — the rule in plain language + three **gate buckets**
  Kept / Suspect / Dropped with counts. ⏳ Phase 2 (Suspect tier needs the regime matrix).
- **Eval matrix** — experiments (rows) × regimes (cols), each cell a Δ-Sharpe **delta cell**
  (heat-tinted), with running/queued/failed states; click a cell → flight recorder, click a row →
  Experiment screen. ⏳ Phase 2.
- **Experiments table** ("what the optimizer tried · why · what was kept") — sigil, experiment
  pill (kind), why/what-changed summary, attesters strip, Δ vs parent, kept indicator. ✅ core
  (lineage + diff summary); ⏳ attester strip (Phase 4).
- **Lineage tree** — inline expandable cross-cycle genealogy (parent→child graph). ✅
- **Attester activity** — local attester agents + event log of ENDORSE/QUESTION/REJECT. ⏳ Phase 4.
- **Evening-summary preview** — local unpublished receipt mock + kept list; *Publish to chain*
  stubbed empty-state. ⏳ Phase 4.

### 4.3 Experiment detail — `/optimizer/experiment/:bundle_hash`

- **Breadcrumb** — `OPTIMIZER › cycle › <id> › experiment › <hash>`.
- **Experiment hero** — lineage strip (parent sigil → experiment sigil), verdict badge + Kept,
  headline metrics (Δ-Sharpe vs parent, Sharpe, regimes-kept, sign-offs), action stack
  (*Activate for paper trading*, *Fork to manual edit*, *Re-eval*, *Drop*). ✅ metrics from
  `lineage_node_metrics`; ⏳ regimes-kept + sign-offs (Phases 2/4).
- **Parent diff** (old `Diff` tab) — "What this experiment changed": changed sections shown as
  before/after, unchanged sections collapsed; *Copy diff*, *Full manifest*. ✅ from `/blob/:hash`.
- **Per-regime evaluation** — one card per regime: mini equity curve + Δ-Sharpe + ret/dd/winrt/
  trades. ⏳ Phase 2.
- **Flight recorder** — structured trace (intern → trader → risk → execution), filterable
  inline (not a modal). ⏳ Phase 3.
- **Sign-off receipts** — attester approvals with notes + fingerprints. ⏳ Phase 4.
- **Decision strip** — "Ready to sign off …" with confirm / hold / drop. ⏳ Phase 4.

## 5. Terminology

| Operator label | Backend mapping | Notes |
|---|---|---|
| Optimizer | `autooptimizer` module/API/tables | route `/optimizer`; never bare `optimizer` in code. |
| Cycle | `cycle_id` | one run; groups N experiments. |
| Experiment | candidate / `MutationOutcome` / lineage node | never "Mutation" in UI. |
| Experiment writer | `Mutator` / `MutatorScore` | never "Mutator" in UI. |
| Lineage | `lineage_nodes` (parent_hash graph) | |
| **Kept** | `LineageStatus::Active` | passes a bull **and** a bear/shock regime. |
| **Suspect** | `SingleRegimeEvidence` (new) | passes one side only; created by the regime matrix. |
| **Dropped** | `LineageStatus::Rejected` / `GateVerdict::Fail` | gate failure. |
| Anti-overfit gate | `gate.rs` | re-derived as `PassesBothRegimes` in Phase 2. |
| Honesty check | canary (`cycle_honesty_checks`) | the sabotage canary, surfaced read-only. |
| Flight recorder | `agent_runs` / `spans` (linked in Phase 3) | |
| Attester / sign-off receipt | new concept (Phase 4) | local attester agents. |
| Evening summary | new concept (Phase 4) | local, unpublished by default. |

## 6. Visual system

Adopt the mockup language as **themed primitives in the SPA theme layer**, not inline styles.

- **Color tokens** (semantic): `gold` (kept / positive / primary), `danger` (dropped / negative),
  `warn` (suspect / caution), `info` (running), `violet` (accent); text tiers `text`/`text-2`/
  `text-3`/`text-4`; surfaces `surface-card`/`surface-elev`; borders `border`/`border-soft`/
  `border-strong`; gold accents `gold-soft`/`gold-bg`. Wired through the existing theme variables;
  honor dark-mode border rules (no 100%-white borders).
- **Type**: Geist (sans) + Geist Mono (all metrics, ids, code, traces). Uppercase micro-labels
  (`.ulabel`, 8.5–10px, wide tracking) for eyebrows.
- **Reusable components** (new, under `features/autooptimizer/ui/`):
  `ProgressDial`, `HeatCell` / `DeltaCell`, `GateBadge` (Kept/Suspect/Dropped), `ExperimentPill`
  (kind), `RegimeCard` (mini equity), `HashSigil` (deterministic identicon from `bundle_hash`),
  `Breadcrumb`, `AttesterBadge`, `TraceRow`.
- **Layout compliance**: every screen is a single full-width column (`space-y-*`, no
  `grid-cols-12 … col-span-4` right sidebar — the chat rail owns the right edge). No popups /
  modals / sheets / popovers; the flight-recorder filter, lineage tree, and writer-row expansion
  are inline / dock / accordion.

## 7. Backend changes (full fidelity)

### Phase 2 — Regime matrix (the hero)

- **Config**: a named **regime set** (operator-configurable, default 3–5 scenarios spanning at
  least one bull and one bear/shock, plus a chop). Reuses the existing `Scenario` entity and its
  `regime_label`/`trend_direction` fields.
- **Orchestrator**: extend the cycle loop to evaluate each candidate across the regime set
  (cost = N × current per-experiment backtest; operator shrinks the set to bound it).
- **Storage**: new table `autooptimizer_regime_results(bundle_hash, scenario_id, metrics_json,
  delta_sharpe, equity_curve_json, created_at)`.
- **Gate**: re-derive `GateVerdict` as an aggregate — `PassesBothRegimes` (Kept) /
  `SingleRegimeEvidence` (Suspect) / `Fails` (Dropped). This is the source of the Suspect tier.
- **API**: extend `/api/autooptimizer/cycles/:id` and add per-experiment regime results to the
  experiment detail payload; feeds eval matrix, gate buckets, per-regime cards.

### Phase 3 — Flight recorder

- Link an experiment's eval run(s) to `agent_runs`/`spans` (add `eval_run_id` ↔ lineage linkage),
  expose `GET /api/autooptimizer/traces/:bundle_hash` returning the staged trace
  (intern/trader/risk/execution). Wire the read-only, filterable trace panel.

### Phase 4 — Attesters + receipts + evening summary

- New concept: local **attester agents** that emit sign-off **receipts** (ENDORSE / QUESTION /
  REJECT) per experiment, persisted to a new table; a per-cycle **evening summary** (local,
  unpublished) aggregating kept experiments + receipts. Surface the attester activity log,
  per-experiment receipts, and the summary preview. *Publish to chain* remains a stub.

### Already-live (Phase 1 wires, no backend work)

Experiment-writer ladder (`/ladder`), lineage/genealogy (`/lineage`), parent diff (`/blob/:hash`),
cycle cost/token/$ rollups (`cycle_cost`, `/cycles`), live SSE event feed, honesty-check read-out.

## 8. Phased delivery (Approach A)

| Phase | Deliverable | Backend? | Ships |
|---|---|---|---|
| **1** | 3-screen IA + breadcrumbs + themed visual primitives + terminology; all live panels wired; ⏳ panels as honest empty-states; tab bar deleted; `/optimizer` route + redirect. | No | The redesign. |
| **2** | Configurable regime set + regime-results table + `PassesBothRegimes` gate + Suspect tier → eval matrix, gate buckets, per-regime cards. | Yes | Hero matrix. |
| **3** | Experiment→trace linkage + traces API → flight-recorder panel. | Yes | Trace panel. |
| **4** | Attesters + receipts + evening summary. | Yes | Sign-off flow. |

Each phase is independently shippable behind the Phase-1 shell. **Phases 2–4 each get their own
implementation plan**; this spec is the north-star plus Phase-1 detail.

## 9. Risks / watch-items

- **Regime-matrix cost** — N× backtests per experiment. Mitigated by the configurable set; default
  kept small; surface per-cycle $ prominently.
- **Gate-semantics change** — moving from dual-scenario to `PassesBothRegimes` alters which
  experiments are kept. Land behind the regime-set config and validate against historical cycles
  before making it the default gate.
- **Fast-moving `main`** — the autooptimizer engine churns and `main` is not build-gated
  (per project memory). Keep phases small, build-verify before deploy, watch for consumer drift in
  `xvision-dashboard` route payloads.
- **Empty-states (Approach A interim)** — ⏳ panels must read as intentional ("lights up when the
  regime matrix runs"), not broken.

## 10. Open implementation questions (defer to per-phase plans)

- Exact default regime-set membership and how operators edit it (Phase 2; settings panel is out of
  scope here).
- Whether `agent_runs` linkage is per-experiment or per-eval-run granularity (Phase 3).
- Attester agent model/config and receipt schema (Phase 4).
