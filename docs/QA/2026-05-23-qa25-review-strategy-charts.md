# QA25 Review, Strategy, Filter, and Chart Items

Source: operator QA pass on 2026-05-23.

## Intake

- Add an easy “set all review agents” control instead of setting each review prompt/profile individually.
- Replace the four review prompt buttons with one review prompt field and a review prompt preset dropdown, while preserving custom prompt entry.
- Replace the gear next to review with the standard model pick list used by the chat rail.
- Fix strategy inspector spacing so `Strategy ID:` and the ID do not run together.
- Rename `Cadence (minutes)` to `Time frame` and expose standard candle timeframes such as `15m`, `1h`, `4h`, and `1d`.
- Use a standardized timeframe component.
- Filter format should only be JSON; remove TOML from the UI.
- Replace “No filter artifact attached” with clearer UI wording.
- Fix filter save failures that surface as `internal: internal error`.
- Remove “Prompt wording alone is not an XVN filter artifact.” from strategy/filter UI.
- Fix chat rail strategy creation failures caused by `agent name mentions SOL but system_prompt does not`.
- Add strategy deletion through the UI.
- Replace the main page chart snapshot from the old TradingView/lightweight chart surface to XVN charts.
- Improve XVN chart interaction with smooth wheel scrolling and `+` / `-` zoom controls.

## Implementation Notes

- Keep the public storage field `decision_cadence_minutes`; only the UI copy/control changes to `Time frame`.
- Preserve the agent validator’s diagnostic warning for asset-name/prompt mismatches, but stop making that warning a hard save gate for AI-generated strategy agents.
- The filter editor remains a raw JSON editor for now; form-based filter composition is a later improvement.
