# Charts Section B4 — Gradient Warm Hero Dashboard (`/charts/hero`)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
>
> **Scaffold status (2026-05-23):** Topology + files + acceptance gates only. Per-step TDD bodies are written when the contract is claimed and B0 + B1 have merged (B4 reuses B1's `KpiCard`, `Topbar`, `DrawdownCard` and extends the equity pane with a gradient variant).

**Goal:** Replace `/charts/hero`'s B0 placeholder with the Chart 05 design — the hero/showpiece variant of the dashboard: 3 radial aura washes + grain overlay, glass-card chrome, gradient-text headline, 5-up KPI row (gold radial glow on the Total Return card), `HeroGradientEquity` (multi-stop warm gradient area-fill + sheen + lead halo), `PerformanceRadar` (6-axis SVG, top-3 strategy polygons), drawdown comparison, and a market-context card.

**Architecture:** New surface `GradientHeroDashboard`. Six new primitives: `AuraBackground`, `GrainOverlay`, `GlassCard`, `HeroGradientEquity`, `PerformanceRadar`, `MarketContextCard`, `GradientHeadline`. Two new uPlot plugins (filling in the stubs left by B1): `xvnGradientFill` (5-stop) and `xvnSheen` (top-band highlight). No new backend work — reuses B1's `/api/v2/charts/dashboards/overview` payload.

**Tech Stack:** React 18 + TypeScript, uPlot, SVG.

**Spec:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B4 milestone, §4A.1 (AuraBackground, GrainOverlay, GlassCard, HeroGradientEquity, PerformanceRadar, MarketContextCard, GradientHeadline), §4A.3 (uplot-plugins.ts — fill xvnGradientFill + xvnSheen).

**Prereqs:**
- B0 merged: warm-palette tokens, typography stack, fixture payload.
- B1 merged: `KpiCard`, `Topbar`, `DrawdownCard`, `uplot-plugins.ts` (with `xvnGradientFill` and `xvnSheen` stubs to fill in here).
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/source/charts/chart-gradient-dashboard.jsx` + README.md §05.

---

## File structure

| Path | Action | Responsibility |
|---|---|---|
| `frontend/web/src/components/chart/v2/primitives/AuraBackground.tsx` | Create | Three absolutely-positioned radial-blur washes behind the content layer. Defaults: 520×520 gold/ember top-left; 680×680 ember/plum bottom-right (-260,-100); 380×380 amber top-right. `opacity: 0.30–0.55`, `filter: blur(20px)`. Light theme passes a desaturated variant; dark/folio-dark/black use full strength |
| `frontend/web/src/components/chart/v2/primitives/GrainOverlay.tsx` | Create | `repeating-linear-gradient(0deg, rgba(241,236,221,0.012) 0 1px, transparent 1px 3px)` at `opacity:0.5`, full-bleed, behind content |
| `frontend/web/src/components/chart/v2/primitives/GlassCard.tsx` | Create | `.glass` utility component. `linear-gradient(180deg, rgba(34,30,20,0.62), rgba(20,18,14,0.78))`, `border: 1px solid rgba(241,236,221,0.07)`, `backdrop-filter: blur(8px)`, inset highlight `0 1px 0 rgba(255,255,255,0.02)` |
| `frontend/web/src/components/chart/v2/primitives/GradientHeadline.tsx` | Create | Cormorant + JetBrains Mono headline with a linear-gradient text fill on the bracketed phrase: `90deg, #E5B86A 0% → #D4A547 35% → #C16A3A 80%` |
| `frontend/web/src/components/chart/v2/primitives/HeroGradientEquity.tsx` | Create | Variant of `UplotEquityPane` with `xvnGradientFill` 5-stop plugin + `xvnSheen` top-band plugin + `xvnLastDot` halo on lead. Single-series (lead only) — non-lead strategies are not overlaid here (use B1 surface for that view) |
| `frontend/web/src/components/chart/v2/primitives/PerformanceRadar.tsx` | Create | Pure SVG, 260×220. 6 axes (Return / Sharpe / Stability / Win Rate / Consistency / Drawdown). Top 3 strategies as overlaid polygons (`fillOpacity 0.10`, stroke 1.4px in strategy color, vertex dots `r=2.2`). 4 ring polygons at 25/50/75/100%. Axis labels at 1.18× radius, tracked uppercase |
| `frontend/web/src/components/chart/v2/primitives/MarketContextCard.tsx` | Create | 2×2 BTC stats grid (Price / Funding / Open Interest / Liq 24h). Below: regime chip row (`BULL · 62%`, `SIDEWAYS · 22%`, `BEAR · 9%`, `HIGH VOL · 7%`). Data: stubbed for B4 (literal); follow-up wires to a real market-context endpoint |
| `frontend/web/src/components/chart/v2/primitives/*.test.tsx` | Create (7 files) | One per primitive |
| `frontend/web/src/components/chart/v2/adapters/uplot-plugins.ts` | Modify | Fill in the `xvnGradientFill` body (5 color stops per spec) and `xvnSheen` body (top-40% gradient highlight) that B1 stubbed |
| `frontend/web/src/components/chart/v2/adapters/uplot-plugins.test.ts` | Modify | Add tests for the two new plugin bodies against a stub Ctx |
| `frontend/web/src/components/chart/v2/surfaces/GradientHeroDashboard.tsx` | Create | Composition: `AuraBackground` + `GrainOverlay` (chrome) · `Topbar` (`GradientHeadline`) · `KpiCard×5` (Total Return card gets radial gold corner glow via prop) · `HeroGradientEquity` · `PerformanceRadar` + `DrawdownCard` (drawdown variant; lead area-filled in gold-tinted red) · `MarketContextCard` |
| `frontend/web/src/components/chart/v2/surfaces/GradientHeroDashboard.test.tsx` | Create | Renders against fixture; aura washes do not increase paint count past one extra composite layer; performance radar polygon `d` strings round-trip against known fixture values |
| `frontend/web/src/components/chart/v2/primitives/KpiCard.tsx` | Modify | Add optional `cornerGlow?: "gold"` prop. When set, renders a `radial-gradient(closest-side, rgba(212,165,71,0.30), transparent 70%)` 120×120 overlay offset `-30 -30 auto auto`. Default behavior unchanged |
| `frontend/web/src/components/chart/v2/primitives/DrawdownCard.tsx` | Modify | Add optional `leadStyle?: "default" \| "gold-tinted-red"` prop for B4 (lead series area-filled in gold-tinted red). Default unchanged |
| `frontend/web/src/routes/charts/ChartsHero.tsx` | Modify | Replace B0 placeholder. Fetch `/api/v2/charts/dashboards/overview`; render `<GradientHeroDashboard payload={data} />` |
| `frontend/web/src/routes/chart-lab/dashboards/Hero.tsx` | Create | Fixture-render |
| `team/contracts/charts-section-b4.md` | Create | Track contract |
| `team/status/charts-section-b4.md` | Create | Status file |

---

## Task topology

1. **Fill `xvnGradientFill` + `xvnSheen` in `uplot-plugins.ts`** — bodies + tests.
2. **Primitive: `AuraBackground`** — three positioned washes; light-theme variant; snapshot.
3. **Primitive: `GrainOverlay`** — single full-bleed div; snapshot.
4. **Primitive: `GlassCard`** — `.glass` utility; snapshot the inset+border+blur stack.
5. **Primitive: `GradientHeadline`** — text-fill gradient; snapshot.
6. **Primitive: `HeroGradientEquity`** — wraps uPlot with the two new plugins + lead halo; renders against fixture.
7. **Primitive: `PerformanceRadar`** — pure SVG; geometry tests for 6 axes, 4 rings, 3 polygons.
8. **Primitive: `MarketContextCard`** — DOM; literal data for B4 (real wiring is a follow-up).
9. **Extend `KpiCard` with `cornerGlow`** — prop + test.
10. **Extend `DrawdownCard` with `leadStyle`** — prop + test.
11. **Surface: `GradientHeroDashboard`** — composition.
12. **Wire `ChartsHero` route** + `/chart-lab/dashboards/hero` fixture page.
13. **Verification gate** — visual diff vs handoff; paint-layer test; CSS `will-change` only added if Lighthouse flags repaint cost.

---

## Acceptance gates

- Aura washes do not increase paint count past one extra composite layer in DevTools' "Layers" panel. Add `will-change: transform` only if Lighthouse / Performance panel flags repaint cost.
- `HeroGradientEquity` area-fill renders identically across dark / folio-dark / black themes (one fixture, three theme snapshots, byte-equal SVG/canvas serialization).
- `PerformanceRadar` polygon `d` strings round-trip against known fixture values (snapshot test).
- `GradientHeadline` text-fill gradient renders correctly even when the bracketed phrase wraps (force-wrap test at narrow width).
- `B5 review milestone`: after B4 ships and the team has lived with all four canvases in production behind the cookie, decide whether `/` should redirect/alias to `/charts/hero`, surface a minimal/hero toggle, or stay as-is. Code commitment for that decision is **separate** from B4 — B4 itself only mounts at `/charts/hero`.
- `npm run typecheck && npm test && npm run build` clean.

---

## Out of scope for B4

- Replacing the default `/` dashboard. Per spec §11.3 resolution, B4 mounts only at `/charts/hero`; the `/`-replacement decision is the B5 review milestone, not a code commitment.
- Real `MarketContextCard` data wiring (B4 uses literal stub data; the real endpoint is a follow-up).
- Animated entrances (last-dot fade-in, heat-band shimmer). Spec's "Open Questions / Next Steps" §3 calls this out as a separate motion pass.

## Sources

- Spec: `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §3 B4, §3 B5 (review checkpoint), §4A.1, §4A.3.
- Design reference: `docs/design/trading-charts/XVN.zip` → `design_handoff_charts/README.md` §05, `source/charts/chart-gradient-dashboard.jsx`, `source/charts/chart-helpers.js` (uPlot plugin source — port `xvnGradientFill` + `xvnSheen` from here).
- B0 plan: `docs/superpowers/plans/2026-05-23-charts-section-b0-foundation.md`.
- B1 plan: `docs/superpowers/plans/2026-05-23-charts-section-b1-overview-dashboard.md`.
