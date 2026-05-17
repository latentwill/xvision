# Agent Run Observability — UI Surface Design

**Date:** 2026-05-17
**Surface:** Vite dashboard SPA (`frontend/web/src/`)
**Status:** Draft for user review
**Related:**
- `docs/superpowers/specs/2026-05-15-xvn-agent-run-system-spec.md` — data model / OTel / export spec (Draft for evaluation)
- `team/intake/2026-05-17-agent-run-observability.md` — wave intake, promotes observability to v1
- `frontend/web/src/routes/eval-runs-detail.tsx` — primary surface this design composes onto

## Goal

Define the dashboard UI for agent-run observability so that:

- The trace/spans/model-calls/tool-calls data from the agent-run system is
  **accessible from any run-related surface** (post-hoc eval review AND live
  trading) without crowding the primary view.
- The surface tolerates both **post-hoc** (steady, scrollable) and **live**
  (accumulating, streaming) operation modes — including a running strategy
  executing real-money trades.
- The surface accommodates **tree-shaped trace data** (parent/child spans,
  10 span kinds, potentially hundreds of spans per run).
- The surface provides an **action axis** for checkpointing — "rerun from this
  span" — without becoming an action-heavy primary view.
- The surface uses **no popups, modals, or overlays**. This is a project-wide
  design rule (see "Project rule: no popups" below).

## Decisions

1. Observability is surfaced as a **three-layer stack**: ambient
   status-line strip, summoned bottom dock, and dedicated route. Layers are
   independently shippable.
2. The **dock is the primary working surface** for trace inspection. It is
   resizable, route-persistent, F12-keyboard-summoned, and **minimizes to
   the status-line strip** rather than closing.
3. The **same dock is used in both eval-run detail and live trading
   surfaces.** The dock content adapts (post-hoc trace replay vs. live
   streaming spans) but the shell, summon shortcut, minimize behavior, and
   tree-rendering component are shared.
4. The **dedicated `/agent-runs/:id` route** is the "pop out to full
   screen" target for the dock and the deep-link target for sharing a run
   trace.
5. Tree rendering uses a **horizontal flame-graph** inside the dock and a
   **rail-tree + indented-timeline split-pane** on the dedicated route.
6. **Checkpoint / rerun-from-here** is an action on individual span rows in
   the dock and on the dedicated route — not on the status-line strip.
   Rerun semantics: branch (new run id), never overwrite.
7. The current **agent popup window is removed**. Its content (live agent
   stream during active runs) migrates into the dock's "LIVE" mode.
8. **No popups, no modals, no overlays** is elevated to a project-wide
   frontend rule. Toasts (transient, non-focus-stealing) are not "popups"
   in the sense this rule forbids.

## Scope

This spec covers:

- The three observability surface layers (strip / dock / route) and how
  they relate.
- How the dock behaves in post-hoc and live modes.
- How tree-shaped trace data is rendered in each layer.
- Component reuse and which existing components are touched.
- The follow-ups this work depends on and the follow-ups it generates.

This spec does NOT cover:

- The agent-run data model (defined in
  `2026-05-15-xvn-agent-run-system-spec.md`).
- The OTel/`tracing-opentelemetry` plumbing inside `agent/**`.
- `xvn_run.json` and `xvn_report.md` schema.
- Backend API shape for spans (open question — see Open Questions).
- Implementation plan (this spec needs to be evaluated and reduced to one).

## The three surface layers

```
   layer            surface                role                                   default
   ───────────────  ─────────────────────  ─────────────────────────────────────  ────────
   ① ambient        status-line strip      always-visible heartbeat: span         visible
                    (slim, above body)     count · cost · error flag · click→     when run
                                           expands dock                           is loaded
   
   ② focused        bottom dock            flame-graph + inspector pane; the      hidden
                    (resizable,            primary working surface; F12-style
                    minimizable)           keyboard summon; minimizes back to
                                           the strip rather than closing
   
   ③ dedicated      /agent-runs/:id        full split-pane: rail-tree left,      n/a
                    (separate route)       timeline center, inspector right;
                                           pop-out from the dock; deep-link
                                           target; checkpoint actions live here
                                           too
```

