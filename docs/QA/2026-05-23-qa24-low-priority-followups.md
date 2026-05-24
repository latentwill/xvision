# QA24 low-priority follow-ups

Source: `docs/QA/2026-05-23-filter-strategy-eval-efficiency.md` plus the QA24 implementation pass on 2026-05-23.

These are useful product/workflow improvements, but they were intentionally not folded into the QA24 deployment branch because they need broader API, workflow, or product decisions.

## Best next fixes

1. Add eval preflight/launch warnings for cadence mismatch and missing filter artifacts.
2. Add create-time cadence/timeframe fields, ideally with "match selected scenario".
3. Add backend `decision_provenance` fields instead of deriving synthesized-row counts from justification text.
4. Add filter diagnostics to compare runs: action divergence, first divergence, filter events by arm.
5. Add an early-stop/noop-skip disable option for QA eval runs.
6. Add a "clone as baseline/variant" action so filter A/B tests are not hand-built.

## Additional backlog ideas

- Remote CLI error copy should point creation tasks to the supported API/UI path.
- Consider `xvn remote recipe strategy-eval` or a short remote-safe strategy eval recipe.
- Add scenario regime/trade-density tags such as `trend`, `chop`, `false-breakout`, `volatility-expansion`, and `high-trade-density`.
- Maintain a default cadence-compatible filter stress-test scenario.
- Show eval header filter state as `No filter attached`, `Prompt-only filter language`, or `Filter attached and evaluated`.
- Add explicit filter-QA launch checks that warn when a prompt mentions filter language but no XVN filter artifact is attached.
- Make recorder insertion for duplicate `agent_runs.id` idempotent or improve warning context.

## Opinionated items to validate first

- Whether remote CLI limitations are a product bug or an intentional security boundary.
- Whether baseline/variant creation belongs in strategy authoring, eval launch, or compare.
- Which filter diagnostics are essential for v1 versus exploratory analytics.
- Whether early-stop/noop-skip should be disabled by mode, by explicit QA flag, or only annotated in results.
