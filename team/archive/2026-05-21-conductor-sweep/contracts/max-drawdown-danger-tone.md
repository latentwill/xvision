---
track: max-drawdown-danger-tone
lane: leaf
wave: docs-lists-metric-polish-2026-05-21
worktree: .worktrees/max-drawdown-danger-tone
branch: task/max-drawdown-danger-tone
base: origin/main
status: merged
depends_on: []
blocks: []
stacking: none
allowed_paths:
  - frontend/web/src/routes/eval-runs.tsx                                # drawdownToneClass + list-row use
  - frontend/web/src/routes/eval-runs.test.tsx                           # positive-DD danger styling assertion
  - frontend/web/src/routes/eval-runs-detail.tsx                         # KPI grid Max DD metric
  - frontend/web/src/routes/eval-runs-detail.test.tsx                    # if it exists; add a DD tone test
  - frontend/web/src/routes/eval-runs-detail-mobile.tsx                  # mobile KPI tile
  - frontend/web/src/routes/eval-compare.tsx                             # compare metrics table DD cell
  - frontend/web/src/routes/eval-compare.test.tsx                        # if it exists; DD tone assertion
  - frontend/web/src/routes/home.tsx                                     # only if home/control-tower renders max DD
  - frontend/web/src/lib/metric-tone.ts                                  # NEW (if extracted) — shared helper module
  - frontend/web/src/lib/metric-tone.test.ts                             # NEW