Each layer is independently shippable. Recommended ship order: strip → route → dock.

## Layer 1 — status-line strip

A slim (single-row, ~28px) strip rendered immediately above the main body of
any page that has an agent run in context. The strip is the ambient layer.

### Contents

- Span count (`47 spans`)
- Aggregate model cost (`$0.18`)
- Aggregate model token totals (`12.4k in · 3.1k out`)
- Run duration (`3.4s` for completed, live `0:42` ticking for in-flight)
- Error flag if any span has `SpanStatus::Error` (red dot + count)
- "Expand" affordance — click strip or press `F12` (or rebind) to open the dock
- "Pop out" affordance — small icon, opens `/agent-runs/:id` in same SPA route

### Behavior

- Strip is present on `eval-runs-detail`, the live trading view, and any
  future surface that names an `agent_run_id` in its data.
- Strip never disappears while a run is in context. Closing the dock
  returns to the strip; minimizing the dock returns to the strip.
- Strip is non-interactive beyond expand/pop-out. No tree, no actions.
- Color/density encode state: green for completed, blue for live, amber
  for warnings, red for error.

### Component reuse

- New component (`<RunStatusStrip>`), but uses existing `Pill` and color tokens.
- Mounts inside the route below `<Topbar>` and above the existing body.

## Layer 2 — the dock

The dock is the primary working surface for trace inspection. It is a
resizable bottom pane that can be:

- **Collapsed to the strip** (default; only the status-line strip shows)
- **Peek height** (~240px; flame-graph fits, no inspector)
- **Working height** (~480px; flame-graph + inspector pane visible)
- **Full** (covers ~80% of viewport; flame-graph + inspector + span detail)
- **Popped out** to `/agent-runs/:id` (the dedicated route)

### Contents

The dock's body is a two-column layout when at working height or larger:

```
┌─ dock header ─────────────────────────────────────────────────────────────────┐
│ TRACE  ▓▒░ 47 spans · 12 model · 3.4s · $0.18    [⤡ pop out] [⤓ minimize]    │
├──────────────────────────────────────────────┬────────────────────────────────┤
│ flame-graph (horizontal, scrollable)         │ inspector pane (selected span) │
│ ┌──────────────────────────────────────────┐ │                                │
│ │ agent.run ████████████████████  3.4s     │ │  model.call                    │
│ │  agent.plan █████             0.4s       │ │  ──────────                    │
│ │  model.call ███████  $0.04    1.1s       │ │  provider: anthropic           │
│ │    tool.call run_backtest ████ 1.9s      │ │  model:    claude-opus-4-7     │
│ │  model.call ████   $0.02      0.5s       │ │  in:       8,412 tok           │
│ │  supervisor.review ██         0.1s       │ │  out:      1,204 tok           │
│ └──────────────────────────────────────────┘ │  cost:     $0.0416             │
│                                              │  prompt:   sha256:a1b2…        │
│                                              │                                │
│                                              │  [⤴ jump to decision #14]      │
│                                              │  [⟲ rerun from here]           │
│                                              │  [⎘ copy span json]            │
└──────────────────────────────────────────────┴────────────────────────────────┘
```

At peek height, the inspector hides and only the flame-graph shows.

### Filter / search bar (Logfire-style)

Both the dock and the dedicated route surface a **single compact filter bar**
sitting between the header and the flame-graph / timeline. Inspired by
Pydantic Logfire's faceted filter UI: one input, structured facets, instant
visual narrowing.

```
┌─ filter bar (28px, fits inside dock header row when narrow) ──────────────────────────┐
│ 🔎 [agent: trader ×] [model: opus ×] [kind: tool.call ×] [decision: #14 ×]   ▢ errors │
│    [search span title or attribute…]                              45 → 12 spans  [⌫]   │
└────────────────────────────────────────────────────────────────────────────────────────┘
```

