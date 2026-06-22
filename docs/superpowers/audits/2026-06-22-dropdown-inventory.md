# Dropdown Inventory — 2026-06-22

## Summary

| Category | Count | Decision |
|---|---:|---|
| Signal primitives already compliant | 17 | Keep / improve primitive |
| Searchable strategy/entity pickers | 9 | Migrate first |
| Native enum selects | 24 | Migrate to SignalSelectMenu |
| Bespoke comboboxes | 4 | Align or migrate |
| Retained native controls | 2 | Must include justification |

## Surfaces

| Path | Component/function | Current control | Options source | Strategy-related? | Decision | Notes |
|---|---|---|---|---|---|---|
| `frontend/web/src/components/primitives/SignalMenu.tsx` | `SignalActionMenu` | Signal portal menu (`aria-haspopup="menu"`) | `groups[].items` prop | no | Keep / improve primitive | Standard action-menu primitive; already handles portal positioning, outside click, and Escape. |
| `frontend/web/src/components/primitives/SignalMenu.tsx` | `SignalSelectMenu` | Signal portal single-select listbox | `options` prop | no | Keep / improve primitive | Standard enum/select primitive for migration targets. |
| `frontend/web/src/components/primitives/SignalMenu.tsx` | `SignalCheckboxMenu` | Signal portal multiselect listbox | `options` prop | no | Keep / improve primitive | Available primitive for checkbox/menu migrations such as columns. |
| `frontend/web/src/components/primitives/SignalMenu.tsx` | `SignalModelPickerMenu` | Signal searchable model listbox | `ModelOption[]` grouped by provider | no | Keep / improve primitive | Already searchable by model/provider and used through `ModelPicker`. |
| `frontend/web/src/components/ModelPicker.tsx` | `ModelPicker` / `ModelPickerDropdown` | `SignalModelPickerMenu` wrapper | configured `ProviderRow.enabled_models` filtered by provider | no | Keep / improve primitive | Shared model picker for chat, settings, authoring, optimizer, and review surfaces. |
| `frontend/web/src/components/AssetPicker.tsx` | `AssetPicker` | bespoke inline combobox (`role="combobox"` + listbox) | `assets` prop grouped by `AssetInfo.category` | yes | Align bespoke combobox | Searchable entity picker; keep behavior, audit ARIA/keyboard parity if not moving into shared picker primitive. |
| `frontend/web/src/components/TimeframeSelect.tsx` | `TimeframeSelect` | native `<select>` | `STANDARD_TIMEFRAMES` plus current custom value | no | `SignalSelectMenu` | Reusable enum select; should share Signal styling. |
| `frontend/web/src/components/agent/SlotForm.tsx` | provider field | native `<select>` | `providerNames` from provider query | no | `SignalSelectMenu` | Dynamic provider list, but small enough for standard Signal select. |
| `frontend/web/src/components/agent/SlotForm.tsx` | model field | `ModelPicker` | provider rows filtered to selected provider | no | Keep / improve primitive | Already on shared searchable model picker. |
| `frontend/web/src/components/agent/SlotForm.tsx` | memory field | native `<select>` | static `off/global/agent_scoped` memory modes | no | `SignalSelectMenu` | Native enum select in agent slot editor. |
| `frontend/web/src/components/eval-detail/DecisionsTable.tsx` | decisions sort | native `<select aria-label="Sort decisions">` | static `SortKey` options | no | `SignalSelectMenu` | Dense table sort control. |
| `frontend/web/src/components/lists/ListToolbar.tsx` | filter controls | `SignalSelectMenu` | `ActiveFilter.def.options` | varies by route | Keep / improve primitive | Route list filters are already standardized through this component. |
| `frontend/web/src/components/lists/ListToolbar.tsx` | sort control | `SignalSelectMenu` | `ListSortState.options` | varies by route | Keep / improve primitive | Route list sort menus are already standardized through this component. |
| `frontend/web/src/components/lists/ListToolbar.tsx` | `ColumnPickerButton` | bespoke absolute checkbox dropdown | non-essential `columns` with `ColumnState` | no | Align or migrate | Consider `SignalCheckboxMenu` or a column-specific Signal primitive; currently lacks listbox/menu roles. |
| `frontend/web/src/components/primitives/useServerPagination.tsx` | `ServerPagerStrip` page size | native `<select id="list-pagination-size">` | `PAGE_SIZE_OPTIONS` | no | Retain native | Compact pagination affordance; retaining is acceptable if visual inconsistency is intentional and documented. |
| `frontend/web/src/components/scenario/ScenarioForm.tsx` | calendar field | native `<select>` | static `CalendarKind` options | no | `SignalSelectMenu` | Scenario authoring enum select. |
| `frontend/web/src/components/calendar-picker/calendar-desktop.tsx` | `InlineRangeBar` | bespoke inline expandable date range panel | generated month/year/day grids | no | Retain native | Date-range picker, not an option dropdown; keep out of select/menu migration unless date-picker scope expands. |
| `frontend/web/src/components/shell/ChatRail.tsx` | chat context menu | bespoke inline `role="menu"` radio menu | `RailContextMode` values | no | `SignalSelectMenu` or radio-menu primitive | Small enum menu; can migrate to shared primitive or extract a Signal radio-menu variant. |
| `frontend/web/src/components/shell/ChatRail.tsx` | `RailModelBar` model | `ModelPicker` | provider rows from settings | no | Keep / improve primitive | Chat model picker already uses shared searchable menu. |
| `frontend/web/src/components/shell/CommandPalette.tsx` | command palette input | bespoke modal combobox (`role="combobox"`) | command/search result groups | yes | Align bespoke combobox | Global search/navigation surface; not a dropdown replacement, but should share combobox accessibility expectations. |
| `frontend/web/src/components/strategy/InlineFilterComposer.tsx` | filter agent picker | native `<select>` | `filterCandidates` AgentRefs | yes | `StrategyPicker` / agent picker | Dynamic entity picker; should search by name/id and show scoped state. |
| `frontend/web/src/components/strategy/InlineFilterComposer.tsx` | provider/model picker | `ModelPicker` | `providers` prop | no | Keep / improve primitive | Already uses shared searchable model picker. |
| `frontend/web/src/components/strategy/InlineFilterComposer.tsx` | scalar operator | native `<select aria-label="operator">` | `SCALAR_OPS` | no | `SignalSelectMenu` | Small enum select in inline strategy filter composer. |
| `frontend/web/src/features/autooptimizer/screens/AutoresearcherTab.tsx` | source strategy | bespoke searchable input dropdown | `listStrategies()` filtered by display name/id | yes | `StrategyPicker` | Already searchable but bespoke and missing combobox/listbox roles. |
| `frontend/web/src/features/autooptimizer/screens/AutoresearcherTab.tsx` | label strategy | native `<select id="ar-label-strategy">` | static `price_forward/outcome_imitation` modes | yes | `SignalSelectMenu` | Enum affects autoresearch label generation; migrate after strategy picker. |
| `frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx` | `LaunchPanel` parent strategy | native `<select id="optimizer-strategy">` | `listStrategies()` | yes | `StrategyPicker` | Parent strategy must search by name/id. |
| `frontend/web/src/features/autooptimizer/ui/LaunchPanel.tsx` | experiment writer / reviewer overrides | `ModelPicker` | provider rows from settings | no | Keep / improve primitive | Two model override surfaces share the same control. |
| `frontend/web/src/features/autooptimizer/ui/NanochatSlotCard.tsx` | nanochat model | native `<select id="nc-checkpoint-picker">` | checkpoint list (`checkpoints`) | no | `SignalSelectMenu` | Dynamic checkpoint list; may become searchable if checkpoint count grows. |
| `frontend/web/src/features/eval-runs/review/AgentPicker.tsx` | review model | `ModelPicker` | provider rows from settings | no | Keep / improve primitive | Review model already uses shared model picker. |
| `frontend/web/src/features/eval-runs/review/AgentPicker.tsx` | review prompt preset | native `<select aria-label="Review prompt preset">` | `CANONICAL_AGENT_PROFILES` | no | `SignalSelectMenu` | Static profile preset select. |
| `frontend/web/src/features/eval-runs/review/MemoryPanel.tsx` | recall row actions | bespoke absolute `role="menu"` | recall item actions (`Open Pattern`) | no | `SignalActionMenu` | One-off action menu should use shared action primitive. |
| `frontend/web/src/features/live/VenueAccountPanel.tsx` | venue account picker | `SignalSelectMenu` | configured `LIVE_VENUE_KINDS` from broker settings | no | Keep / improve primitive | Already standard Signal select. |
| `frontend/web/src/features/marketplace/routes/browse/Toolbar.tsx` | marketplace sort | `SignalSelectMenu` | `SORT_LABELS` / allowed sort keys | no | Keep / improve primitive | Already standard Signal select. |
| `frontend/web/src/features/memory/MemorySurface.tsx` | demo source | native `<select>` | static demo source modes | no | `SignalSelectMenu` | Memory demo setup enum select. |
| `frontend/web/src/features/memory/MemorySurface.tsx` | namespace filter | native `<select>` | `agent:{id}` / `global` by mode | no | `SignalSelectMenu` | Memory namespace control. |
| `frontend/web/src/features/memory/MemorySurface.tsx` | lifecycle filter | native `<select>` | static `all/active/staged/forgotten` lifecycle values | no | `SignalSelectMenu` | Memory pattern lifecycle filter. |
| `frontend/web/src/features/memory/MemorySurface.tsx` | pattern namespace | native `<select>` | `agent:{id}` / `global` by mode | no | `SignalSelectMenu` | Memory pattern create/edit namespace control. |
| `frontend/web/src/routes/agents.tsx` | agents list toolbar | `ListToolbar` Signal filters/sort/columns | `SHAPE_FILTER`, `ARCHIVED_FILTER`, `SORT_OPTIONS`, columns | no | Keep list primitives; align columns | Filter/sort already Signal; column picker inherits bespoke `ColumnPickerButton`. |
| `frontend/web/src/routes/agents.tsx` | `AgentToolsSelect` | native `<select>` | `listTools()` plus current multi-tool sentinel | no | `SignalSelectMenu` or multiselect | Single-tool dropdown masks multi-tool custom state; audit before choosing single vs multi primitive. |
| `frontend/web/src/routes/authoring.tsx` | pipeline kind | native `<select>` | static `PipelineKind` options | yes | `SignalSelectMenu` | Strategy pipeline shape enum. |
| `frontend/web/src/routes/authoring.tsx` | existing agent | native `<select>` | `props.available` AgentRefs | yes | `StrategyPicker` / agent picker | Dynamic agent picker should search by name/id. |
| `frontend/web/src/routes/authoring.tsx` | new agent model | `ModelPicker` | provider rows from settings | no | Keep / improve primitive | Already shared searchable model picker. |
| `frontend/web/src/routes/authoring.tsx` | entry rule direction | native `<select>` | static `long/short` | yes | `SignalSelectMenu` | Strategy rule enum. |
| `frontend/web/src/routes/authoring.tsx` | close policy kind | native `<select>` | static close policy kinds | yes | `SignalSelectMenu` | Strategy risk/exit enum. |
| `frontend/web/src/routes/eval-compare.tsx` | compare sort | native `<select data-testid="compare-sort">` | `COMPARE_SORT_OPTIONS` | yes | `SignalSelectMenu` | Eval comparison sort enum. |
| `frontend/web/src/routes/eval-runs.tsx` | runs list toolbar | `ListToolbar` Signal filters/sort/columns | strategy filter from `listStrategies()` + observed runs, `MODE_FILTER`, `STATUS_FILTER`, `SORT_OPTIONS` | yes | Searchable strategy filter | Strategy filter already uses SignalSelectMenu, but the current menu is not sufficient because strategy options need text search by display name/id. |
| `frontend/web/src/routes/eval-runs.tsx` | start eval strategy | native `<select id="eval-start-strategy">` | `listStrategies()` | yes | `StrategyPicker` | Strategy must search by name/id. |
| `frontend/web/src/routes/eval-runs.tsx` | start eval scenario | native `<select id="eval-start-scenario">` | `listScenarios()` | yes | `ScenarioPicker` | Dynamic scenario picker should search by name/id/window. |
| `frontend/web/src/routes/eval-runs.tsx` | auto-review provider/model | native `<select>` pair | `reviewProviderRows` and selected provider `enabled_models` | no | `ModelPicker` | Replace split provider/model selects with shared model picker. |
| `frontend/web/src/routes/scenarios.tsx` | scenarios list toolbar | `ListToolbar` Signal filters/sort/columns | `SOURCE_FILTER`, `ARCHIVED_FILTER`, `SORT_OPTIONS`, columns | no | Keep list primitives; align columns | Filter/sort already Signal; column picker remains bespoke. |
| `frontend/web/src/routes/scenarios-detail.tsx` | scenario chart asset | `AssetPicker` | `alpacaAssets` | yes | Align bespoke combobox | Entity picker already searchable; keep but standardize combobox behavior. |
| `frontend/web/src/routes/scenarios-detail.tsx` | scenario chart granularity | native `<select id="scenario-chart-granularity">` | `CHART_GRANULARITY_OPTIONS` | no | `SignalSelectMenu` | Indicator timeframe enum. |
| `frontend/web/src/routes/scenarios-detail.tsx` | scenario runs toolbar | `ListToolbar` Signal filters/sort/columns | `RUNS_MODE_FILTER`, `RUNS_STATUS_FILTER`, `RUNS_SORT_OPTIONS`, columns | yes | Keep list primitives; align columns | Run filters already Signal; column picker remains bespoke. |
| `frontend/web/src/routes/strategies.tsx` | strategies list toolbar | `ListToolbar` Signal filters/sort/columns | `SHAPE_FILTER`, `SORT_OPTIONS`, columns | yes | Keep list primitives; align columns | Pipeline shape filter and sort are already Signal; column picker remains bespoke. |
| `frontend/web/src/routes/strategies.tsx` | row actions | `SignalActionMenu` | Open/Duplicate/Compare/Delete action groups | yes | Keep / improve primitive | Already standard action menu. |
| `frontend/web/src/routes/settings/index.tsx` | Hyperliquid network | `SignalSelectMenu` | `NETWORK_OPTIONS` | no | Keep / improve primitive | Broker network surface already migrated. |
| `frontend/web/src/routes/settings/index.tsx` | Degen Arena network | native `<select id="degen-network">` | static `testnet/mainnet` | no | `SignalSelectMenu` | Same options as migrated broker cards; inconsistent today. |
| `frontend/web/src/routes/settings/index.tsx` | Extended broker network | `SignalSelectMenu` | `NETWORK_OPTIONS` | no | Keep / improve primitive | Broker network surface already migrated. |
| `frontend/web/src/routes/settings/MemorySettingsCard.tsx` | embedder source | native `<select id="memory-embedder">` | built-ins plus OpenAI-compatible provider rows | no | `SignalSelectMenu` | Dynamic but small settings source picker. |
| `frontend/web/src/routes/settings/MemorySettingsCard.tsx` | embedding model | native `<select id="memory-embedder-model">` | `CURATED_EMBEDDING_MODELS` | no | `SignalSelectMenu` | Static curated embedding model enum. |
| `frontend/web/src/routes/settings/providers.tsx` | providers list toolbar | `ListToolbar` Signal filters/sort/columns | `PROVIDER_KIND_FILTER`, `PROVIDER_SORT_OPTIONS`, columns | no | Keep list primitives; align columns | Provider kind filter/sort already Signal; column picker remains bespoke. |
| `frontend/web/src/routes/settings/providers.tsx` | legacy add-provider kind | native `<select>` | hard-coded provider kinds | no | `SignalSelectMenu` | Legacy form enum; newer form has richer `KIND_OPTIONS`. |
| `frontend/web/src/routes/settings/providers.tsx` | add-provider provider | native `<select>` | `KIND_OPTIONS` presets | no | `SignalSelectMenu` | Provider preset enum. |
| `frontend/web/src/routes/settings/skills.tsx` | skills list toolbar | `ListToolbar` Signal filters/sort/columns | `KIND_FILTER`, `SORT_OPTIONS`, columns | no | Keep list primitives; align columns | Skill kind filter/sort already Signal; column picker remains bespoke. |
| `frontend/web/src/routes/settings/skills.tsx` | skill kind | native `<select>` | `KIND_OPTIONS` | no | `SignalSelectMenu` | Skill create/edit enum. |
