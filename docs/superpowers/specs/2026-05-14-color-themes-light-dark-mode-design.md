# Color Themes and Light/Dark Mode Design

**Date:** 2026-05-14
**Surface:** Vite dashboard SPA
**Status:** Draft for user review

## Goal

Add color-only dashboard themes, including a true black theme and a light mode, without changing fonts, spacing, layout, component shape, or other non-color design decisions.

## Decisions

1. Folio dark remains the default palette for users with no saved preference.
2. Theme preference is frontend-only in v1 and persists in localStorage.
3. Settings gains a canonical `General` tab for theme selection.
4. The left sidebar gains compact sun/moon icon controls for fast day/night switching.
5. The first shipped preferences are `Auto`, `Light`, `Folio dark`, and `Black`.
6. `Auto` follows the browser/OS `prefers-color-scheme` media query.
7. Black is an explicit theme, not the default dark mode.
8. Apart from the approved General tab and sidebar quick-toggle controls, the implementation must only change colors and color plumbing.

## Scope

This spec covers:

- app-level theme preference state
- CSS color-token scopes for light, Folio dark, and black themes
- General settings UI for theme selection
- sidebar sun/moon quick toggle
- chart palette integration with the resolved app theme
- tests and manual QA for theme behavior

This spec does not cover:

- server-backed user preferences
- account-level or cross-browser sync
- custom theme authoring
- fonts, font sizes, spacing, borders, radii, icons, page layout, or route structure beyond adding the settings tab and sidebar icon controls
- chart indicator or layer behavior unrelated to theme colors

## Existing Context

The dashboard currently uses CSS variables in `frontend/web/src/styles/tokens.css`. Tailwind maps utility colors to those variables in `frontend/web/tailwind.config.ts`, so most existing components can switch theme by changing variable values rather than changing classes.

The current token set is the warm Folio dark palette and should become the `folio-dark` theme. `:root` should keep Folio dark fallback values so first paint and missing `data-theme` states preserve the current appearance.

There is already a `zustand` UI store in `frontend/web/src/stores/ui.ts` and safe localStorage helpers in `frontend/web/src/lib/storage.ts`. Those are the preferred integration points for local frontend preferences.

Chart components currently accept a `"dark" | "light"` style mode and many routes rely on the default dark value. The chart palette should instead follow the resolved app theme so black, Folio dark, and light modes all render coherent chart backgrounds, grids, text, and series colors.

## User Experience

### General Settings

`/settings` should redirect to `/settings/general`. The Settings tabs should include:

- `General`
- `Providers`
- `Brokers`
- `Danger zone`

The `General` tab contains an Appearance section with four choices:

- `Auto`
- `Light`
- `Folio dark`
- `Black`

The control should be compact and operational, consistent with the existing settings pages. Each concrete theme option should include a small color swatch using the theme's background, surface, text, border, and accent colors. The control must not introduce new fonts or layout language.

Changing the setting applies immediately without a reload. The selected preference persists through reloads using localStorage.

### Auto Behavior

`Auto` resolves from `prefers-color-scheme`:

- browser/OS light resolves to `Light`
- browser/OS dark resolves to `Folio dark`

When the browser/OS preference changes while `Auto` is selected, the dashboard updates immediately. `Auto` should not resolve to `Black`; black is an explicit user preference.

### Sidebar Quick Toggle

The left sidebar should add compact sun and moon icon buttons near the bottom, above the user/profile block.

- Sun sets the preference to `Light`.
- Moon sets the preference to the last explicitly selected dark theme.
- If no explicit dark theme has been selected, Moon sets `Folio dark`.
- Selecting `Black` in General settings updates the remembered dark theme so the Moon button returns to Black later.
- Selecting `Folio dark` in General settings updates the remembered dark theme so the Moon button returns to Folio dark later.
- If the current preference is `Auto`, the active icon should reflect the resolved mode.

The icon controls should be icon-first, with accessible labels and hover/focus states. Active state should use existing accent tokens. A long-press menu or dropdown from the sidebar is out of scope for v1; full selection belongs in General settings.

## Theme Model

Add a focused frontend theme module, for example `frontend/web/src/theme/themes.ts`, that defines:

```ts
export type ThemePreference = "auto" | "light" | "folio-dark" | "black";
export type ResolvedTheme = "light" | "folio-dark" | "black";
export type ThemeMode = "light" | "dark";

export type ChartThemeDefinition = {
  background: string;
  text: string;
  grid: string;
  series: Record<string, string>;
};

export type ThemeDefinition = {
  id: ResolvedTheme;
  label: string;
  mode: ThemeMode;
  chart: ChartThemeDefinition;
};
```

The module should also provide helpers to:

- validate persisted localStorage values
- resolve `auto` from `prefers-color-scheme`
- expose labels and swatch metadata for Settings
- expose the current dark preference fallback for the sidebar Moon button

The saved preference key should be stable and namespaced, for example `xvn.theme.preference`. The last explicit dark theme should use a separate namespaced key, for example `xvn.theme.dark`.

Invalid stored values should fall back to `folio-dark`.

## Theme Application

Add a small app-level provider or shell effect that:

1. Reads the saved preference before or during app startup.
2. Resolves the concrete theme.
3. Applies `data-theme="<resolved-theme>"` to `document.documentElement`.
4. Applies the `dark` class only when the resolved theme mode is dark.
5. Updates the `<meta name="theme-color">` value to match the resolved background.
6. Listens for `prefers-color-scheme` changes while preference is `Auto`.

