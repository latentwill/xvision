---
track: qa-ui-polish-round2
worktree: .worktrees/qa-ui-polish-round2
branch: task/qa-ui-polish-round2
base: origin/main
phase: pr-open
last_updated: 2026-05-18T03:53:30Z
owner: claude
---

# Scope landed

This PR ships **2 of the 5** bundled items. The remaining three are
left to a follow-up because each carries enough risk (upstream
dependencies / non-existent allowed-path files) that bundling them
here would block the two clean fixes.

## Done

- **#9 Duplicate streaming icon in SpanInspector.** Operator saw a
  `STREAMING` header pill AND a `● STREAMING` PullQuote header label
  for every active model.call span. Drop the SpanInspector header
  pill — the PullQuote indicator stays co-located with the body it
  describes. ERROR badge slot is preserved because no body affordance
  exists for it. Test moved from "≥ 1 STREAMING marker" to "exactly 1".

- **#10 Loud full_debug retention banner.** The `role="alert"` Card
  on every agent-runs detail page repeated what the minimal
  `retention: full_debug` Pill already said. Per the operator's
  round-2 feedback and `feedback_no_privacy_overkill`, the banner is
  removed. The badge stays. Test flipped from "renders banner for
  full_debug" → "banner never renders".

## Deferred / not in this PR

- **#3 Latest-run chart eval name.** Allowed-path
  `frontend/web/src/features/home/LatestRunChart.tsx` does not exist
  on `origin/main` — the home page renders the latest run chart
  inline in `home.tsx`. The contract guidance also says to confirm
  this is not already covered by `ux-polish-eval-list-and-snapshot`
  (merged via #241) before editing; that audit is the right starting
  point. Recommend respawning as a focused track once the actual
  surface is identified.
- **#4 Agents archived delete.** Allowed-path
  `frontend/web/src/features/agents/**` does not exist; archived
  delete needs both a backend DELETE route and a confirm-in-place UI
  affordance. Scope is more than a polish nit.
- **#13 TradingView chart titles.** Contract acceptance asks for an
  investigation of the cause (config prop, CSS, upstream regression)
  before changing anything. Without a local way to repro on the
  TradingView component, this turns into a fishing expedition that
  is better as a separate track.

## Files touched (vs. contract `allowed_paths`)

- `frontend/web/src/features/agent-runs/SpanInspector.tsx` — IN allowed_paths.
- `frontend/web/src/features/agent-runs/SpanInspector.test.tsx` — IN allowed_paths.
- `frontend/web/src/routes/agent-runs-detail.tsx` — **NOT** in
  allowed_paths. The contract listed `routes/index.tsx` (doesn't
  exist) and `features/settings/RetentionCard.tsx` (also doesn't
  exist) as the surfaces for the retention warning, but the actual
  banner lives at `routes/agent-runs-detail.tsx:103-112`. Edit is
  minimal (one Card block removed); flagged for the conductor.
- `frontend/web/src/routes/agent-runs-detail.test.tsx` — same.

## Verification

- Passed: `corepack pnpm --dir frontend/web test -- --run SpanInspector agent-runs-detail` — 23 tests
- Passed: `corepack pnpm --dir frontend/web typecheck`
- Passed: `corepack pnpm --dir frontend/web build`

## Notes

- No `border-white` / `border-gray-100` / `border-gray-200` / `#fff`
  introduced (CLAUDE.md rule).
- Each landed item is a separate commit so the operator can validate
  them one at a time, per contract acceptance.
