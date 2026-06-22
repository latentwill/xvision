# Dropdown Audit and Strategy Search Design

## Context

xvision is a product UI for technical operators managing AI trading strategies. Dropdowns are part of capital-adjacent workflows: strategy selection, model/provider choice, scenario setup, filtering, sorting, pagination, and action menus. The current frontend already has a Signal-styled floating menu primitive in `frontend/web/src/components/primitives/SignalMenu.tsx`, a rich `ModelPicker`, tokenized styling in `frontend/web/src/styles/tokens.css`, and several remaining native `<select>` or bespoke combobox surfaces.

## Goal

Audit every dropdown, menu, select, and combobox in `frontend/web/src` and bring them into one consistent Signal control vocabulary. Strategy dropdowns must support text search.

## Non-goals

- Do not redesign the app shell, sidebar, tables, charts, or route-level IA.
- Do not add a new UI dependency unless the existing primitives cannot be made accessible.
- Do not change backend API contracts unless a strategy selector lacks enough fields to display and search current options.
- Do not convert unrelated text search fields into dropdowns.

## Recommended Approach

Use the existing Signal primitive family as the foundation, not native `<select>` styling. Native select dropdown panels cannot be reliably styled across browsers and have already produced route-specific failures such as blank-select behavior in scenario granularity handling. The safer cutover is:

1. Inventory every dropdown surface.
2. Extend shared primitives where behavior is common.
3. Add a searchable strategy picker primitive.
4. Migrate native and bespoke strategy selectors first.
5. Migrate remaining native selects or explicitly document any retained native controls.
6. Verify with unit tests, keyboard checks, and visual screenshots.

## Control Taxonomy

### Action menus

Use `SignalActionMenu` for overflow menus and grouped actions. Requirements:

- `button` trigger with accessible name.
- `aria-haspopup="menu"` and `aria-expanded`.
- Portal or fixed-position floating panel so menus are not clipped by scroll containers.
- Escape and outside-click close behavior.
- Disabled actions remain visible but not clickable.

### Single-select menus

Use `SignalSelectMenu` or a shared successor for compact filters, sort controls, and finite enum choices. Requirements:

- Selected item has visual checkmark and `aria-selected` when rendered as listbox options.
- Trigger label reflects current value.
- Empty/loading states are represented inside the menu, not as blank native controls.
- Visual treatment matches Signal tokens: `surface-elev` panel, `border-border`, text tokens, accent only for focus/selected state.

### Multi-select menus

Use `SignalCheckboxMenu` for column pickers and filters where multiple values may be active. Requirements:

- Trigger shows active count or active state.
- Clear action is available when at least one value is selected.
- Each option exposes checked state to assistive tech.

### Searchable entity comboboxes

Use a new shared searchable combobox when options exceed a short static enum or when the selector chooses an entity such as a strategy. Requirements:

- Text input inside the menu filters options by visible label, stable id, and relevant metadata.
- Supports loading, empty, and no-results states.
- Keyboard behavior: open trigger, focus search input, type to filter, ArrowDown/ArrowUp move through options, Enter selects, Escape closes, Tab leaves predictably.
- ARIA behavior: combobox/listbox relationship with accessible name, expanded state, active descendant or roving focus, selected state.
- Popover remains fixed/portal-based to avoid clipping.

## Strategy Dropdown Requirement

Every dropdown whose options are strategies must include text search. A strategy option is searchable by:

- Display name.
- Strategy id or `agent_id`.
- Bundle hash when present.
- Tags/template/origin when already available at the callsite.

The visible option should include enough context to disambiguate similarly named strategies: display name as primary text, id/hash as monospace secondary text, and optional status/origin metadata when present.

## Visual Standard

Dropdowns must feel like xvision controls, not generic SaaS widgets.

- Surface: `bg-surface-elev` or `bg-surface-card` depending on elevation.
- Border: `border-border`, stronger only on focus or active menus.
- Text: `text-text` for primary, `text-text-2`/`text-text-3` for metadata, never low-contrast placeholder gray.
- Accent: `gold`/`accent` for focus ring, selected check, and active count only.
- Radius: existing `--radius-card`/`rounded-card` or `rounded-sm` where neighboring controls use it.
- Motion: 120-200ms opacity/translate for open/close if implemented; reduced motion collapses transition.
- Z-index: one semantic dropdown layer above sticky chrome and below modals/toasts/tooltips.

## Accessibility Standard

All menu-like controls must satisfy WCAG AA expectations:

- Accessible trigger labels.
- Visible focus ring or border change on triggers, search input, and options.
- Keyboard access without pointer use.
- Escape closes without losing page context.
- Disabled/loading states announced through text and disabled attributes where appropriate.
- No keyboard trap inside the menu.
- No reliance on color alone for selected/checked/destructive state.

## Audit Method

The audit must cover:

- Native `<select>` and `<option>` usage.
- `role="combobox"` and custom comboboxes.
- `aria-haspopup="menu"` / `aria-haspopup="listbox"` triggers.
- `SignalActionMenu`, `SignalSelectMenu`, `SignalCheckboxMenu`, `SignalModelPickerMenu`, and custom `useSignalMenu` compositions.
- Route-level controls in `frontend/web/src/routes/**`.
- Shared controls in `frontend/web/src/components/**`.

For each surface, record:

- File path and surrounding component/function.
- Current control primitive.
- Options source and whether the list is static, API-backed, or derived.
- Whether it is strategy-related and therefore needs text search.
- Accessibility/styling gap.
- Migration decision: migrate to shared primitive, keep native with justification, or fix primitive only.

## Testing Strategy

Use test-first implementation.

1. Add failing tests for the searchable strategy picker:
   - filters by strategy display name;
   - filters by id/hash metadata;
   - shows no-results state;
   - selects the highlighted result with keyboard;
   - preserves accessible name and expanded state.
2. Add failing tests for migrated callsites that require strategy search.
3. Add primitive-level tests for menu keyboard/focus behavior if missing.
4. After green tests, visually verify representative routes with menus open.

## Visual Verification

Capture screenshots after implementation for at least:

- Strategies list/folder controls.
- Strategy authoring or agent slot controls where strategy/model/provider choices appear.
- Scenario form dropdowns.
- Eval run/detail sorting or filtering dropdowns.
- Settings provider/broker/tool menus.

Screenshots should show the closed trigger and open menu state where practical.

## Acceptance Criteria

- Every dropdown/menu/select/combobox in `frontend/web/src` is inventoried.
- Every strategy dropdown includes text search.
- Existing strategy selectors search by visible name and stable id where data is available.
- Dropdown popovers use Signal styling and tokens consistently.
- Menus are not clipped by scroll/overflow containers.
- Keyboard and focus behavior works for shared primitives and migrated strategy selectors.
- Native `<select>` usage is eliminated where styling/search requirements apply, or retained only with a written reason in the audit notes.
- Tests cover searchable strategy selection and at least one migrated callsite.
- Visual screenshots confirm the main dropdown families look consistent.
