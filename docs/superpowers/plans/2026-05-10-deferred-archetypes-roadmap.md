# Deferred Archetypes — Post-v1 Roadmap

> **Purpose:** sequencing + dependencies for the UX archetypes that `ui-elements.md` v0.2 §16 explicitly defers past v1. These designs exist (full prompts in `docs/design/gptprompts.md` §19–§23), but their plans don't yet — and they shouldn't, until the v1 surface ships and we have real users using it.
>
> **Status:** decision doc, not an implementation plan. Each entry is a one-paragraph sketch + dependencies + the user story that justifies picking it up. Treat as a backlog ordering, not a commitment.

---

## Sequencing

The post-v1 archetypes split into two groups: **chrome polish** (small, lands fast, no new user-facing concepts) and **new surfaces** (big, requires its own plan + new user story to validate). Chrome polish ships first because the marginal cost is low and the v1 surface gets sharper.

| Phase | Archetype | Lands as | Cost | Trigger |
|---|---|---|---|---|
| Post-v1 Phase 1 | **Pass-Ribbon (Move H)** — ambient ticker | Chrome update to `base.html` | 1–2 days | After live cockpit (Plan 2c) ships and ≥3 deployments are running concurrently for ≥1 user |
| Post-v1 Phase 1 | **Lineage tree on `/strategies` (Move G)** | New view on existing route | 2–3 days | When ≥3 sibling drafts from one root exist for any user (the `Forked from` column is the v1 stub) |
| Post-v1 Phase 2 | **Slot Machine starter spinner (L1 onboarding)** | New `/setup?starter=1` mode | 3–4 days | When the slot-machine engine spec ships (`docs/superpowers/specs/2026-05-08-slot-machine-design.md`) and we have evidence that L1 users bounce off the open-ended Wizard chat |
| Post-v1 Phase 3 | **Spreadsheet (parameter sweep matrix)** | New `/sweeps/<id>` route | 5–7 days | When the eval engine supports batch-run dispatch + a paid-tier user asks for it |
| Post-v1 Phase 4 | **Power Notebook (`/lab`)** | New `/lab` route | 7–10 days | When ≥3 L4 power users explicitly ask for programmatic access — earlier and we'd be building speculatively |
| Post-v1 Phase 5 | **Canvas (spatial node graph)** | New `/canvas` route | 10–14 days | When designer-oriented power users (Persona D, not yet defined) request it; OR when the strategy bundle becomes graph-shaped enough that flat forms feel painful |

The Lab Notebook (`/journal`, Move F) is **also deferred** but has its own plan (`2026-05-10-lab-notebook-plan.md`) — it lands ahead of this roadmap, between v1 and post-v1 Phase 1.

---

## Per-archetype dossiers

### Pass-Ribbon (Move H)

**Source:** `gptprompts.md` §22, `ui-elements.md` v0.2 §15 (deferred but retained).

**One-line sketch:** 64px-tall persistent footer strip across every authenticated route, showing 3 live-deployment pills with last-decision summaries. Collapses to an 8px just-status-colors bar.

**User story:** "I want to glance at my running deployments without leaving whatever I'm doing." This is exactly what the v1 Control Tower's `Live now` panel does on `/`, but the user has to be on `/` to see it. The ribbon globalizes that.

**Dependencies:** Plan 2c (live cockpit + scheduler events SSE). Chat-rail-persistence plan (so the ribbon can sit *under* the rail without overlap on small viewports).

**Cost-to-value:** The ribbon is one HTML partial + one JS subscriber to the existing `scheduler_events` SSE. Cheap. The reason it's not in v1 is that for a single deployment it's noise rather than signal — the v0.2 ideonomy evaluation explicitly downgraded it once Move A (Control Tower) absorbed its primary use case. Pick it up when concurrent-deployment density justifies it.

**Stub today:** Add a feature flag `XVN_PASS_RIBBON=1` in v1.1 that turns it on for power users; promote to default when ≥30% of users have ≥3 concurrent deployments.

---

### Lineage tree on `/strategies` (Move G)

**Source:** `ui-elements.md` v0.2 §5 + §16. Currently stubbed by the `Forked from` column.

**One-line sketch:** When a user has ≥3 sibling drafts forked from one root, the `/strategies` table grows a "Tree view" toggle that renders the lineage as a left-side tree on a strategies-detail panel (similar to a git log graph).

**User story:** "I've forked btc-momentum 4 times this week — show me how they relate, which one is best, what the diff is between them." Today the user has to flip between the `/strategies` list and `/eval/compare` manually.

**Dependencies:** Slot Machine spec (provides the `parent_draft_id` field already; `Forked from` column reads it). LLM-providers plan's `Fork with different model →` action populates lineage rows.

**Cost-to-value:** Tree rendering is moderate (recursive accordion or `<details>` chains). The "trigger when ≥3 sibling drafts exist" predicate is one DB query. Reasonable post-v1 first pickup.

---

### Slot Machine starter spinner

**Source:** `gptprompts.md` §23 + `docs/superpowers/specs/2026-05-08-slot-machine-design.md`.

