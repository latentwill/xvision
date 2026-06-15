# Product

## Register

product

## Users

Operators and quantitative traders running the xvision trading-strategy
optimizer. They work at a desk, often with a live optimizer run executing in a
terminal (`xvn optimize run`) while they watch the dashboard SPA in a browser
tab. Their context is high-stakes and detail-dense: they are deciding whether an
automated optimization run is healthy, what stage it is in, whether to pause or
cancel it, and which experiments survived the gate. The primary job on the
Optimizer screen is **situational awareness of an in-flight run** and triage of
its results.

## Product Purpose

xvision optimizes trading strategies by repeatedly proposing experiment
variations, backtesting them across market regimes, gating each result honestly
against its parent, and keeping the winners into a lineage. The dashboard is the
operator surface for this loop: launching/observing runs, reading the live
"console" (phase ribbon → experiment board → narrated event feed), and browsing
historic cycles. Success looks like an operator being able to glance at the page
and instantly know: is a run happening right now, what is it doing, and is it
producing edge.

## Brand Personality

Precise, candid, fast. Three words: **instrument-grade, honest, calm.** The
voice is that of a serious trading terminal — plain-language operator labels over
developer jargon, no hype, no fake precision. Numbers are shown with their
sample sizes ("honesty chips"); states are never overstated. Motion is
functional (a live pulse means *live*), never decorative.

## Anti-references

- Generic SaaS dashboards with hero-metric templates (big gradient number, three
  supporting stats) — this is a working instrument, not a marketing page.
- Anything that paints a finished/idle state over a running process (the current
  bug: an all-green "Cycle complete" ribbon shown during an active run).
- Popups / modals / sheets / right-side floating cards (explicitly banned in
  CLAUDE.md — the desktop shell reserves the right rail for the chat pane).
- Decorative glassmorphism, gradient text, side-stripe accent borders.

## Design Principles

1. **Truthful state above all.** The UI must never imply a run is finished/idle
   when it is active, or vice-versa. Running, paused, cancelling, and idle each
   read unambiguously at a glance, and "running" is derived from the most
   resilient signal available (server status → live stream → in-flight cycle).
2. **One source of truth per concept.** "Is a run active?" is computed once and
   shared; surfaces never diverge (e.g. headline says running while the console
   says complete).
3. **Show the work, honestly.** Live telemetry (phase, experiments, events) is
   surfaced as it happens; sample sizes and freshness stamps accompany every
   claim; degrade gracefully when telemetry is missing rather than faking it.
4. **Calm density.** Pack high information into inline, full-width strips with
   clear rhythm — no side boxes, no overlays, no scaffolding eyebrows.
5. **Motion means meaning.** Animation is reserved for genuinely live state and
   always has a reduced-motion fallback that stays visually distinct.

## Accessibility & Inclusion

Dark-first theme with a light theme; body text must meet WCAG AA (≥4.5:1).
All status changes are conveyed by text + shape, not color alone (e.g. a live
pulse pairs with a "RUNNING" label and a `role="status"` announcement). Every
animation respects the global `prefers-reduced-motion` safeguard. Interactive
controls are keyboard-reachable and labelled.
