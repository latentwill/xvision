# Charts Section B5 — Hero-default review checkpoint

> **For agentic workers:** This is a **review milestone**, not an implementation plan. No code commits expected unless the review concludes with a "yes" decision; in that case spawn a short follow-up PR per §3 below.

**Goal:** After all four Track-B canvases (B0–B4) are live in production, decide whether the default `/` Dashboard should redirect / alias to `/charts/hero` (the GradientHeroDashboard), keep its current minimal home, or expose a per-user toggle.

**Decision deferred from:** `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md` §11.3 resolution. Per user direction 2026-05-23: "I want to see how all charts feel in production before deciding."

**Prereqs:**
- B0 (#556), B1 (#559), B2 (#560), B3 (#561), B4 (#563) all merged.
- B-rollout (separate follow-up PR — drops the `xvn.chartv2=1` cookie gate from the sidebar) merged.
- The team has lived with `/charts/{overview,compare,annotated,hero}` for **at least one week** in production. Earlier reviews risk recency bias.

---

## 1. Decision options (from spec §11.3)

| Option | What lands | Risk | Reversibility |
|---|---|---|---|
| **(a) Keep `/` as-is; hero stays at `/charts/hero`** | Nothing. | Lowest. | Trivial — no code change to revert. |
| **(b) Redirect `/` → `/charts/hero`** | One `Navigate` in `frontend/web/src/routes.tsx`; the existing HomeRoute becomes orphaned (delete or keep behind `/home`). | Aura washes + backdrop-filter are heavy; some users may find them loud for daily use. | One commit to revert. |
| **(c) Add `?variant=minimal\|hero` to `/`** | Small refactor: `/` reads the query param and dispatches to `DarkMinimalDashboard` (default) or `GradientHeroDashboard`. Cookie-persist the choice. | More code paths; per-user state to migrate. | Medium — touches the route, persistence, settings UI. |

The team should pick one with the criteria in §2 below. Default-of-the-default proposal: **(a)** unless a strong majority finds the hero variant pleasant as a daily landing.

---

## 2. Review criteria (the operator running B5 should write a 1-paragraph decision in this file before closing)

Pick the answer that's correct in your judgement; not all checkboxes need to be true.

- [ ] **Performance.** Lighthouse + Chrome DevTools "Layers" panel show `/charts/hero` does not introduce more than +1 composite layer or +200ms LCP delta vs `/charts/overview`. If hero is heavier, default to (a).
- [ ] **Daily-use ergonomics.** When stakeholders open `/` first thing in the morning, do they want the hero treatment (aura, gradient hero, performance radar) or the minimal grid? If split, lean (c).
- [ ] **Mobile.** The hero variant is desktop-first (per the spec — mobile is a separate workstream). If mobile users hit `/`, (b) would surface a desktop-only treatment to phones. Disqualifies (b) until mobile lands.
- [ ] **No-popups guardrail.** Both variants comply (no modals, no overlays beyond aura/grain). No friction here.

---

## 3. If the review concludes with (b) or (c) — implementation hand-off

### (b) Redirect `/` → `/charts/hero`

```tsx
// frontend/web/src/routes.tsx — replace the root index route
{ index: true, element: <Navigate to="/charts/hero" replace /> }
```

Either delete `HomeRoute` + its file, or remount at `/home` for muscle-memory deep-links. Add a 1-line redirect rule covering the previous `/` URL in any docs. Open a one-commit PR titled `feat(charts): default / to /charts/hero per B5 review`.

### (c) Variant toggle on `/`

1. Frontend: `frontend/web/src/routes/home.tsx` (or wherever `HomeRoute` lives) reads `useSearchParams().get("variant")` ∈ {`minimal`, `hero`} (default `minimal`), and dispatches the matching surface.
2. Persistence: write the choice to `localStorage` (key `xvn.dashboard.variant`) on change; read on mount as the fallback. Per-user is sufficient — no backend.
3. UI: small toggle in the page header (radio pill, "Minimal | Hero"). Mirror the existing theme toggle pattern in `Sidebar.tsx`.
4. Tests: a test that toggling persists across re-renders.

---

## 4. Decision record (fill in when B5 review fires)

| Field | Value |
|---|---|
| Decision date | _yyyy-mm-dd_ |
| Decision owner | _name_ |
| Days in production before review | _N_ |
| Option chosen | _(a) / (b) / (c)_ |
| One-paragraph rationale | _free-form_ |
| Implementation PR (if (b) or (c)) | _#NNN_ |

Once filled, archive this plan under `docs/superpowers/plans/archive/`.

---

## 5. Sources

- Spec §11.3 (the original three-option breakdown).
- Spec §3 B5 (the review-checkpoint placeholder).
- B4 PR #563 (the canvas that triggered this review).
