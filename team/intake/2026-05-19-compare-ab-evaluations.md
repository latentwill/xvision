# Intake — 2026-05-19 — Compare AB evaluations feature

Operator ask 2026-05-19: invest in the "Compare AB evaluations" surface
as a first-class product feature. Sits alongside the multi-eval capsule
work landing in the same wave — capsule gives the operator awareness
of N concurrent in-flight evals; compare gives them the post-hoc
side-by-side once the runs land.

## Source

Operator request, recorded immediately after the multi-eval capsule
intake (`docs/design/Capsule · Multi-Eval.html`) was merged into the
agent-run-observability surface. Tracks against the v3 roadmap item
"Expand per-agent metrics and compare views" in
`docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md:193`.

## Current state (what already ships)

- Route: `/eval-runs/compare?ids=…` mounted at
  `frontend/web/src/routes/eval-compare.tsx` →
  `EvalCompareRoute` (registered at `frontend/web/src/routes.tsx:82`).
- API: `compareRuns(ids)` →
  `crates/xvision-engine/src/eval/compare.rs::compare_runs` returns a
  `ComparisonReport { runs[], equity_curve, findings }`. Capped at 10
  runs (`crates/xvision-engine/src/api/chart.rs:1057`).
- CLI: `xvn ab-compare --cycles …` (terminology-lock 2026-05-10; was
  `--setups`).
- Type surface: `ComparisonReport`, `ComparisonRunSummary`,
  `ComparisonEquityCurve`, `CompareRunSeries` — all already ts-rs
  exported in `frontend/web/src/api/types.gen/`.
- Tests: `crates/xvision-engine/tests/api_eval_compare.rs`,
  `crates/xvision-engine/tests/chart_payload.rs::build_compare_payload_caps_at_10_runs`.

## Asks (not yet decomposed)

Open-ended product asks. The conductor decomposes one wave at a time;
this intake is the raw operator hopper.

1. **Live AB compare for in-flight runs.** Today `compareRuns` only
   operates on completed runs. Pair it with the multi-eval capsule so
   the operator can compare two-or-more running evals against each
   other in real time (streaming equity, streaming findings).
2. **Promote / demote AB arms inline.** From the compare view, mark
   one arm as the new champion strategy and demote the others, without
   leaving the page. Today the operator has to navigate per-run to
   trigger a retry or promote.
3. **Expand per-agent metrics.** The current report rolls up at the
   run level. Pull per-`AgentRef` slot metrics (latency, token spend,
   error count, intervention rate) so multi-agent strategies surface
   which slot is responsible for an arm's delta.
4. **Side-by-side trace dock.** When two compared arms are still live,
   surface their traces side-by-side rather than as separate
   `/agent-runs/<id>` routes. Likely reuses the existing trace dock
   primitives but in a split-pane mode.
5. **Statistical confidence on deltas.** The current findings list
   reports raw numeric deltas. Add a confidence summary
   (sample size, effect size, p-value or equivalent) so the operator
   isn't drawing conclusions from noise on a 30-cycle scenario.
6. **Compare templates.** Save a recurring compare set (e.g. "this
   strategy across all 2018-VIX scenarios") as a named template that
   the AB-compare route can reload, so the operator doesn't manually
   reselect ids each iteration.
7. **Capsule → compare bridge.** From the multi-eval capsule, allow
   the operator to multi-select sibling rows and jump straight to
   `/eval-runs/compare?ids=…` with those runs pre-selected. Today the
   capsule only switches focus to one run at a time
   (`onSwitchFocus(r)`).
8. **Mobile compare view.** No mobile route exists at
   `/eval-runs/compare` today; the desktop pane is wide and
   table-heavy. Decide whether to ship a mobile variant or leave it
   desktop-only.

## Non-goals / out of scope

- Replacing the existing `compare_runs` engine path. Evolution, not
  rewrite — keep the report shape ts-rs-stable; extend with optional
  new fields if needed.
- Cross-strategy NFT-promotion gating. That's a separate identity
  track (`xvision-identity`) and should not block this feature.
- Charting backend rewrites. Reuse the existing `ChartEquityPoint`
  + `ComparisonEquityCurve` payloads.

## Verification (when a track lands)

Each decomposed track should:

- Add or update tests in `crates/xvision-engine/tests/api_eval_compare.rs`.
- Type-check + test the dashboard:
  `pnpm --dir frontend/web typecheck && pnpm --dir frontend/web test --run eval-compare`.
- Keep the 10-run cap or document why it changed.
- Document any new comparison metric in
  `docs/superpowers/specs/<date>-compare-<feature>.md`.

## Related artifacts

- `docs/design/Capsule · Multi-Eval.html` — sibling design;
  capsule and compare share the multi-run conceptual model.
- `frontend/web/src/features/agent-runs/EvalCapsule.tsx` — new
  multi-eval capsule (landing in the same wave; `onSwitchFocus`
  is the natural hand-off point to a compare bridge).
- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` — v3
  roadmap item this maps to.
