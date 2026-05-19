# status — decision-side-label-sell-vs-short

Last updated: 2026-05-19, worker session start.

## Claim

- Track contract: `team/contracts/decision-side-label-sell-vs-short.md`
- Branch: `task/decision-side-label-sell-vs-short`
- Worktree: `.worktrees/decision-side-label-sell-vs-short`
- Base: `origin/main` @ `c8a812b`
- Wave: QA Operator Round 4 (intake
  `team/intake/2026-05-19-qa-operator-round-4.md`, finding #4)

## Verified non-claimed before grabbing

- `team/contracts/decision-side-label-sell-vs-short.md` did not exist on
  `origin/main`, `origin/conductor/2026-05-18-sweep`, or any local branch.
- `git branch --contains` search across the workspace returned no prior
  branch named `task/decision-side-label-sell-vs-short`.
- `worktree-qa22-inspector-polish` exists but is at `origin/main` with
  no extra commits.

## Plan

1. Read the existing `decisionKind` / `decisionActionLabel` /
   `DecisionFilter` machinery in `eval-runs-detail.tsx` and the
   `derivePositionsByDecision` helper in `features/decisions/positions.ts`.
2. Add `derivePriorSideByDecision(rows): Map<decision_index, "long" | "short" | "flat">`
   in `features/decisions/positions.ts` — walks the same ordered
   sequence as the existing helper but snapshots the per-row asset's
   prior side BEFORE the action applies. Unit-test it alongside the
   existing positions tests.
3. Widen `DecisionFilter` to `all | buy | short | sell | cover | hold`.
   Re-key `decisionKind` to a `(action, priorSide)` function. Re-key
   `decisionActionLabel` to the five-label set. Update filter tabs +
   counts to use the position-aware kind.
4. Thread `priorSideByDecision` (or the resolved kind) through to the
   `DecisionSignal` render so each row's pill resolves correctly.
5. Mirror the mapping in `eval-runs-detail-mobile.tsx`'s `actionLabel`
   and widen `ActionPill`'s typed `action` union.
6. Update tests in both detail-route test files. Add at least one
   `short_open → SHORT` assertion and one `flat-after-short → COVER`
   assertion.
7. Run `pnpm tsc --noEmit`, the scoped vitest suites, and `pnpm build`.

## Open questions / risks

- The `flat`-from-`flat` defensive case lands on `HOLD`. If a future
  contract introduces a "Cancel" or "Reject" action that decays to a
  `flat`-from-`flat` row, the catch-all is still safe (no false SELL).
- The filter bucket rename ("Close" → "Sell"/"Cover") changes a piece of
  visible UI copy; reviewer should sign off on the tab labels matching
  the intake's five-label spec.