**Faceted filters (chip-style, click-to-remove):**

| Facet         | Source                                           | Compact form               |
|---------------|--------------------------------------------------|----------------------------|
| `agent`       | `AgentRef.role` on the run's strategy            | `agent: trader`            |
| `model`       | `ModelCall.model` (any substring)                | `model: opus`              |
| `provider`    | `ModelCall.provider`                             | `provider: anthropic`      |
| `kind`        | `SpanKind` enum (10 values)                      | `kind: tool.call`          |
| `tool`        | `ToolCall.tool_name`                             | `tool: run_backtest`       |
| `decision`    | decision_index on the linked eval run            | `decision: #14`            |
| `status`      | `ok` / `error` / `in_progress`                   | `status: error`            |
| `min_duration`| span duration ≥ X ms                             | `min_dur: 500ms`           |
| `min_cost`    | model_call cost ≥ $X                             | `min_cost: $0.05`          |
| `attr.<key>`  | any key in `RunSpan.attributes`                  | `attr.prompt_hash: a1b2…`  |
| free text     | substring of `span.name` OR stringified attrs    | (unprefixed in the input)  |

**Interaction model:**

- **Single input field**, free-text by default. Typing `key:value` (or
  `key:"value with spaces"`) commits a facet chip on space or enter.
- **Tab/autocomplete** suggests facet names from the table above, then
  values from the loaded run (e.g. typing `model:` shows the actual models
  present in this run's `model_calls`).
- **Chips are removable** with `×` or backspace at end-of-input.
- **Quick toggles** sit at the right of the bar:
  - `▢ errors` — filter to spans with `status: error` or any descendant
    that errored
  - `▢ live only` — filter to spans with `status: in_progress`
  - `⌫` — clear all filters
- **Counter** shows `<filtered> → <total> spans` so the operator knows
  what's been hidden.
- **Filtered spans dim** in the flame-graph (`opacity: 0.25`) rather than
  hiding — preserves the timeline shape and parent context. The rail-tree
  and indented timeline DO hide non-matching rows (denser, scannable).
- **Filter state persists per route** in localStorage, keyed by
  `agent_run_id`. Reopening the same run restores filters.
- **URL-shareable**: filter state serializes to a `?q=` query param so
  operators can deep-link `/agent-runs/<id>?q=model:opus%20status:error`.

**Compact layout rules:**

- Bar height is **28px** when there are 0 or 1 chips; expands to **two
  rows** (chips on top, input below) only when chips wrap.
- Chip text uses `font-mono` `text-[11px]` to match the strip and dock
  header.
- Chip background: low-opacity accent for the facet kind (matches
  `span-colors.ts` palette — `kind:tool.call` chip is faint emerald,
  `kind:model.call` is faint blue, etc.). Free-text and meta facets
  (decision, status) use a neutral surface tint.
- The bar disappears entirely at dock `peek` height (only the flame-graph
  is shown). At `working` and `full`, it sits as a row inside the dock
  body's top edge.

**Backend implications** (out of this spec, into FU-A):

- The filter is applied client-side in v1 — assumes the loaded run fits
  in memory (hundreds of spans, not millions). Adequate for single-run
  traces.
- A future server-side query API would mirror the same facet vocabulary
  so URL `?q=` params work identically against either path.

### Behavior

- **Keyboard summon**: `F12` (or rebind via the existing keybindings system)
  toggles dock visibility between collapsed-to-strip and the last non-collapsed
  height.
- **Persisted state**: dock height and last-selected span persist per route in
  localStorage. Closing one route's dock does not close another's.
- **Minimize ≠ close**: clicking the minimize affordance returns to the
  strip. There is no "close" — the strip is always the floor.
- **Resize**: dragging the top edge resizes; double-click toggles between
  peek and working heights.
- **Live vs. post-hoc**: spans render identically; the dock auto-scrolls to
  follow the newest span when live, locks to user scroll position otherwise.

### Dock in LIVE mode (live trading, real-money)

The dock is **available on the live trading surface** for any strategy with
an active agent run. This is non-negotiable for operator confidence: when a
running strategy is actively making real-money decisions, the operator needs
the same span-level visibility they have post-hoc.

Live mode differs from post-hoc in:

- The status-line strip pulses blue (live) and shows ticking duration.
- The flame-graph auto-scrolls to follow the newest span; a "lock scroll"
  toggle pins it.
- The inspector pane shows a "STREAMING" badge on the currently-open span
  if that span has not yet finished.
- The "rerun from here" action is **disabled** on spans belonging to the
  currently-running run — checkpoint-rerun is post-hoc only in v1 (see
  Open Questions on whether mid-run branching is feasible).
- An additional dock action appears: **`[⏹ halt strategy]`** in the dock
  header — large, red-bordered, requires confirm-by-typing-strategy-name
  (NOT a popup; an inline confirm row that appears in the dock header).

### Component reuse

- Dock shell reuses `Card` for the inspector pane, `Pill` for badges, the
  same color tokens, and the existing keybindings infrastructure for
  `F12` rebinding.
- Flame-graph component is new and lives in
  `frontend/web/src/features/agent-runs/FlameGraph.tsx`.
- Inspector pane is new but composed of existing primitives.
- Mount: the dock is rendered at the `AppShell` level (not per-route) so it
  persists across navigation when a run is loaded.

## Layer 3 — the dedicated route

`/agent-runs/:runId` is the full-screen home for an agent run.

### Layout

```
┌─ Topbar: Run abc1234… · scenario flash-crash-2024-08 ─────────────────────────┐
├─ SummaryCard: status · agent · duration · cost · token totals · error count ──┤
├───────────────┬───────────────────────────────────────┬───────────────────────┤
│ rail-tree     │ indented timeline (vertical)          │ inspector             │
│ (left rail)   │ (scrollable, virtualized)             │ (selected span)       │
│               │                                       │                       │
│ ▼ agent.run   │ agent.run             3.4s            │ (same content as     │
│   ▼ plan      │   agent.plan          0.4s            │  dock inspector)     │
│   ▼ model     │     model.call $0.04  1.1s            │                       │
│     tool      │       tool.call …     1.9s            │                       │
│   ▼ model     │     model.call $0.02  0.5s            │                       │
│     review    │     supervisor.review 0.1s            │                       │
│   ▼ artifact  │   ...                                 │                       │
│               │                                       │                       │
└───────────────┴───────────────────────────────────────┴───────────────────────┘
```

### Why split-pane on the route but not the dock

The dock optimizes for **density-at-a-glance** (flame-graph). The route
optimizes for **navigation-of-a-large-trace** (rail-tree to scan structure,
indented timeline to read sequence, inspector to drill).

### Bidirectional linking with eval-runs-detail

- `eval-runs-detail.tsx` summary card adds a "View agent trace" link when
  `agent_run_id` resolves on the run.
- `/agent-runs/:runId` summary card embeds the existing `RunChart` and
  links back to the eval-run that produced the financial outcome.

### Component reuse

- Reuses `Topbar`, `Card`, `Pill`, `SummaryCard` shape from
  `eval-runs-detail`.
- Reuses `RunChart` for the embedded equity curve when this agent run has
  an associated `financial_eval_id`.
- The route is mostly composition; the only new component shared with the
  dock is the flame-graph and the inspector pane.

## Checkpoint / rerun-from-here

This is a follow-up feature, but the UI surface must accommodate it from
the start because it is the answer to the "reversibility" axis of the
trace data.

### Surface

- **Dock inspector pane**: `[⟲ rerun from here]` button per span (post-hoc
  runs only).
- **Dedicated route inspector pane**: same button, plus a "Branched runs"
  section in the SummaryCard listing any runs that branched from a span in
  this run.
- **Status-line strip**: no checkpoint action. The strip is read-only.

### Semantics (v1)

- Rerun creates a **new run id** (branch). The original run is never
  mutated.
- The new run starts at the chosen span with the captured deterministic
  inputs (scenario snapshot, agent prompt state, RNG seed).
- The branch relationship is stored on the new run (`branched_from_run_id`,
  `branched_from_span_id`).
- Active-run checkpoint is **out of scope for v1**. Mid-run branching
  requires solving live-agent-state-fork, which is a separate design.

This section is intentionally a **stub** for the checkpoint design. A
follow-up spec must work out:

- What "deterministic inputs" actually means per span kind (model.call vs.
  tool.call vs. sandbox.exec all have different replay surfaces).
- Storage cost of capturing per-span inputs vs. recomputing from the parent
  span.
- UI for navigating between branches of the same root run (tree-of-runs
  view).

## Replacing the agent popup window

The current agent live-output window is a popup. This violates the
project-wide no-popups rule (next section) and must be replaced.

### Migration

- **Old**: popup window streams agent stdout/decisions during active runs.
- **New**: the dock gains a `LIVE` mode (described above). The same dock
  shell, the same minimize-to-strip behavior, the same keyboard summon.
- The popup component is deleted; the route that triggered it instead
  loads the agent run into context, which causes the strip to mount and
  the dock to be summonable.

### Migration risks

- Anything currently coupled to the popup's window object (focus, separate
  workspace) loses that affordance. Acceptable trade-off — the dock keeps
  the operator on-page where the run's context lives.
