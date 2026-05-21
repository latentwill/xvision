---
track: list-migrate-strategies
contract: team/contracts/list-migrate-strategies.md
status: ready-for-review
owner: claude-opus-4-7
claimed_at: 2026-05-21
worktree: .worktrees/list-migrate-strategies
branch: task/list-migrate-strategies
---

# Status

## 2026-05-21 — implementation complete

Lifted the 2a (`/eval-runs`) migration pattern verbatim onto
`frontend/web/src/routes/strategies.tsx`.

### Wiring

- Replaced the bespoke `<Card>` + `<StrategiesTable>` + `<FilterBar>`
  block with a single `<ResponsiveListCard listId="strategies">`.
- `useListState<StrategyListItem>` drives search + filters + sort:
  - **search**: matches `display_name` (case-insensitive), the first
    12 chars of `agent_id` (ULID prefix), and the full ULID.
  - **filters**:
    - `shape` — "All shapes" / "Trader-only (single agent)" /
      "Multi-agent". Derived from the runtime composition exposed on
      `StrategyListItem` — `provider_models.length` (preferred) or the
      legacy parallel `providers[]` / `models[]` arrays. `> 1` agent ⇒
      multi; otherwise single. (`StrategyListItem` does not expose
      `agents[]` directly — those live on the full `Strategy` shape,
      not the list envelope. The provider/model count is the available
      proxy and matches the contract's "trader-only vs multi-agent"
      intent.)
    - `template` — dynamic, sourced from the templates observed on
      the current page (`"all"` default + alphabetic).
  - **sort**: "Recently added" (default — backend already returns ULID
    DESC per `engine::api::strategy::list_paged`, so the default key
    is a no-op transform) and "Name A → Z".
- `useListUrlState("strategies", list)` round-trips `?q=…&shape=…&template=…&sort=…`
  via `react-router-dom`'s `useSearchParams`. Verified by the new
  `hydrates the search term from the ?q= URL parameter` test.

### Mobile shape

`renderMobileRow` returns `<MListRow>`:

- title = `display_name || "Untitled strategy"`
- badge = `"multi-agent"` (info) or `"trader-only"` (muted)
- subtitle = `${template} · ${formatCadence(decision_cadence_minutes)}`
- meta = model summary (`row.model` → `provider_models` → `models[]` → `"—"`)
- rightTop = `"N agents"` when N > 0
- rightSub = `"draft"`

### Pagination carve-out (same as 2a)

`<ListPagination>` JSX renders **outside** the card — `<MListCard>`
has no footer slot, so splitting the pagination control across
breakpoints is worse UX than the consistency. The unified-pagination
shape will be settled in 2c; this matches the 2a status note.

`useServerPagination` keeps owning offset/limit; the
`{ items, total }` envelope from #397 stays canonical.

### Test updates

- Added the `stubMatchMediaDesktop()` helper (copied from
  `eval-runs.test.tsx`) in `beforeEach` so `useViewportMode()` resolves
  to the desktop branch under jsdom.
- Re-wrote `renderRoute` to take an `initialEntry` for URL-state tests.
- First test now waits for actual row text (`Trend 4H`) instead of the
  toolbar `"Name"` label — the toolbar always renders synchronously so
  the original `findAllByText("Name")` no longer waited for the row to
  land.
- Added three new tests:
  - Empty state renders `"No strategies match these filters."` plus a
    `/strategies/new` CTA.
  - Pipeline-shape filter narrows the rows from 2 → 1 when set to
    `"multi"`.
  - `?q=alpha` hydrates the search input on mount and filters rows.

### Verification

```
$ pnpm --dir frontend/web typecheck
> tsc -b
(passes silently)

$ pnpm --dir frontend/web test -- routes/strategies --run
Test Files  2 passed (2)
     Tests  8 passed (8)

$ pnpm --dir frontend/web test
Test Files  4 failed | 72 passed (76)
     Tests  5 failed | 622 passed (627)
```

The 5 failures match the documented pre-existing set on `origin/main`:

- `agent-runs-detail.test.tsx > inspector selection fallback`
- `MarkdownView.test.tsx > does not render raw HTML`
- `TraceDock.test.tsx > inspector selection fallback`
- two `InlineEditField.test.tsx` strategy-detail tests

No new failures introduced.

`pnpm --dir frontend/web lint` is not a defined script in this
workspace — skipped. Contract-listed but not actionable.
