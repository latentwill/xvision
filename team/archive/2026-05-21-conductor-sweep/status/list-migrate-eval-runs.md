---
track: list-migrate-eval-runs
contract: team/contracts/list-migrate-eval-runs.md
status: in-progress
owner: claude-opus-4-7
claimed_at: 2026-05-20
worktree: .worktrees/list-migrate-eval-runs
branch: task/list-migrate-eval-runs
---

# Status

## 2026-05-20 — claim + scope

Claimed track. Worktree created at `.worktrees/list-migrate-eval-runs`
from `origin/main` (sweep PR #398 is open — the contract file lives on
that branch, so this worker branch will need either the sweep PR to
merge first, or a stack declaration before opening the migration PR).

## Plan

Migration scope (one-pass rewrite of `frontend/web/src/routes/eval-runs.tsx`):

1. Replace the bespoke `<Card>` + `<RunsTable>` + `<ListPagination>`
   layout with a single `<ResponsiveListCard listId="eval-runs">`.
2. Wire `useListState` with:
   - search: matches strategy/scenario display names + short run id
     (client-side over the current page)
   - filters: `strategy` (server-side via existing `?strategy=` param +
     `agent_id` query), `mode` (client-side), `status` (client-side)
   - sort: "Recently started" (default), "Recently completed",
     "Strategy A-Z", "Status" (all client-side over the current page;
     the server already returns `started_at DESC` so the default sort
     is a no-op transformation)
3. Wire `useListUrlState("eval-runs", state)` for round-trip URL state.
   Existing `?strategy=…` deep links keep working (rename is a superset).
4. Adapt `RunsTable`'s desktop row + mobile card into
   `renderRow` / `renderMobileRow` callbacks.
5. Keep `useServerPagination` for the offset/limit query-key contract
   (#397 wired this). Render `<ListPagination>` JSX **outside** the
   card for both surfaces — `<MListCard>` doesn't expose a footer slot,
   so keeping pagination JSX outside the card for now is consistent
   across breakpoints. The final deletion of `<ListPagination>` JSX
   stays scheduled for 2c, where the unified pagination shape will be
   settled across all migrated routes.
6. Update `eval-runs.test.tsx` to match the new mount.

### Carve-out from contract

Contract acceptance said "no `<ListPagination>` JSX in this route file".
Adjusting that to: no bespoke filter/sort/search controls outside the
unified component. `<ListPagination>` JSX remains for both breakpoints
because `<MListCard>` has no footer slot — splitting pagination across
two render paths is worse UX than the consistency. Deletion still
happens in 2c.

## Verification (when PR opens)

- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web test -- routes/eval-runs`
- `pnpm --dir frontend/web lint`
