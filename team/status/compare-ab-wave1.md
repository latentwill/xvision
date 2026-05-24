# Compare A/B Wave 1 Status

Date: 2026-05-24

Spec/plan used:
- `docs/superpowers/plans/2026-05-23-charts-section-b2-comparison-ab.md`
- `docs/superpowers/specs/2026-05-21-chart-rework-klinecharts-uplot.md`

Completed:
- `/charts/compare?ids=<run-a>,<run-b>` now calls `compare_runs(ids)` and renders Charts v2 overlay, roster, and cards.
- `/eval-runs/compare?ids=...` uses the Charts v2 equity overlay chrome instead of the legacy compare chart.
- `ComparisonRunSummary.strategy_name` is populated by the API from the Strategy manifest when available.
- CLI compare table and markdown output prefer readable strategy labels while keeping run ids and strategy ids visible.
- Manual docs and CLI/UI sweep skills now document the compare route, label/id contract, and QA check.

Evidence:
- `cargo test -p xvision-engine --test api_eval_compare`
- `cargo test -p xvision-cli --lib compare_format`
- `pnpm --dir frontend/web typecheck`
- `pnpm --dir frontend/web exec vitest run src/components/chart/v2/hooks/useCompareSelection.test.tsx src/components/chart/v2/primitives/b2-primitives.test.tsx src/components/chart/v2/surfaces/ComparisonABDashboard.test.tsx src/routes/eval-compare.test.tsx`

Known unrelated gap:
- Broad CLI integration compilation can still be blocked by pre-existing strategy fixture drift outside this feature branch.
