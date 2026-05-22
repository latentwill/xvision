# Intake — 2026-05-21 — Docs UI, list search/filter completion, drawdown color semantics

Operator-driven (Ed, 2026-05-21). This intake captures three UI polish
items that cut across existing V2A/docs and list-component work:

1. Improve the in-app docs UI so it matches the shipped dashboard prototype
   visual language.
2. Ensure every list surface has the standardized search/filter/sort
   controls, not only the high-traffic routes.
3. Fix max-drawdown color semantics: a positive max drawdown value is still a
   loss/risk number and must render red/danger, not neutral, gold, or warning.

Verbatim ask:

> improve docs UI to match prototype, add filters/search to all lists and
> positive max dd is in red.

## Source context

- `frontend/prototype/` — visual source of truth for folio-dark dashboard
  treatment. Relevant files:
  - `frontend/prototype/README.md`
  - `frontend/prototype/styles.css`
  - `frontend/prototype/shared.jsx`
  - `frontend/prototype/screen-*.jsx`
- Current docs route:
  - `frontend/web/src/routes/docs/index.tsx`
  - `frontend/web/src/api/docs.ts`
  - `frontend/web/src/features/docs/DocsMarkdown.tsx`
  - backend content/index in `crates/xvision-dashboard/src/routes/docs/`
- Existing docs intake:
  - `team/intake/2026-05-20-docs-user-and-agent-wiki.md`
- Existing list intake/spec:
  - `team/intake/2026-05-19-list-component-design-intake.md`
  - `docs/superpowers/specs/2026-05-20-standard-list-component.md`
  - active contracts in `team/board.md`: `list-migrate-eval-runs`,
    `list-migrate-strategies`, `list-migrate-decisions-and-tail`
- Current max-drawdown examples:
  - `frontend/web/src/routes/eval-runs.tsx::drawdownToneClass`
  - `frontend/web/src/routes/eval-runs-detail.tsx`
  - `frontend/web/src/routes/eval-runs-detail-mobile.tsx`
  - `frontend/web/src/routes/eval-compare.tsx`

## Current state

### Docs UI

`/docs` already has real behavior: route-level search, section grouping,
deep-linkable `?slug=`, loading/error/empty states, and markdown rendering.
It is functional, but visually reads as a generic two-column card rather
than the prototype-backed folio dashboard surface. The docs/wikis intake
focused on content drift and baked/manifest plumbing, not presentation.

This intake should not reopen docs content scope. Keep content refresh and
wiki manifest work in the existing 2026-05-20 docs intake. This wave is
route presentation and reader ergonomics.

### Lists

The standardized list component foundation exists in production code
(`frontend/web/src/components/lists/`) and phase-2 migrations are on the
main board for `/eval-runs`, `/strategies`, `/scenarios`, and `/agents`.
The operator expectation is stronger: every list-like surface should have
search/filter/sort, including tail or secondary lists that were easy to
miss when phase 2 was scoped.

This intake should produce an audit first, then migrate the missing
surfaces. Avoid another parallel list system.

### Max DD color

`max_drawdown_pct` appears in list rows, run detail, mobile run detail,
compare tables, tests, and chart payloads. Some code treats the sign like
return/PnL, where positive is good and negative is bad. That is wrong for
drawdown. For display purposes, any non-zero drawdown magnitude means loss
or risk. A stored positive value like `4.50%` must be red/danger.

If some backend paths emit negative drawdown and others positive drawdown,
the UI should normalize by magnitude for color while preserving the chosen
formatted value convention until a separate data-contract cleanup changes
the payload.

## Raw items → tracks