- A "always show dock for live runs" preference may be useful so the
  operator doesn't have to press `F12` every time a strategy goes live.

## Project rule — no popups, no modals, no overlays

This rule is elevated from "design preference" to **written project rule**
by this spec.

### The rule

- No `Dialog`, `Modal`, `Sheet`, `Popover`, or any overlay that steals focus
  or paints over the primary surface.
- Confirmations, detail views, agent windows, settings panels, error
  recovery flows, share dialogs — everything routes, docks, rails,
  accordions, tabs, or inline-expands.
- **Exception**: toasts (transient, non-focus-stealing feedback) are not
  "popups" in the sense this rule forbids. A toast that auto-dismisses and
  does not require user input is allowed.
- **Exception**: native browser primitives we cannot reasonably replace
  (file picker, browser print dialog). Avoid where possible; do not invent
  new ones.

### Why

- Popups destroy the spatial mental model of the app. The user loses
  track of where they were and what state the underlying surface is in.
- Popups are hostile to keyboard navigation, deep-linking, and
  screen-sharing.
- Popups are a sign of weak information architecture — the question they
  answer should have a home in the actual layout.

### Migration

- Audit every current `Dialog`/`Modal`/`Sheet`/`Popover` usage in
  `frontend/web/src/`.
- For each, decide: route / dock / rail / accordion / tab / inline-expand
  / toast.