forbidden_paths:
  - frontend/web/src/components/chart/**                                 # chart drawdown series rendering is out-of-scope per intake
  - frontend/web/src/theme/themes.ts                                     # do not introduce a new "drawdown" token here; use existing text-danger
  - crates/**                                                            # do not change backend sign convention; this is display-only
  - frontend/web/src/api/types.gen/**                                    # generated; do not hand-edit
interfaces_used:
  - RunSummary                                                           # frontend/web/src/api/types.gen/RunSummary.ts (max_drawdown_pct: number | null)
  - MetricsSummary                                                       # frontend/web/src/api/types.gen/MetricsSummary.ts (max_drawdown_pct: number)
  - fmtPct                                                               # frontend/web/src/lib/format.ts (or wherever it lives)
parallel_safe: true                                                      # no shared writers
parallel_conflicts: []
verification:
  - pnpm --dir frontend/web typecheck
  - pnpm --dir frontend/web test -- routes/eval-runs
  - pnpm --dir frontend/web test -- routes/eval-runs-detail
  - pnpm --dir frontend/web test -- routes/eval-compare
  - pnpm --dir frontend/web lint
acceptance:
  - **Helper rewritten to magnitude-only.** `drawdownToneClass` at `frontend/web/src/routes/eval-runs.tsx:1069` (current shape: `text-warn` for magnitude < 10, `text-danger` for ≥ 10) is replaced by a magnitude-only rule: any non-zero non-null value returns `text-danger`; null and exactly zero return the neutral class (`text-text`).
  - **Helper moved to a shared module.** Extract `drawdownToneClass` from `eval-runs.tsx` into a small shared file (e.g. `frontend/web/src/lib/metric-tone.ts`) so all surfaces import the same function instead of inlining sign-checks. The companion test (`metric-tone.test.ts`) asserts: `+4.5 → text-danger`, `-4.5 → text-danger`, `0 → text-text`, `null → text-text`, `+0.001 → text-danger`. Don't extract sibling tone helpers — only drawdown.
  - **List row uses the shared helper.** `eval-runs.tsx:613` no longer reads from a local function. Visual: a +4.5% max DD row renders red, not gold/warn.
  - **Run detail KPI grid uses the shared helper.** `eval-runs-detail.tsx:605` wraps its Max DD `<Metric>` in the danger tone when the value is non-zero. Today the metric ships without tone wiring; add it.
  - **Mobile run detail uses the shared helper.** `eval-runs-detail-mobile.tsx:354` currently has a buggy `summary.max_drawdown_pct < 0 ? "neg" : undefined` check — for a positive-stored DD this returns `undefined`, leaving the value neutral. Replace with the shared helper so non-zero magnitude lands on danger tone regardless of sign.
  - **Compare table uses the shared helper.** `eval-compare.tsx:213` renders Max DD via `<MetricCell value={fmtPct(r.metrics?.max_drawdown_pct)} />` with no tone parameter. Add a `tone` (or wrapping class) that uses the shared helper.
  - **Home/control-tower check.** Audit `frontend/web/src/routes/home.tsx` (and any home mini-list helpers) for a Max DD cell. If one exists, apply the helper. If not, this PR notes the absence in `Notes:` and moves on.
  - **Tests cover all three sign cases.** At least one component test asserts a positive stored max DD renders with the danger class. The shared helper unit test covers positive, negative, zero, and null inputs.
  - **No new theme tokens.** This contract uses the existing `text-danger` token. Do not add new theme variants in `frontend/web/src/theme/themes.ts`.
  - **Charts untouched.** `frontend/web/src/components/chart/**` is in `forbidden_paths` — chart drawdown series rendering is out-of-scope per intake §"Out of scope".

---

# Scope

Track #4 of `team/intake/2026-05-21-docs-lists-metric-polish.md`. The
helper `drawdownToneClass` and several inline sign-checks at max-DD
render sites currently treat drawdown like a return/PnL metric — sign
determines tone. That's wrong: drawdown is by definition a loss
metric, and any non-zero magnitude is bad news. The intake's verbatim:

> positive max dd is in red

Replace the sign-aware helper with a magnitude-only helper, extract
it to a shared module, and apply it on every surface that renders
`max_drawdown_pct`: eval-runs list row, run-detail KPI grid (desktop
+ mobile), eval-compare metrics table, and any home/control-tower
summary if one exists. Add tests so the regression doesn't recur.

This is display semantics only. The backend sign convention is
intentionally untouched (intake §"Out of scope": "changing the backend
`max_drawdown_pct` sign convention" is deferred).

# Out of scope

- Chart series rendering (`frontend/web/src/components/chart/**`).
  Chart drawdown already renders red via theme tokens; not this track.
- New theme tokens. Use existing `text-danger`.
- Backend payload normalization (intake §"Out of scope").
- Refactoring `returnToneClass` or other sign-aware helpers. Only
  drawdown is wrong here.
- The deeper `metricKind: "drawdown"` API the intake mentions on the
  PnL tone helper. If the shared helper exposes a one-mode entry point
  for now, that's fine — the broader refactor can come later.
- Hooks into the agent-runs / decisions tables (those don't render
  max DD).

# Sync-before-work ritual

```bash
cd /Users/edkennedy/Code/xvision
git fetch --prune origin
git -C .worktrees/max-drawdown-danger-tone status
git -C .worktrees/max-drawdown-danger-tone log --oneline -3 origin/main..HEAD
```

If the worktree does not exist:

```bash
git fetch --prune origin
git worktree add .worktrees/max-drawdown-danger-tone -b task/max-drawdown-danger-tone origin/main
```

# Notes

Recon (2026-05-21) findings:

- `drawdownToneClass` lives at `frontend/web/src/routes/eval-runs.tsx:1069`
  — magnitude-aware *but* in a way the operator considers wrong
  (`text-warn` for |n| < 10, `text-danger` for |n| ≥ 10).
- `eval-runs-detail-mobile.tsx:354` has a `< 0 ? "neg" : undefined`
  check that's actively buggy for positive stored DDs.
- `eval-runs-detail.tsx:605` and `eval-compare.tsx:213` don't pass tone
  at all — Max DD renders in default text color today.
- Backend payload sign convention is mixed: some paths emit positive
  drawdown (e.g. `final_pnl_usd, max_drawdown_pct` in `RunEquitySeries.ts`),
  others use `< 0` checks downstream. The display helper must accept
  both signs and treat magnitude as the source of truth, per intake
  §"Max DD color".
