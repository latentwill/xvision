# Annotation + Review Wave 1

Date: 2026-05-24

## Completed

- Added migration `037_review_annotations_and_autofire` for persisted review annotations and per-run auto-review launch state.
- Extended manual LLM review parsing/prompting to accept and persist `annotations`.
- Converted rule-based auto-review from global finalize behavior to per-run opt-in via `auto_fire_review`.
- Synthesized chart annotations from rule-based findings so auto-review produces the same artifact shape as LLM review.
- Replaced real run annotation chart reads with persisted review annotations. `/api/v2/charts/annotated/demo` stays fixture-backed.
- Added dashboard launcher checkbox + review model picker, run-row auto-review pill, CLI launch flags, and generated TS types.
- Updated operator docs and CLI/UI skills.

## Evidence

- `cargo check -p xvision-engine -p xvision-cli -p xvision-dashboard -p xvision-mcp` passed.
- `cargo test -p xvision-engine --test eval_review` passed: 20 tests.
- `cargo test -p xvision-engine --test eval_review_auto` passed: 7 tests.
- `cargo test -p xvision-engine --test api_eval_run run_with_deps_` passed: 3 tests, including auto-fire opt-in and default-off coverage.
- `cargo test -p xvision-dashboard --test charts_annotated` passed: 3 tests, including persisted annotation route coverage.
- `pnpm --dir frontend/web typecheck` passed after installing locked dependencies.
- `pnpm --dir frontend/web exec vitest run src/components/chart/v2/primitives/b3-primitives.test.tsx src/routes/eval-runs.test.tsx src/routes/eval-runs-detail.test.tsx src/routes/eval-runs-detail-mobile.test.tsx` passed: 95 tests.
- `cargo run -q -p xvision-cli -- eval run --help | rg "auto-fire-review|review-provider|review-model|max-review-annotations"` showed the new CLI flags.
- `curl http://127.0.0.1:5174/charts/annotated?run_id=demo` returned `200 text/html` from the local Vite UI route.

## Deferred

- Manual "Run review" CTA/model picker polish on eval-run detail.
- True symbol-aware live lookup beyond scanning latest stored live review annotations.