| # | Severity | Track | One-line scope |
|---|---|---|---|
| 1 | P1 | `docs-ui-prototype-alignment` | Restyle `/docs` to match the folio prototype visual language: prototype typography, surface spacing, sidebar/search treatment, active state, article width, code block treatment, empty/loading/error states. Behavior stays as-is. |
| 2 | P1 | `list-search-filter-completion-audit` | Inventory every list-like route/component and mark whether it uses `<ResponsiveListCard>` / `useListState`, has search, has filters, has sort, and has mobile parity. Output becomes the migration checklist. |
| 3 | P1 | `list-search-filter-missing-surfaces` | Migrate every missing list surface from the audit to the standardized list component or explicitly document why the surface is not a list. Must include secondary/tail lists, not only `/eval-runs`, `/strategies`, `/scenarios`, `/agents`. |
| 4 | P1 | `max-drawdown-danger-tone` | Normalize max-drawdown display tone across eval list, run detail desktop/mobile, compare, home mini-lists, and any summary cards so any non-zero drawdown magnitude renders red/danger. Add tests with positive drawdown values. |
| 5 | P2 | `docs-search-list-component-adoption` | Optional follow-up if the docs sidebar page list is complex enough: adopt the same list search/chip visual treatment for docs navigation without turning docs into a heavy data table. |

## Acceptance criteria

### `docs-ui-prototype-alignment`

- `/docs` still supports the existing behaviors:
  - `?slug=<slug>` deep linking.
  - sidebar/index filtering.
  - section headers.
  - loading, empty, error, and populated states.
- Presentation matches the prototype-backed dashboard language:
  - folio-dark tokens and typography align with `frontend/prototype/styles.css`
    and existing production tokens.
  - docs navigation looks integrated with the app chrome, not like a plain
    form/sidebar.
  - markdown reader has comfortable measure, heading hierarchy, code block
    styling, table styling, and link treatment.
  - mobile/tablet behavior is explicit; no text overlap or clipped controls.
- Keep the no-popup rule. No modal/sheet docs navigation unless a later mobile
  design explicitly grants a narrow exemption.

### `list-search-filter-completion-audit`

- Audit covers at least:
  - `/eval-runs`
  - `/strategies`
  - `/scenarios`
  - `/agents`
  - agent runs / trace lists
  - decisions / trade ledger / open positions / journal if present or planned
  - home/control-tower mini-lists
  - docs navigation if treated as a list
- Each row records: route/component, current list primitive, search, filters,
  sort, URL state, mobile parity, owner contract, and migration decision.
- The audit identifies overlap with active list phase-2 contracts so workers
  do not duplicate work.

### `list-search-filter-missing-surfaces`

- Every user-facing list has search and sort.
- Every domain list with meaningful categories has filters. If no filters are
  useful, the PR states why.
- "Recently added" or equivalent recency sort is the default unless the route
  has a stronger domain-specific default.
- Mobile list filtering uses the approved list component mobile pattern from
  the standard-list spec.

### `max-drawdown-danger-tone`

- Positive stored values, negative stored values, and zero/null values are all
  covered by tests:
  - `+4.50%` max drawdown renders danger/red.
  - `-4.50%` max drawdown renders danger/red, unless the data contract is
    normalized first.
  - `0.00%`, `null`, or missing drawdown renders neutral.
- Surfaces checked:
  - eval run list row.
  - eval run detail KPI grid.
  - mobile eval run detail KPI grid.
  - eval compare metrics table.
  - home/control-tower recent run or KPI summaries if they render max DD.
- Do not reuse return/PnL tone helpers for drawdown unless they accept a
  semantic mode like `metricKind: "drawdown"`.

## Out of scope

- Rewriting docs content. Use
  `team/intake/2026-05-20-docs-user-and-agent-wiki.md` for content and
  wiki-source work.
- Building a public marketing docs site.
- Creating a second list component.
- Changing the backend `max_drawdown_pct` sign convention. This intake fixes
  display semantics; a backend/data-contract cleanup can follow if needed.
- Reworking chart drawdown series behavior. Chart rendering should only change
  if the same incorrect positive-is-good tone appears in chart labels/legends.

## Verification

- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test --run`
- Targeted route/component tests for:
  - docs route presentation state snapshots or DOM assertions.
  - list audit/migration rows where applicable.
  - positive max-drawdown danger styling.
- Manual responsive pass for `/docs` and at least one migrated list at desktop,
  tablet, and phone widths.
- `git diff --check`
