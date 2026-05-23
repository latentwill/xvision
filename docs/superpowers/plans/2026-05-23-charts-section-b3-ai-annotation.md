# Charts Section B3 — AI Annotation Chart (`/charts/annotated`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Scaffold status (2026-05-23):** Topology + files + acceptance gates only. Per-step TDD bodies are written when the contract is claimed and B0 has merged. B3 is independent of B1/B2 because it doesn't compose `MultiStrategyEquityPane` — it can run in parallel with B1 once B0 lands.

**Goal:** Replace `/charts/annotated`'s B0 placeholder with the Chart 03 design — single-asset KlineCharts candle pane with EMA(21) overlay, AI-callout overlay layer with two-row callout placement + dashed connectors anchored to real candle indices, collapsible insight log right rail, and a pulsing "AI Engine · live" pill.

**Architecture:** New surface `AIAnnotationDashboard` (chart-only frame). Four new primitives: `AnnotationOverlay`, `Callout`, `InsightLog`, `AiEnginePill`. One new adapter `kline-anchor.ts` that wraps a KlineCharts instance to expose `xForIndex` / `yForPrice` / `subscribeLayout` (ports the handoff's `convertToPixel` + `onVisibleRangeChange` pattern). Two new backend routes: `/api/v2/charts/annotated/:run_id` (source=run) and `/api/v2/charts/annotated/live/:symbol` (source=live; returns empty annotations until the producer ships — see §9 "Out of scope" in the spec).

**Tech Stack:** React 18 + TypeScript, KlineCharts (already pinned by A-M0), TanStack Query, react-router-dom v6 `useSearchParams`. Rust 2021 + axum for the two endpoints.

**Spec:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B3 milestone, §4A.1 (AnnotationOverlay, Callout, InsightLog, AiEnginePill), §4A.3 (kline-anchor adapter), §6.2 (AnnotatedChartPayload — note `source: "run" | "live"` provenance from the §11.2 resolution).

**Prereqs:**
- B0 merged: `Annotation` and `AnnotatedChartPayload` types in `frontend/web/src/components/chart/v2/types.ts`; `annotations.json` fixture; `/charts/annotated` placeholder route.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-ai-annotation.jsx` + README.md §03.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `frontend/web/src/components/chart/v2/adapters/kline-anchor.ts` | Create | Wraps a KlineCharts instance. Exposes `xForIndex(idx) → px`, `yForPrice(p) → px`, `subscribeLayout(cb) → unsubscribe`. Fans out `onVisibleRangeChange` + ResizeObserver into one layout-changed callback |
| `frontend/web/src/components/chart/v2/adapters/kline-anchor.test.ts` | Create | Mocked KlineCharts instance returns known pixel values for known indices/prices; subscriptions fire on resize |
| `frontend/web/src/components/chart/v2/primitives/Callout.tsx` | Create | ~210px card. Border `rgba(212,165,71,0.32)` (gold) or `rgba(200,68,58,0.32)` (red when `danger:true`). Inner: `.callout-head` eyebrow (`type · conf 74%`), Cormorant title 14px, body 11.5px in `--text-2`, foot row `idx · 22` + action in accent |
| `frontend/web/src/components/chart/v2/primitives/AnnotationOverlay.tsx` | Create | Absolutely-positioned layer above `KlineCandlePane`. SVG `pointer-events:none` connectors (dashed `3,3`) from candle anchor (`r=6` ring 55% + `r=2.4` solid dot) to nearest callout corner. Two callout rows: top at `y=24`, bottom at `y=H-180`. Spread evenly across `(width - 12 - 80)`. Re-anchors via `kline-anchor.subscribeLayout` |
| `frontend/web/src/components/chart/v2/primitives/AnnotationOverlay.test.tsx` | Create | Two-row placement math; spread math for n=1..7 callouts; danger callout border switches color; filter toggle hides callouts by `type` |
| `frontend/web/src/components/chart/v2/primitives/InsightLog.tsx` | Create | Collapsible right rail (280px ↔ 36px via `grid-template-columns` transition 200ms ease). Cards: title row (Cormorant 14px / Mono timestamp), body 11.5px, footer (category pill + confidence). Left edge 2px accent bar (gold / red). Collapse button `›` shrinks to a 36px rail showing vertical "Insight Log · N events" + 6 colored dots |
| `frontend/web/src/components/chart/v2/primitives/InsightLog.test.tsx` | Create | Expand/collapse state; filter pass-through; entry count and dot color rendering |
| `frontend/web/src/components/chart/v2/primitives/AiEnginePill.tsx` | Create | Animated gold-dot pill. CSS keyframe `aiPulse` 1.8s ease-out infinite: `scale(1, opacity:0.7) → scale(3.4, opacity:0)`. Inner solid dot stays at scale(1) opacity(1) |
| `frontend/web/src/components/chart/v2/surfaces/AIAnnotationDashboard.tsx` | Create | Composition: `ChartFrame` · header (xvn lozenge + symbol pill + price + 24h change + `AiEnginePill` + filter toggle row `Patterns Risk Flow All`) · `<grid 1fr 280px>` of (`KlineCandlePane` + `AnnotationOverlay`) and `InsightLog` · status footer (JetBrains Mono 10.5px) |
| `frontend/web/src/components/chart/v2/surfaces/AIAnnotationDashboard.test.tsx` | Create | Renders fixture; pan/zoom re-anchors callouts; filter pass-through hides; collapse state preserved across re-renders |
| `frontend/web/src/routes/charts/ChartsAnnotated.tsx` | Modify | Replace B0 placeholder. Parse `?source=run\|live`, `?run_id=`, `?symbol=` params. Default = `source=run`. Fetch the matching endpoint. Render `<EmptyState text="annotation producer not configured" />` when `source=live` and `annotations.length === 0` |
| `frontend/web/src/api/charts.ts` | Modify | `fetchAnnotated({ source, runId?, symbol? }): Promise<AnnotatedChartPayload>` |
| `crates/xvision-engine/src/api/charts_annotated.rs` | Create | Two handlers: `GET /api/v2/charts/annotated/:run_id` reads stored annotations from the run; `GET /api/v2/charts/annotated/live/:symbol` returns `annotations: []` with a `source: "live"` body — the producer wiring is **out of scope per spec §9**, and the UI handles the empty case |
| `crates/xvision-engine/src/api/mod.rs` | Modify | Register the two new routes |
| `crates/xvision-engine/tests/charts_annotated_endpoints.rs` | Create | Both endpoints return correct shape; live returns `annotations: []`; run returns whatever the test DB seeded |
| `frontend/web/src/routes/chart-lab/dashboards/Annotated.tsx` | Create | Fixture-render the surface against `annotations.json` |
| `team/contracts/charts-section-b3.md` | Create | Track contract |
| `team/status/charts-section-b3.md` | Create | Status file |

---

## Task topology

1. **Adapter: `kline-anchor.ts`** — wraps KlineCharts. TDD against a mock-instance double.
2. **Primitive: `Callout`** — DOM only. Snapshot test the gold vs red border switch.
3. **Primitive: `AnnotationOverlay`** — depends on `kline-anchor`. Spread math + connector geometry tests with stubbed anchor.
4. **Primitive: `InsightLog`** — collapse animation; vertical-text rail mode; tests.
5. **Primitive: `AiEnginePill`** — CSS keyframe; snapshot the rendered style.
6. **Backend: `/api/v2/charts/annotated/:run_id` + `/live/:symbol`** — both endpoints; live always returns empty until the producer ships.
7. **Surface: `AIAnnotationDashboard`** — composition; filter pass-through; pan/zoom re-anchor; collapse persistence.
8. **Wire `ChartsAnnotated` route** — parse `?source=`, `?run_id=`, `?symbol=`; default `run`; handle empty `live` response with EmptyState.
9. **`/chart-lab/dashboards/annotated`** — fixture page.
10. **Verification gate** — pan/zoom test; cookie on/off; live mode empty state; visual diff vs the handoff PNG.

---

## Acceptance gates

- Pan / zoom the candle chart → all visible callouts re-anchor to the right candle index; connectors do not visibly lag (≤16 ms drift on a 60 Hz frame).
- Callouts whose anchor index is off-screen are not rendered (clip cleanly at chart edges, including the price-axis margin on the right).
- Filter toggle (`Patterns | Risk | Flow | All`) hides both the chart callouts and the matching insight log entries by `type`.
- Insight log collapse animates 200ms ease without layout shift in the candle pane (test: capture `KlineCandlePane.getBoundingClientRect()` before and after collapse — width changes by the rail's delta, height does not).
- `source=run` reads stored annotations from the run; `source=live` returns `annotations: []` with `source: "live"` and the UI renders the "annotation producer not configured" EmptyState (no console errors).
- `source` default is `run` when the query param is absent.
- `npm run typecheck && npm test && npm run build` clean.
- `cargo test -p xvision-engine` clean.

---

## Out of scope for B3

- **The annotation producer itself.** B3 consumes annotations. The LLM call / schedule / persistence that emits them is a separate spec (called out in the spec §9). For `source=run`, B3 reads whatever is stored; if nothing is stored, the chart renders without callouts and the insight log says "no annotations".
- Live streaming append of new annotations as they're generated (B3 reads at-load).
- Drag-to-reorder of insight log entries.

## Sources

- Spec: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B3, §4A.1, §4A.3, §6.2, §11.2.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/README.md` §03, `source/charts/chart-ai-annotation.jsx`.
- B0 plan: `docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md`.