- Migration is a separate track; this spec records the rule, not the
  migration plan.

## Component inventory

```
   component                           new?    location
   ──────────────────────────────────  ──────  ────────────────────────────────────────
   <RunStatusStrip>                    NEW     frontend/web/src/features/agent-runs/
   <TraceDock>                         NEW     frontend/web/src/features/agent-runs/
   <FlameGraph>                        NEW     frontend/web/src/features/agent-runs/
   <SpanInspector>                     NEW     frontend/web/src/features/agent-runs/
   <AgentRunDetail> (route)            NEW     frontend/web/src/routes/agent-runs-detail.tsx
   <AgentRunRailTree>                  NEW     frontend/web/src/features/agent-runs/
   <AgentRunIndentedTimeline>          NEW     frontend/web/src/features/agent-runs/
   
   Topbar / Card / Pill / SummaryCard  REUSED  no change
   RunChart                            REUSED  embedded on the dedicated route
   API client (api/agent-runs.ts)      NEW     mirrors api/eval.ts patterns
   SSE stream helper                   REUSED  pattern from openRunStream
   keybindings system                  REUSED  for F12 rebind
```

## Open questions

1. **Span storage shape blocks frontend typing.** If spans live only as
   OTel exports (not SQLite rows), the dock cannot fetch via the
   `api/eval.ts`-style pattern. Resolve in the foundation track before
   this spec becomes a plan.
