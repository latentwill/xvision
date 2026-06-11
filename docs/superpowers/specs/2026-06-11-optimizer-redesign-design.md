# Optimizer redesign — editorial mission control

**Status:** approved design, 2026-06-11. Brainstormed with the operator; supersedes
the layout (not the data model) of the current `/optimizer` surface.
**Scope:** the whole optimizer family — OptimizerHome, LiveCycleView, CycleDetail,
ExperimentDetail, RunDetail, StrategyInspector.
**Companion docs:** dashboard redesign brief `docs/design/README.md` (visual
language source), design-audit finding F8 (`docs/design-audit/README.md`),
terminology lock `docs/superpowers/specs/2026-05-27-autooptimizer-terminology-lock.md`.
Prior art mined for ideas (not adopted wholesale): `docs/design/XVN_optimizer/ar-*.jsx`.

## Why

The dashboard redesign (PR #897) gave the home page an editorial, honest,
mission-control voice and explicitly left the optimizer pages out of scope. The
optimizer page is "almost there" but fails in four ways (operator-confirmed):

1. **Dead air when idle.** "Waiting for connection…" / "Waiting for the cycle…"
   states and empty charts dominate; the page looks broken when no cycle is
   running (audit F8).
2. **No narrative or hierarchy.** A stack of equal-weight panels; nothing tells
   the operator what matters first.
3. **The live cycle lacks drama.** When a cycle IS running the experience
   undersells it — it should feel like watching the machine think.
4. **Disconnected detail pages.** Cycle/experiment/run/strategy pages feel like
   separate apps.

## Core framing decision

**Mission-control live console** (chosen over "research lab record" and
"pipeline funnel"): the running cycle is the hero, and the idle state is a
designed first-class state — never an apology. Page direction: **editorial
mission report** (chosen over "state-morphing flight deck" and "ticker rail +
canvas"): the page opens with a written headline sentence and always reads
like a report, with liveness layered on.

## 1 · Information architecture — the fold

The family shrinks from five screens to three:

| Route | Role | Absorbs |
|---|---|---|
| `/optimizer` | Mission-control home | **RunDetail** — a session is the home scoped to a session id (`/optimizer?session=<id>`), same anatomy, history filtered |
| `/optimizer/cycle/:id` | Cycle detail | **ExperimentDetail** — experiments live in the inline-expanding board; `?exp=<id>` deep-links open the matching card expanded |
| `/optimizer/strategy/:hash` | StrategyInspector | survives unchanged in role; cross-surface artifact page linked from lineage/marketplace |

Old `/optimizer/experiment/:id` and run-detail routes redirect to their new
homes (experiment → `/optimizer/cycle/:cycleId?exp=:id`). No URL an operator
has bookmarked may 404.

## 2 · Home anatomy (top to bottom, single column)

Single center column inside `DesktopThreePaneShell` — no fourth column, no
side boxes (house rule). Order:

1. **Editorial headline.** A state-aware sentence. It must never assume the
   overnight schedule ("tonight's run") — schedule-flavored copy is allowed
   only when derived from an actual schedule record. States:
   - *Running:* "A run is in progress. 1 cycle running · 5 active lineages."
   - *Paused / cancelling:* same form with the state named.
   - *Idle:* "Last ran 3h ago — kept 2 of 14 experiments." plus best find
     one-liner when available.
   - *Never ran:* "The optimizer hasn't run yet. Launch its first cycle."
   Beneath the headline: the **digest stats line** (experiments this week ·
   kept this week · tokens · LLM spend) and the contextual primary action —
   Launch (idle) / Pause + Cancel (running) / Resume + Cancel (paused). This
   replaces the current CommandBar.
2. **Console module** (§3).
3. **Charts row.** Improvement-over-time + edge-vs-random. All uPlot gradient
   construction guarded against empty/non-finite series (closes audit
   F8/F15 console noise).
4. **Experiment writers ladder** (existing panel, restyled to tokens).
5. **Cycle history.** Rows route to `/optimizer/cycle/:id`.

Mobile: same sections stacked; board cards collapse to rows.

## 3 · The console module

One full-width module, three stacked zones (chosen over side-by-side so each
zone breathes and the stack survives mobile unchanged — operator-requested):

```
┌─ phase ribbon ──────────────────────────────┐
│ PROPOSE ▸ EVAL ▸ GATE ▸ KEEP                │
├─ experiment board ──────────────────────────┤
│ [v3.1.g PASS +0.21] [v3.1.h gating…] [...]  │
├─ narrated feed ─────────────────────────────┤
│ 12:01 Writer gemini-2.5 proposed tighter    │
│       stop → v3.1.g                         │
│ 12:04 Gate PASS · ΔSharpe +0.21 · kept      │
└─────────────────────────────────────────────┘
```

- **Phase ribbon:** the cycle's phases with the live one highlighted; all-done
  when replaying.
- **Experiment board:** one card per experiment in the cycle; cards fill in
  live with gate verdict and ΔSharpe as events land. Spatial view of the
  population.
- **Narrated feed:** every cycle event rendered as a human sentence with the
  numbers inline ("Writer gemini-2.5 proposed a tighter stop (v3.1.g)" /
  "Gate passed v3.1.g: ΔSharpe +0.21 — kept"). Temporal view of the run.
- **Inline expansion (no popups):** any board card or feed item expands as an
  accordion to the full artifact — writer prompt, model response, gate
  numbers, config diff. URL-addressable on CycleDetail via `?exp=`.
- **Idle = replay.** When no cycle is running, the same module replays the
  last completed cycle verbatim (ribbon all-done, final board, full feed)
  under a "Last cycle · <relative time>" label, sourced from persisted cycle
  events — not a live socket. The strings "Waiting for connection…" /
  "Waiting for the cycle…" are deleted from the product.
- **Never-ran:** the module renders a designed explainer of the four phases
  with the Launch action — an honest empty state, not a skeleton.

## 4 · CycleDetail

Repeats the home anatomy at cycle zoom:

1. Editorial headline for the cycle ("Cycle 7f3a kept 2 of 14 — best find
   tightened the stop for +0.21 Sharpe"), breadcrumb strip back to the home.
2. The **same ConsoleModule component** scoped to that cycle, board expanded
   by default, feed complete.
3. Existing cycle-scoped panels (gate scorecard, eval matrix, lineage tree,
   parent/origin diffs) restyled to tokens beneath.

The module is literally shared code with the home — that, plus the repeated
headline anatomy, is what makes the family feel like one app.

## 5 · Visual language

Inherits the dashboard redesign brief (`docs/design/README.md`) wholesale:

- Quant mission-control, calm density; dense-but-legible single column.
- Numbers are the typography: tabular Geist Mono numerals; labels small,
  uppercase, tracked, muted.
- Color is signal only: green = kept/pass/live; amber = suspect/gating/stale;
  red = fail/veto. Gray ramp otherwise. Theme border tokens only — no white
  borders in dark mode.
- Honesty chips: sample size, freshness, spend on every metric that has them.
- Operator-surface vocabulary per the terminology lock: **Experiment**,
  **Experiment writer**, **Rejected**, **Suspect**, **honesty check**. The
  developer-surface `autooptimizer` codename is untouched; DSPy
  `Optimizer*`/`optimization` tokens are untouched.

## 6 · Components & tech

- New shared primitives in `frontend/web/src/features/autooptimizer/ui/`:
  - `EditorialHeadline` — state-aware sentence + digest line + action slot.
  - `ConsoleModule` — composition of `PhaseRibbon`, `ExperimentBoard`,
    `NarratedFeed`.
  - `ExpandableArtifact` — the inline accordion for prompt/response/gate
    payloads.
- `ConsoleModule` **replaces `LiveCycleView`** (`features/autooptimizer/LiveCycleView.tsx`,
  ~1100 lines); the SSE plumbing in `hooks/useCycleEventStream` is reused, the
  presentation is retired with it.
- **`narrateEvent(event) → string`** is a pure selector over the existing SSE
  cycle-event types; unit-tested per event type without a stream. The feed
  renders narrations; the board derives card state from the same events.
- Replay reads persisted cycle events through the same data path CycleDetail
  uses. If an events-by-cycle endpoint is missing, adding it is in scope for
  the implementation plan (confirm during planning).
- Charts stay on uPlot; no new charting library. Gradient guards required.
- Vitest coverage on: headline copy per state (including never-ran and the
  no-"tonight" rule), `narrateEvent` per event type, board state derivation,
  deep-link expansion (`?exp=`), and route redirects.

## 7 · Out of scope

- No marketplace or lineage-page redesign (they keep linking into
  StrategyInspector).
- No DSPy prompt-optimizer surface changes.
- No backend changes beyond a possible events-by-cycle read endpoint for
  replay (see §6).
- No new charting library; no popups/modals of any kind (house rule).
