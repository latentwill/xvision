# Intake — 2026-05-18 — TradingView Advanced Charts upgrade

Operator follow-up (2026-05-18) on the long-deferred TV Advanced Charts
upgrade noted in `docs/superpowers/plans/2026-05-08-strategy-engine-2d-dashboard-wizard.md:32`
("TradingView Advanced Charts upgrade — Lightweight only in v1").
Forward-looking; not contracts yet — gated on the license request and
a datafeed-shape spike.

## Operator scoping (2026-05-18)

| Question | Answer |
|---|---|
| Scope | **All detail charts, not inline.** Replace `RunChart`, `ScenarioChart`, `CompareChart`, `StrategyChart`. Keep Lightweight Charts for inline chat charts (`InlineChartSvg`) and the Wizard preview. |
| License | **No — need to request it.** First track is the TV form + ToS review (free for product attribution; commercial review for the marketplace surface). Code work starts only after the library file is in hand. |
| v1 features | **Better-organized indicators + design polish.** Not drawing tools, not save/load layouts, not Pine/JS studies, not multi-symbol overlay. The win is visual hierarchy + indicator UX, not feature surface. |
| Datafeed | **UDF (Universal Datafeed) endpoints.** Implement `/api/udf/*` in `xvision-dashboard` mirroring TV's UDF protocol (`config`, `symbols`, `history`, `marks`). TV maintains the client adapter. |

## What this rules out (record so it doesn't drift back in)

- **No drawing tools** in v1. If the operator changes their mind we
  re-open the question; the Advanced library supports them out of the
  box but they need a per-user persistence backend.
- **No save/load chart layouts** in v1. Same reasoning.
- **No Pine / custom studies** in v1. Big sandboxing scope.
- **No multi-symbol overlay** in v1. That's a multi-asset feature
  and is captured in the sibling intake
  `team/intake/2026-05-18-multi-asset-agents-strategies-eval.md`.
- **No inline (chat / Wizard preview) replacement.** Lightweight
  stays for those surfaces; bundle and DX cost not worth it where
  the chart is a thumbnail.

## Dependencies before decomposition

1. **TV license request.** Submit the Advanced Charts request form
   (https://www.tradingview.com/advanced-charts/) including the
   xvision marketplace context so the commercial-use review is on
   the record. Block any code track until the library file lands.
2. **UDF datafeed shape spike.** Compare the existing
   `/api/chart/*` payloads to the UDF contract; produce a
   mapping doc (symbol-search, history, marks). Output is a
   short ADR or design note.
3. **Indicator-set inventory.** Current set lives in `chart-theme.ts`
   + `chart-layers.ts` + the `payload.indicators.*` Rust shape.
   Audit which the operator actually uses + how Advanced groups
   them natively so "better-organized indicators" has a concrete
   target instead of "more polish".

## Likely track decomposition (sketch only — do not contract yet)

| Layer | Track (provisional) | Lane | Notes |
|---|---|---|---|
| Procurement | `tv-advanced-license-request` | foundation | Non-code; ToS review |
| Spike | `tv-advanced-udf-mapping-spike` | foundation | Design note / ADR |
| Backend | `udf-datafeed-endpoints` | foundation | `/api/udf/config`, `/api/udf/symbols`, `/api/udf/history`, `/api/udf/marks` in xvision-dashboard |
| Backend | `udf-marks-from-run-payload` | integration | Adapt trade/veto/hold markers to UDF marks endpoint |
| Frontend | `tv-advanced-bundler-setup` | foundation | Vendor the library file, wire Vite, gate behind a build flag |
| Frontend | `runchart-advanced-migration` | leaf | Replace RunChart implementation; keep public component API stable |
| Frontend | `scenariochart-advanced-migration` | leaf | Same for ScenarioChart |
| Frontend | `comparechart-advanced-migration` | leaf | Same for CompareChart |
| Frontend | `strategychart-advanced-migration` | leaf | Same for StrategyChart |
| Frontend | `tv-advanced-indicator-organization` | leaf | The actual v1 value: group / categorize indicators in the Advanced indicator panel; drop low-signal ones |

That's 10 tracks if it lands as one wave. More realistic: three waves
(procurement + spike, then backend UDF, then frontend migrations).

## Out of this intake (explicit)

- Drawing tools persistence (per-user, per-chart key) — punt to a
  separate intake if requested.
- Pine / custom-script editor surface — separate intake.
- Multi-symbol / multi-asset overlay — covered by the multi-asset
  intake. Cross-reference at decomposition time.
- TV-style watchlist / symbol search across venues — would need our
  catalog crate; out of scope until ingestion grows beyond Alpaca.

## Next steps

1. Operator submits the TV Advanced Charts request form. Conductor
   tracks the response in `team/decisions.md` as a new entry once
   submitted.
2. Conductor opens the UDF mapping spike track once the library
   file lands.
3. After the spike's ADR is written, decompose the rest of this
   intake into the contract set above (or whatever the ADR
   suggests).