2. **Dock mount level.** AppShell-level so it persists across navigation,
   or route-level for cleaner code? AppShell is the design intent but it
   complicates the route tree.
3. **Strip-always-visible vs. only-when-run-loaded.** v1 is "only when run
   loaded." Should the strip also show on `/eval-runs` and
   `/agent-runs` list pages summarizing the most recent run? Probably no
   — list pages are not in any single run's context.
4. **Live-mode dock auto-open preference.** When a live strategy is
   running, should the dock auto-open or stay collapsed? Probably
   user preference; default to "collapsed, strip pulses."
5. **Checkpoint scope.** Active-run branching is out of v1; should it be
   in v2 or v3? Depends on whether the agent-state-fork problem is
   tractable in the chosen harness (Cline SDK adapter — see
   `2026-05-17-cline-sdk-agent-replacement-design.md`).
6. **Halt-strategy confirm UX.** Inline confirm-by-typing-strategy-name in
   the dock header — is that the right pattern, or does this deserve
   a dedicated confirmation row above the dock body? Probably the
   latter for visual weight.

## Risks

1. **Dock complexity sprawl.** The dock is the workhorse and risks
   becoming a junk drawer (settings, agent chat, observability, debug,
   logs). Constrain the dock to trace + live agent output. Anything else
   is a separate surface.
2. **Strip becomes wallpaper.** If the strip never changes color or never
   surfaces actionable signal, operators will start tuning it out. The
   strip must earn its persistent presence with meaningful density
   encoding.
3. **Flame-graph performance at scale.** A long agent run (hundreds of
   spans, deep nesting) makes naive flame-graph rendering slow. The
   flame-graph must virtualize.
4. **Live-mode safety.** The dock showing live trades implies the
   operator is watching. If the strip is the only ambient surface and
   the operator is on a different page, error spans must trigger more
   than just a red dot in the strip — at minimum a toast, possibly a
   browser notification. Not in this spec; flag for live-trading-safety
   design.
5. **Same dock, different modes.** Cramming post-hoc inspection and live
   monitoring into the same component risks neither doing well. Counter:
   the structural shape (spans-as-tree-over-time) is identical; only the
   data freshness differs. Keep the modes inside one component until
   evidence forces a split.

## Result

When this spec is implemented:

- Every page with an agent run in context has a persistent ambient strip
  summarizing run cost / span count / errors.
- Operators summon the dock with `F12` (or click the strip) to see the
  flame-graph and inspect spans, without leaving their current page.
- The dock is the same surface in post-hoc and live trading modes;
  operators learn it once.
- The dedicated route is the full-screen home for sharing and deep
  inspection, and the natural target for checkpoint-rerun workflows.
- No popup is ever shown.
- The agent popup window is gone; its content lives in the dock's LIVE
  mode.

## Follow-ups generated by this spec

- **FU-1** (next): Implementation plan that turns this spec into shippable
  tracks. Three tracks suggested: strip / route / dock, ordered as listed.
- **FU-2**: Checkpoint / rerun-from-here design spec. Required before
  the rerun button on span rows can be wired.
- **FU-3**: Popup audit + migration plan. List every
  `Dialog`/`Modal`/`Sheet`/`Popover` in `frontend/web/src/` and assign
  each to a non-popup surface.
- **FU-4**: Live-trading-safety design — error escalation beyond strip
  color (toast, browser notification, halt-on-error?).
- **FU-5**: Mid-run / active-run checkpoint branching — agent-state-fork
  feasibility tied to the chosen harness.
