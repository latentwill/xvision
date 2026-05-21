# List Surfaces Audit — 2026-05-21

**Source intake:** `team/intake/2026-05-21-docs-lists-metric-polish.md` (track #2)
**Contract:** `team/contracts/list-search-filter-completion-audit.md`
**Author:** conductor (Claude, 2026-05-21)
**Consumed by:** `team/contracts/list-search-filter-missing-surfaces.md` (flips to `ready` when this lands)

## Why this audit exists

The operator's expectation (verbatim): *"add filters/search to all lists."*
The phase-2 list migration shipped the four high-traffic routes
(`/eval-runs`, `/strategies`, `/scenarios`, `/agents`) onto
`<ResponsiveListCard>` + `useListState`. The intake calls for stronger
coverage — every list-like surface, including tail / secondary lists
that were easy to miss when phase 2 was scoped. Before opening
migration PRs we need a single inventory so workers don't duplicate
effort and so non-list surfaces are explicitly carved out.

This document is the checklist the next migration wave reads.

## Surfaces Inventory

Methodology: grep for `.map(` / `<table>` / `<Card>` repetition across
`frontend/web/src/routes/`, `frontend/web/src/features/`, and
`frontend/web/src/components/chat/cards/`. Sub-agent did the sweep;
findings cross-checked against the phase-1 components at
`frontend/web/src/components/lists/`.

| # | Path | Contents | Current primitive | Search | Filters | Sort | Mobile | Migration decision |
|---|---|---|---|---|---|---|---|---|
| 1 | `/eval-runs` (`routes/eval-runs.tsx`) | Eval run summaries (backtest/paper) | `ResponsiveListCard` + `useListState` | yes | mode, status | started, completed, strategy, status | yes | **Already-migrated** — phase 2a (PR #399) |
| 2 | `/strategies` (`routes/strategies.tsx`) | Strategy bundles in workspace | `ResponsiveListCard` + `useListState` | yes | shape (single/multi-agent) | added, name A→Z | yes | **Already-migrated** — phase 2b (PR #400) |
| 3 | `/scenarios` (`routes/scenarios.tsx`) | Test scenarios (canonical/user/clone/generated) | `ResponsiveListCard` + `useListState` | yes | source, archived | added, name | yes | **Already-migrated** — phase 2c (PR #403) |
| 4 | `/agents` (`routes/agents.tsx`) | Agent library (reusable templates) | `ResponsiveListCard` + `useListState` | yes | shape (single/multi-slot), archived | updated, name | yes | **Already-migrated** — phase 2c (PR #403) |
| 5 | `/eval-runs/compare` (`routes/eval-compare.tsx`) | Comparison metrics table (equity curves side-by-side) | `<table>` + sort dropdown | no (justified — see right) | no | call order (default) + gross / net / sharpe / max-dd / decisions | partial (chart only) | **Migrated (sort-only)** — `list-search-filter-missing-surfaces` slice 3 (PR opened 2026-05-21). Search/filter skipped: page typically shows 2–10 rows all visible at once, so substring match has no real win. Palette dots derive from the ORIGINAL `runs` index so table-row colour stays in sync with the equity chart legend after sort. |
| 6 | `/settings/providers` (`routes/settings/providers.tsx`) | LLM provider credentials (OpenAI, Anthropic, OpenRouter, DeepSeek, Ollama, …) | `ResponsiveListCard` + `useListState` | yes (name + kind) | kind | name A→Z (default), kind then name | yes | **Migrated** — `list-search-filter-missing-surfaces` slice 2 (PR opened 2026-05-21). Add/edit/test chrome stays inline above the list; the row component `ProviderRowView` is unchanged. |
| 7 | `/settings/skills` (`routes/settings/skills.tsx`) | Skill registry (tools, prompt fragments, evaluators) | `ResponsiveListCard` + `useListState` | yes (name + description) | kind | added (updated_at DESC default), name A→Z | partial (mobile row click → inline edit) | **Migrated** — `list-search-filter-missing-surfaces` slice 1 (PR opened 2026-05-21). The original audit row said `/agents/skills`; correct path is `/settings/skills`. |
| 8 | Scenario detail (`routes/scenarios-detail.tsx`) — Runs tab | Eval runs scoped to this scenario | `ResponsiveListCard` + `useListState` | yes (run id + strategy name) | mode, status | completed-desc (default), started-desc, strategy A→Z | yes | **Migrated** — `list-search-filter-missing-surfaces` slice 4 (PR opened 2026-05-21). The audit's original framing was "cycle results"; the real surface is the Runs tab listing every eval run against the scenario. URL state at `useListUrlState("scenario-runs", …)`. |
| 9 | `/eval-runs/:id` Decisions panel (`routes/eval-runs-detail.tsx::DecisionsPanel`) | Trade decision ledger per run | raw `<table>` + filter buttons | partial (by action kind) | decision kind only | no | yes | **Not-a-list** — in-context inspector sub-table. Decisions belong to the run; the table is a detail view, not a browse surface. Filter-by-action via buttons is the right ergonomic; full search would need schema changes (search by asset / fill price). If revisited, treat as a design spec, not a migration. |
| 10 | `/agent-runs/:id` Timeline (`features/agent-runs/AgentRunIndentedTimeline.tsx`) | Agent-run span hierarchy (indented trace tree) | bespoke `.map()` → span divs + FilterBar (kind, tag) | no | filter bar (kind, tag) | no | yes | **Not-a-list** — hierarchical trace visualization; filtering is done by FilterBar at the right granularity. Forcing it into `<ResponsiveListCard>` would lose the tree shape. |
| 11 | `/home` Recent runs card (`routes/home.tsx`) | Last 5 eval runs (mini-list widget) | `<Card>` + `.slice(0, 5).map` | no | no | no | yes | **Not-a-list** — fixed-size control-tower mini-card. Operator clicks through to `/eval-runs` for the full list. |
| 12 | `/home` Attention card | High-priority alerts (missing providers/brokers/stale scenarios) | `<Card>` + `buildAttention().map` | no | no | no | yes | **Not-a-list** — alert summaries, not a searchable list. |
| 13 | `/home` Count cards | Strategies / agents / providers counters | `<Card>` + `<Link>` | n/a | n/a | n/a | yes | **Not-a-list** — single-value KPI cards. |
| 14 | `/docs` (`routes/docs/index.tsx`) | Documentation page index | bespoke nav + grouped sections + client-side fuzzy filter | yes (substring) | no | no | yes | **Not-a-list** (with caveat) — covered by `docs-search-list-component-adoption` (deferred follow-up to `docs-ui-prototype-alignment`). If that contract activates, this row flips to "Already-migrated". |
| 15 | `/strategies-folder` (`routes/strategies-folder*`) | Uploaded strategy files by subfolder | `<Card>` + `.map` grouped by folder (notes/docs/strategy-files/evals/library) | no | no | no | yes | **Not-a-list** — file browser surface with grouping by subfolder + inline import status. Forcing it into the row-shaped list component would lose the folder grouping. Revisit if the V2F strategies-folder intake adopts a flat-table presentation. |
| 16 | `/settings/brokers` Alpaca | Single broker credential form | `<Card>` + edit-toggle | n/a | n/a | n/a | yes | **Not-a-list** — single entity form. |
| 17 | `/settings/brokers` Orderly | Single broker credential form | `<Card>` + edit-toggle | n/a | n/a | n/a | yes | **Not-a-list** — single entity form. |
| 18 | `/settings/general` | General settings (checkboxes, toggles) | form fields + `.map` for options | no | no | no | yes | **Not-a-list** — form fields. |
| 19 | `/settings/danger` | Workspace reset / destructive actions | `<Card>` + action buttons | n/a | n/a | n/a | yes | **Not-a-list** — control panel. |
| 20 | Chat rail RunListCard (`components/chat/cards/ChatRunListCard.tsx`) | Recent runs (hardcoded slice 0–5) | bespoke article + `.map` 5 rows | no | no | no | yes | **Not-a-list** — chat-context affordance with a deliberate small-cap limit. |
| 21 | Chat rail StrategyCard | Strategy selector (abbreviated) | bespoke `<Card>` + `.map` | no | no | no | yes | **Not-a-list** — inline chat card. |
| 22 | Chat rail ScenarioCard | Scenario selector (abbreviated) | bespoke `<Card>` + `.map` | no | no | no | yes | **Not-a-list** — inline chat card. |
| 23 | `/authoring` Agent slots panel | Attached agents in strategy (composition grid) | bespoke grid + `.map` with reorder | partial (pool selection) | no | no | yes | **Not-a-list** — composition editor, not a browse surface. |
| 24 | Mobile eval-runs-detail (`routes/eval-runs-detail-mobile.tsx`) | Mobile-optimized decisions view | wraps the desktop decisions panel | partial | partial | no | yes | **Not-a-list** — mobile-only inspector detail. Inherits whatever the decisions panel does. |

**Totals: 24 surfaces · 4 already-migrated · 3 migrate · 17 not-a-list (with reasons)**

## Recommendation: how `list-search-filter-missing-surfaces` should batch the work

### Small PR (`providers + skills`) — ~200-300 LOC

Group rows #6 and #7. Both are settings surfaces with the same shape
(name / kind / inline action). Establishes the migration pattern for
non-route lists. Suggested acceptance:

- `/settings/providers` mounts `<ResponsiveListCard listId="settings-providers">`. Search by provider name + kind. No filter (operators want every provider listed). Default sort: name A→Z.
- `/agents/skills` mounts `<ResponsiveListCard listId="agents-skills">`. Search by skill name. Filter chip: `kind ∈ {tool, prompt, evaluator}`. Default sort: most-recently-added.
- Mobile: both use `<MListRow>` with title=name, sub=kind, right=action button.
- Both should respect the no-popups rule for any inline edit (use accordion / route to detail, not modal).

### Medium PR (`eval-compare + scenarios-detail`) — ~300-500 LOC

Rows #5 and #8. Both are tables nested inside a detail route; they're
edge cases where "list vs sub-table" is judgement. Decisions:

- `/eval-runs/compare` — the metrics table IS the page body, not a sub-section. Migrate to `<ResponsiveListCard>` with one row per run. Search by strategy display name. Filter: include/exclude by metric (e.g. only show runs with non-null Sharpe). Default sort: comparison index (preserves call order). Sort options: return %, Sharpe, max DD, decisions count.
- `/scenarios/:id` test-results table — gate on cycle count. If the engine pagination claims every cycle gets a row, migrate. If most scenarios have ≤ 10 cycles, leave it bespoke and document. Worker decides at sync-before-work.

### Deferred until a design decision lands

- **Decisions panel** (row #9) — Filter-by-action buttons are the right ergonomic for in-context inspector use. A migration to `<ResponsiveListCard>` would lose the filter-button strip and force a flat table where the operator currently sees decision context. Revisit only if the operator pushes back on the current shape.
- **Agent-runs timeline** (row #10) — tree-shaped. Not a candidate for the row-list pattern. The existing FilterBar is the right primitive.
- **Chat-rail cards** (rows #20-22) — by design these are abbreviated mini-cards with a hardcoded 5-item cap. Not full lists. If operator wants chat-rail-to-full-list nav, that's a routing change not a component swap.

## Out of scope (per intake)

- Creating a second list component.
- Refactoring the four phase-2 routes.
- Designing new metrics or new filter dimensions on existing lists.
- Migrating docs navigation (covered by `docs-search-list-component-adoption`).
- Migrating strategies-folder (V2F seed; needs spec).

## Followup tasks identified during the audit

These are NOT migration candidates but emerged from the sweep — file
as separate intake if/when they become operator-visible:

1. `/eval-runs/compare` doesn't currently expose a "remove this run from the comparison" affordance. If the medium-PR migration above lands, that's a natural place to add it.
2. The `<table>` element used in `/eval-runs/compare` doesn't have accessible column-sort affordances even when the data would support it. Migration to `<ResponsiveListCard>` would inherit phase-1's sort UX.
3. `/settings/providers` currently shows test-button results inline as text; if the migration lands, a `<Pill tone="ok | err">` would match the dashboard's status-pill language.

These aren't blocking; they're nice-to-haves for the migrating PR to consider.