The implementation should minimize first-paint flash. Keeping Folio dark values in `:root` is required. A tiny pre-React inline bootstrap in `index.html` is acceptable if needed to set the initial `data-theme` before the bundle loads, but it must only handle theme attributes and must share constants with the app as much as practical.

## CSS Tokens

Keep the existing token names so Tailwind classes and raw CSS continue to work:

- `--bg`
- `--surface-sidebar`
- `--surface-card`
- `--surface-elev`
- `--surface-panel`
- `--surface-hover`
- `--border`
- `--border-strong`
- `--border-soft`
- `--text`
- `--text-2`
- `--text-3`
- `--text-4`
- `--gold`
- `--gold-soft`
- `--gold-bg`
- `--gold-bg-strong`
- `--warn`
- `--danger`
- `--info`

`folio-dark` should preserve the current palette.

`black` should use true black or near-black surfaces:

- background should be `#000000`
- sidebar and cards should remain distinguishable through slight elevation differences
- borders must remain visible without becoming high-contrast outlines
- existing gold accent may be retained, adjusted only if contrast requires it

`light` should be a restrained light counterpart:

- neutral off-white background
- white or near-white cards
- readable dark text hierarchy
- warm accent compatible with existing gold usage
- warning, danger, and info colors adjusted for light-background contrast

Do not add theme-specific font, spacing, radius, or layout tokens as part of this feature.

## Chart Theme Integration

Charts should use the resolved theme, not route-level hardcoded defaults.

Update `frontend/web/src/components/chart/chart-theme.ts` so it accepts the resolved app theme or reads from the shared theme definition. It should provide chart colors for:

- chart background
- grid lines
- text
- candle up/down
- SMA/EMA/Bollinger/Donchian lines
- equity and drawdown
- position bands
- buy/sell/veto/hold markers

The black theme should use black-specific chart surfaces and grid colors, not simply the generic dark chart palette. Series colors should remain semantically stable: buy/long stays green, sell/short stays red, warnings/vetoes stay yellow/gold, and equity remains visually distinct.

## Dark-Class Audit

Existing `dark:*` Tailwind classes must be audited. The theme system should support Folio dark and Black as dark modes while allowing Light to avoid dark-only styling.

Known areas to inspect include:

- `frontend/web/src/routes/setup.tsx`
- `frontend/web/src/routes/settings/danger.tsx`
- `frontend/web/src/components/shell/ChatRail.tsx`

Prefer token-based classes over `dark:*` variants where the color should follow all themes.

## Accessibility

Theme controls must be keyboard reachable and screen-reader labeled.

The General setting should expose the current selected preference. The sidebar sun/moon controls should have labels such as `Switch to light theme` and `Switch to dark theme`.

All three concrete palettes must preserve readable contrast for body text, secondary text, borders, disabled states, focus states, and chart labels.

## Acceptance Criteria

1. Fresh load with no saved preference renders the current Folio dark palette.
2. `/settings` redirects to `/settings/general`.
3. `/settings/general` renders a General tab with an Appearance section.
4. General settings offers `Auto`, `Light`, `Folio dark`, and `Black`.
5. Choosing a theme preference applies immediately without reload.
6. Theme preference persists through reload using localStorage.
7. `Auto` responds to `prefers-color-scheme` changes.
8. Sidebar sun/moon buttons switch immediately between Light and the remembered dark theme.
9. Selecting Black in General settings makes the sidebar Moon button return to Black.
10. Black uses true black surfaces while preserving readable text, visible borders, and chart clarity.
11. Chart components render with the resolved app theme.
12. No font-family, font-size, spacing, border radius, route layout, or component structure changes are included except the approved General tab and sidebar icon controls.
13. Existing settings tests still verify Skills is not exposed as a Settings tab.

## Test Plan

Add focused frontend tests for:

- theme preference validation and fallback
- `auto` resolution for light and dark media-query states
- localStorage read/write failures using the safe storage helpers
- remembered dark theme behavior
- `/settings` redirect to `/settings/general`
- SettingsLayout tab rendering, including General and excluding Skills
- General settings selection behavior
- Sidebar sun/moon toggle behavior
- chart theme mapping for Light, Folio dark, and Black

Manual QA should verify:

- Home, Settings, command palette, chat rail, and at least one chart route in Light, Folio dark, and Black
- reload persistence for every preference
- OS/browser color-scheme change while `Auto` is selected
- keyboard focus and labels for General settings and sidebar controls

## Risks

1. Hardcoded color utility classes such as `text-rose-300` may read poorly in Light mode if not migrated to tokens.
2. Chart routes currently default to dark mode, so it is easy to miss a surface unless all chart entry points use the same resolved theme source.
3. A startup flash can occur if React applies the theme only after first render. The Folio-dark fallback avoids a broken first paint, but a bootstrap may be needed for saved Light or Black preferences.
4. Adding General as the default Settings route changes the current `/settings` redirect from Providers; tests and navigation expectations must be updated deliberately.

## Result

When complete, xvision keeps its current Folio dark look by default, adds a true black theme and a light mode, gives operators a canonical General settings page for appearance, and provides quick sun/moon switching from the persistent left sidebar. The work is entirely color-focused and leaves typography, layout, and component structure intact.
