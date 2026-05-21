# Intake — 2026-05-16 — eval-review remainder + V2A onboarding

This is the first intake under the new conductor model. It carries forward
two cohorts:

1. The Eval Review Agent feature, partially landed (data-model only).
2. Onboarding + docs items from
   `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` V2A.

Source specs/plans:

- `docs/superpowers/specs/2026-05-15-eval-review-agent.md`
- `docs/superpowers/plans/2026-05-13-v2-v4-action-plan.md` items 1–3
- `docs/superpowers/specs/2026-05-16-execution-board-process-overhaul.md` (this overhaul itself)

## Raw items → tracks

| Raw item | Track | Lane |
|---|---|---|
| Build payload collector, prompt contract, dispatch, validation | `eval-review-agent-engine` | foundation |
| Add `POST /api/eval/runs/:id/review`, list/get routes, `xvn eval review` | `eval-review-api-cli` | leaf |
| Add Review panel to `/eval-runs/:id` | `eval-review-run-detail-ui` | leaf |
| Driver.js first-run + restart-tour | `v2a-driver-tour` | leaf |
| In-app docs/help route + index | `v2a-in-app-docs` | leaf |
| Resettable example strategies/scenarios/tutorial artifacts | `v2a-example-artifacts` | leaf |

## Out of this intake

- V2B items 4–6 (auth boundary, remote CLI orphan audit trail, kill switch) — separate intake when V2A lands.
- V2C, V3, V4 — long horizon; no contracts yet.
- Autoresearcher mutation loop — deferred per eval-review spec.

## Queued for next intake

| Item | Notes |
|---|---|
| Charting overhaul: KlineCharts + uPlot + Claude design system | Replace current charting stack. KlineCharts for candlestick/k-line pane, uPlot for oscillator/time-series panes (RSI, MACD, equity curve etc.), styled to match Claude design system. High priority — target next version after V2A lands. |

## Next deploy snapshot

`main` at audit time: `c5a3cf1` — typed trader-output failures with raw provider diagnostics (#180).

`main` is deploy-clean. No code changes are part of this intake — every
artifact written today is process/docs only and does not move the runtime
image.
