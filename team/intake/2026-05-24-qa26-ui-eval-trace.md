# QA26 UI / Eval / Trace Intake

Source: operator QA26, 2026-05-24.

## Implemented in `qa26-ui-eval-trace`

- Eval run detail now uses a generic topbar title, shows the full run ID near the top, and includes `mode · status · Run #... · timestamp` metadata without truncating the ID.
- Scenario detail no longer repeats the scenario title and "Back to scenarios" in both topbar and page body.
- Eval launch and mode filters no longer offer Paper. The launch modal presents Backtest and a disabled Live option.
- Review presets no longer expose "Apply model to all review presets"; the review model defaults to an enabled provider/model from Settings when the preset's saved model is not enabled.
- Strategy filter authoring is JSON-only on the UI and authoring API. TOML sources now fail as validation.
- Strategy filter saves update the cached strategy detail immediately so a successful save does not continue showing "No saved filter".
- The reserved `filter` agent role and `Capability::Filter` agent refs are rejected. Filters are strategy JSON artifacts, not agent types.
- Authoring validation failures from strategy save/filter/agent mutations are mapped to validation errors instead of generic internal errors.
- Supervisor category rows are hidden from the indented trace timeline so the "SUPER" span no longer appears there.
- Eval observability now reloads `observability.toml` at run start, so changing the UI to full debug affects the next run without relying on the startup config snapshot.

## Deferred / Product-Scope Items

- Decision semantics: filter non-firing bars and synthetic no-op rows should not be counted as "decisions"; only LLM calls and filter-firing moments should count. This needs backend result-shape work so charts, tables, exports, and metrics agree.
- Trace differentiation for filter firings: add explicit filter-fire/filter-skip trace events or spans and a visible decision table badge so filter moments are not confused with LLM decisions.
- Scenario asset removal: scenarios should become date ranges/windows and stop owning asset/granularity. This requires API/schema migration, form changes, eval initialization changes, and compatibility handling for existing scenarios.
- Scenario granularity removal: strategy/agent timeframe should drive bar loading. This should land with the scenario asset migration.
- Full multi-asset surfaces: backend support still needs first-class UI/CLI affordances for multi-asset strategy setup, scenario initialization, and eval result display.
- Consider replacing the standalone Scenarios page with a date-range picker once scenario asset/granularity are removed.