**One-line sketch:** A `/setup?starter=1` variant that replaces the open-ended Wizard chat with three large slot-reel pickers (Template / Asset / Risk). User pulls reels, gets a complete strategy suggestion, can run paper mode in under a minute.

**User story:** "I just installed xvn. The Wizard chat is intimidating because I don't know what to ask. Give me a single-button suggestion." The reels make the choice space concrete and the action low-stakes.

**Dependencies:** Slot machine engine ships first (separate spec + plan, deferred). Templates registry from Plan 2a. Default risk presets.

**Cost-to-value:** UI work is small (one new template + one JS module + reels CSS). Engine work is the real lift — meta-strategy generation, variant pool sampling. Don't ship the UI without the engine, or it's a wireframe demo.

---

### Spreadsheet (parameter sweep matrix)

**Source:** `gptprompts.md` §21.

**One-line sketch:** Heatmap-style matrix of param × param sweeps (e.g. `bb_period × rsi_oversold`), one Sharpe value per cell, drilldown rail on the right showing the selected cell's full run.

**User story:** "I want to find the parameter combination that wins on this scenario, without manually running 20 evals." Power-user research surface.

**Dependencies:** Eval engine batch-run dispatch (today the engine runs one strategy × one scenario at a time). Scenario subset filtering (multi-asset universe). Run-result caching. Storage of seed-stability variants (the small heatmaps for seed #1234 / #5678).

**Cost-to-value:** Significant engine work — a batch dispatcher, throttle policies, persistent matrix rendering. UI is moderate. This is a paid-tier surface in the gptprompts.md framing; its arrival is gated by whether the product needs a paid tier.

---

### Power Notebook (`/lab`)

**Source:** `gptprompts.md` §20.

**One-line sketch:** Jupyter-style notebook driving the xvn MCP API programmatically. Cells: `draft = template('mean_reversion')`, `set_prompt(...)`, `validate(draft)`, `run_eval(...)`. Output cells show responses inline.

**User story:** "I'm an L4 researcher; the Wizard is too constrained. Give me a REPL where I can compose strategies in code, branch quickly, version everything." This is `/journal` for *programmatic* research, not narrative research.

**Dependencies:** MCP server stable + documented (Plan 2a). Notebook persistence (kernel state, cell history). Markdown-cell rendering in browser. Distinct from the Lab Notebook (`/journal`) — the Notebook is a chronological journal, the Power Notebook is a kernel-backed REPL.

**Cost-to-value:** High build cost. Wait until ≥3 L4 power users explicitly ask for it; before then, they can use `xvn mcp` directly from a Python client.

---

### Canvas (spatial node graph)

**Source:** `gptprompts.md` §19.

**One-line sketch:** Full-bleed node graph for composing strategies visually — Data → Regime → Signal → Decision → Broker, with editable wires + a skill drawer + an inspector panel.

**User story:** "Inspector forms feel like filling out a tax return. Let me move things around in space." Designer-oriented; high power-user appeal but unclear v1 user need.

**Dependencies:** Strategy bundle's graph shape. A node-graph editor library (cytoscape, rete, or hand-rolled). Drag-and-drop skill drawer (skills plan).

**Cost-to-value:** Largest single build. Don't pick up until the strategy bundle's graph shape is stable for ≥3 months and someone with a designer's instinct champions it. Otherwise it becomes a half-finished playground.

---

## Triggers — what would move something up

A deferred archetype gets promoted when one of these happens:

1. **Real user demand.** ≥3 unrelated users explicitly request the missing surface.
2. **A new dependency lands.** The slot machine engine ships → Slot Machine UI becomes pickable. Eval batch dispatch ships → Spreadsheet pickable.
3. **A v1 surface is straining.** If `/strategies` is unusable at 50+ drafts, lineage tree comes earlier than planned. If `/setup` chat-bounces too many L1 users, Slot Machine moves up.
4. **Hackathon-level demo need.** If a demo audience needs the Spreadsheet's visceral "we ran 16 sweeps" moment, prioritize that over user feedback. Note: don't confuse demo demand for product demand.

Until one of those triggers fires, the cost-to-build doesn't justify the speculation. Ship v1, listen to the cycle, then come back to this list.

---

## What stays deferred indefinitely

- **Mobile-responsive layouts** — desktop-only is intentional. Strategy authoring isn't a mobile activity.
- **Multi-user / collaboration** — single-user localhost stays the model.
- **Light theme** — pinned to the theme pilot, which is also deferred per `themes.md`. If we ever pick it up, it lands as one ticket alongside the pilot, not as part of the deferred archetypes line.
- **Real-time collaborative wizard** — the wizard is between you and the LLM; introducing a second human breaks the cognitive model.

---

## Tracking

When work on any of these starts:
1. Move the dossier into a real plan file `docs/superpowers/plans/YYYY-MM-DD-<archetype>-plan.md`.
2. Strike through the row in this doc with a date + plan-link reference.
3. The roadmap-level sequencing in §1 becomes the plan's `> Sequencing:` line.

This file stays as the single source of truth for "what's still deferred."
