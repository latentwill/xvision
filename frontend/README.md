# frontend

Reserved for the xvision web UI. **No production frontend exists yet** — this directory currently holds the design handoff bundle that will become the visual source of truth when frontend work begins.

## Layout

- `prototype/` — runnable HTML/CSS/JSX mockup of the v1 dashboard, exported from Claude Design (claude.ai/design). Six screens on a 1440×900 design canvas. **Read this directory directly; do not screenshot.**
- `prototype/_handoff/` — original handoff README, chat transcript, and upload assets (folio reference images). Provided for context, not implementation.

## Viewing the prototype

The prototype is plain HTML/JSX served by Babel-in-the-browser, so no build step is required.

```sh
cd frontend/prototype
python3 -m http.server 8000
# open http://localhost:8000/
```

`index-print.html` is a print-friendly stack of all six artboards (Cmd+P → Save as PDF, landscape, 1440×900).

## V1 scope

Six screens, all in Folio dark theme:

| Screen | Path in prototype | Purpose |
|---|---|---|
| Home — Control Tower | `screen-home.jsx` | KPI tiles, equity curve, top strategies, recent runs, open positions, agent rail |
| Setup wizard | `screen-setup.jsx` | Chat-driven first-run + on-demand strategy drafting |
| Strategies list | `screen-strategies.jsx` | Drafts/Validated/Archived with template + lineage columns |
| Inspector / Authoring | `screen-inspector.jsx` | Bundle outline tree + LLM slot split editor (form ↔ live preview) + validation rail |
| Eval runs | `screen-eval-runs.jsx` | Leaderboard with filters and Sharpe/Return/DD/Win-rate columns |
| Run detail | `screen-run-detail.jsx` | KPI tiles, equity vs buy-and-hold, findings list with "Draft variant from this →", trade ledger |

Out of v1 scope: marketplace, autoresearch, journal.

## Visual tokens (Folio dark)

Defined in `prototype/styles.css`:

- **Surfaces** — warm-black ink (`#0F0E0C` bg → `#221E14` deepest panel)
- **Borders** — warm slate (`#2A2618`, `#3A3322`, `#221F15`)
- **Text** — `#F1ECDD` → `#4A4536` (4 levels)
- **Accent** — amber-gold `#D4A547` (with `--gold-soft`, `--gold-bg`)
- **Status** — warn `#DB9230`, danger `#C8443A`, info `#6F8FB8`
- **Type** — Cormorant Garamond italic (display), Inter (UI), JetBrains Mono (numerics, IDs)
- **Radii** — 6px cards, 4px small

## Domain mapping (prototype → xvision crates)

Per `CLAUDE.md` terminology (`cycle_id`, `agent_id`, `StrategyBundle`):

- "Strategy" in prototype copy = `StrategyBundle` in code
- Inspector "Layers" (Data / Regime / Intern / Trader / Entry-Exit / Risk / Execution) map to the pipeline crates: `xvision-data`, `xvision-intern`, `xvision-trader`, `xvision-risk`, `xvision-execution`
- "Run ID" (e.g. `01H8N7Z`) is a ULID; one run produces many `cycle_id`s (briefing → decision → outcome) — see `docs/superpowers/plans/2026-05-10-terminology-rename-option-b.md`
- The Setup wizard's "Drafting" panel is the user-facing surface of the engine API foundation work tracked in `docs/superpowers/plans/`

## When real frontend work begins

Recreate the screens **pixel-perfectly** in whatever stack fits (likely Vite + React + Tailwind, given the prototype is React/JSX). Match the visual output; do not copy the prototype's internal structure unless it happens to fit. The prototype's hardcoded mock data should be replaced with calls into the xvision engine API.

The handoff README in `prototype/_handoff/HANDOFF-README.md` is the original instructions from the design tool — read it once for context, then ignore (it's frozen at handoff time).
