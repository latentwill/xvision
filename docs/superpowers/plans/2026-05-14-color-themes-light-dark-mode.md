# Color Themes Light/Dark Mode Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add color-only Light, Folio dark, Black, and Auto dashboard themes with a General settings page, sidebar sun/moon toggle, persistence, and chart theme integration.

**Architecture:** Keep Tailwind mapped to the existing CSS token names and switch values through `data-theme` on `document.documentElement`. Store theme preference and last dark theme in localStorage through existing safe storage helpers. Expose a small theme module and hook so Settings, Sidebar, app shell, and charts all use one resolved theme.

**Tech Stack:** React 18, React Router, Zustand, Vitest, Testing Library, Tailwind CSS variables, Lightweight Charts.

---

## File Structure

- Create `frontend/web/src/theme/themes.ts`: pure theme definitions, storage keys, validators, resolver helpers, chart palettes, and swatch metadata.
- Create `frontend/web/src/theme/useTheme.ts`: React hook/store that reads localStorage, listens to `prefers-color-scheme`, and writes preference changes.
- Create `frontend/web/src/theme/ThemeProvider.tsx`: applies `data-theme`, `dark` class, and `meta[name="theme-color"]`.
- Create `frontend/web/src/theme/themes.test.ts`: pure unit tests for validation, auto resolution, and dark fallback behavior.
- Create `frontend/web/src/theme/useTheme.test.tsx`: hook/provider tests for persistence and DOM application.
- Create `frontend/web/src/routes/settings/general.tsx`: General settings Appearance UI.
- Create `frontend/web/src/components/shell/Sidebar.test.tsx`: quick-toggle behavior tests.
- Modify `frontend/web/src/App.tsx`: wrap router in `ThemeProvider`.
- Modify `frontend/web/src/main.tsx`: import tokens/globals as-is; no theme logic unless bootstrap is needed in `index.html`.
- Modify `frontend/web/index.html`: remove hardcoded permanent `class="dark"`; optionally add tiny pre-React bootstrap if first-paint flash appears during implementation.
- Modify `frontend/web/src/styles/tokens.css`: keep current `:root` as Folio dark fallback; add `[data-theme="folio-dark"]`, `[data-theme="black"]`, and `[data-theme="light"]` token scopes.
- Modify `frontend/web/src/routes.tsx`: add `SettingsGeneralRoute`; redirect `/settings` to `general`.
- Modify `frontend/web/src/routes/settings/index.tsx`: add `General` tab and export route.
- Modify `frontend/web/src/routes/settings/settings-layout.test.tsx`: assert General tab and Skills exclusion.
- Modify `frontend/web/src/components/primitives/Icon.tsx`: add `sun` and `moon` icons.
- Modify `frontend/web/src/components/shell/Sidebar.tsx`: add quick-toggle buttons above the profile block.
- Modify `frontend/web/src/components/chart/chart-theme.ts`: accept `ResolvedTheme` and return palette from shared theme definitions.
- Modify chart components that currently accept `themeMode?: "dark" | "light"` so they use resolved theme by default while keeping prop override only where tests need direct control.
- Modify chart tests to assert black/light/folio chart palettes.

## Task 1: Theme Model

**Files:**
- Create: `frontend/web/src/theme/themes.ts`
- Create: `frontend/web/src/theme/themes.test.ts`
- Modify: `frontend/web/src/lib/storage.ts` only if a missing safe helper is required

- [ ] **Step 1: Write failing theme model tests**

Add `frontend/web/src/theme/themes.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import {
  coerceThemePreference,
  coerceDarkTheme,
  resolveTheme,
  themeDefinitions,
} from "./themes";

describe("theme model", () => {
  it("falls back to folio dark for invalid saved preferences", () => {
    expect(coerceThemePreference(null)).toBe("folio-dark");
    expect(coerceThemePreference("sepia")).toBe("folio-dark");
  });

  it("falls back to folio dark for invalid saved dark themes", () => {
    expect(coerceDarkTheme(null)).toBe("folio-dark");
    expect(coerceDarkTheme("light")).toBe("folio-dark");
    expect(coerceDarkTheme("black")).toBe("black");
  });

  it("resolves auto from browser color scheme without choosing black", () => {
    expect(resolveTheme("auto", "light")).toBe("light");
    expect(resolveTheme("auto", "dark")).toBe("folio-dark");
  });

  it("defines all concrete palettes and swatches", () => {
    expect(Object.keys(themeDefinitions)).toEqual(["light", "folio-dark", "black"]);
    expect(themeDefinitions.black.cssVars["--bg"]).toBe("#000000");
    expect(themeDefinitions["folio-dark"].cssVars["--bg"]).toBe("#0f0e0c");
    expect(themeDefinitions.light.mode).toBe("light");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
corepack pnpm --dir frontend/web test -- themes
```

Expected: FAIL because `frontend/web/src/theme/themes.ts` does not exist.

- [ ] **Step 3: Implement the theme model**

Create `frontend/web/src/theme/themes.ts`:

```ts
export type ThemePreference = "auto" | "light" | "folio-dark" | "black";
export type ResolvedTheme = "light" | "folio-dark" | "black";
export type ThemeMode = "light" | "dark";
export type SystemTheme = "light" | "dark";

export type ChartThemeDefinition = {
  background: string;
  text: string;
  grid: string;
  series: {
    sma20: string;
    sma30: string;
    sma50: string;
    sma60: string;
    sma90: string;
    sma200: string;
    ema20: string;
    ema30: string;
    ema50: string;
    ema60: string;
    ema90: string;
    ema200: string;
    bollUpper: string;
    bollMiddle: string;
    bollLower: string;
    donchianUpper: string;
    donchianLower: string;
    equity: string;
    equityTop: string;
    equityBottom: string;
    drawdown: string;
    drawdownTop: string;
    drawdownBottom: string;
    candleUp: string;
    candleDown: string;
    positionLong: string;
    positionShort: string;
    markerBuy: string;
    markerSell: string;
    markerVeto: string;
    markerHold: string;
    rsi: string;
    guide: string;
    macdLine: string;
    macdSignal: string;
    macdHistogram: string;
    atr: string;
  };
};

export type ThemeDefinition = {
  id: ResolvedTheme;
  label: string;
  mode: ThemeMode;
  metaColor: string;
  swatch: {
    bg: string;
    surface: string;
    border: string;
    text: string;
    accent: string;
  };
  cssVars: Record<string, string>;
  chart: ChartThemeDefinition;
};

export const THEME_PREFERENCE_KEY = "xvn.theme.preference";
export const THEME_DARK_KEY = "xvn.theme.dark";

export const themePreferenceOptions: { value: ThemePreference; label: string }[] = [
  { value: "auto", label: "Auto" },
  { value: "light", label: "Light" },
  { value: "folio-dark", label: "Folio dark" },
  { value: "black", label: "Black" },
];

export const themeDefinitions: Record<ResolvedTheme, ThemeDefinition> = {
  light: {
    id: "light",
    label: "Light",
    mode: "light",
    metaColor: "#f7f5ef",
    swatch: { bg: "#f7f5ef", surface: "#fffaf0", border: "#d8d0c2", text: "#201d18", accent: "#8a5f16" },
    cssVars: {
      "--bg": "#f7f5ef",
      "--surface-sidebar": "#ece6d8",
      "--surface-card": "#fffaf0",
      "--surface-elev": "#f3ecdd",
      "--surface-panel": "#e8dfce",
      "--surface-hover": "#eee5d5",
      "--border": "#d8d0c2",
      "--border-strong": "#b9ad99",
      "--border-soft": "#e2d9c8",
      "--text": "#201d18",
      "--text-2": "#5f584b",
      "--text-3": "#817767",
      "--text-4": "#a89d8a",
      "--gold": "#8a5f16",
      "--gold-soft": "#a87922",
      "--gold-bg": "rgba(138, 95, 22, 0.10)",
      "--gold-bg-strong": "rgba(138, 95, 22, 0.18)",
      "--warn": "#a65f00",
      "--danger": "#b42318",
      "--info": "#2563a8",
    },
    chart: {
      background: "#fffaf0",
      text: "#201d18",
      grid: "#e2d9c8",
      series: {
        sma20: "#0284c7", sma30: "#047857", sma50: "#a16207", sma60: "#059669", sma90: "#16a34a", sma200: "#b91c1c",
        ema20: "#7c3aed", ema30: "#0369a1", ema50: "#a16207", ema60: "#0284c7", ema90: "#0ea5e9", ema200: "#b91c1c",
        bollUpper: "#15803d", bollMiddle: "#64748b", bollLower: "#15803d",
        donchianUpper: "#c2410c", donchianLower: "#c2410c",
        equity: "#0891b2", equityTop: "rgba(8,145,178,0.24)", equityBottom: "rgba(8,145,178,0)",
        drawdown: "#dc2626", drawdownTop: "rgba(220,38,38,0.22)", drawdownBottom: "rgba(220,38,38,0)",
        candleUp: "#16a34a", candleDown: "#dc2626",
        positionLong: "rgba(34,197,94,0.10)", positionShort: "rgba(239,68,68,0.10)",
        markerBuy: "#16a34a", markerSell: "#dc2626", markerVeto: "#ca8a04", markerHold: "#475569",
        rsi: "#7c3aed", guide: "#94a3b8", macdLine: "#0891b2", macdSignal: "#ea580c", macdHistogram: "#64748b", atr: "#a16207",
      },
    },
  },
  "folio-dark": {
    id: "folio-dark",
    label: "Folio dark",
    mode: "dark",
    metaColor: "#0f0e0c",
    swatch: { bg: "#0f0e0c", surface: "#14120e", border: "#2a2618", text: "#f1ecdd", accent: "#d4a547" },
    cssVars: {
      "--bg": "#0f0e0c",
      "--surface-sidebar": "#17150f",
      "--surface-card": "#14120e",
      "--surface-elev": "#1b1810",
      "--surface-panel": "#221e14",
      "--surface-hover": "#1f1c13",
      "--border": "#2a2618",
      "--border-strong": "#3a3322",
      "--border-soft": "#221f15",
      "--text": "#f1ecdd",
      "--text-2": "#a39a85",
      "--text-3": "#6b6553",
      "--text-4": "#4a4536",
      "--gold": "#d4a547",
      "--gold-soft": "#b8862e",
      "--gold-bg": "rgba(212, 165, 71, 0.1)",
      "--gold-bg-strong": "rgba(212, 165, 71, 0.18)",
      "--warn": "#db9230",
      "--danger": "#c8443a",
      "--info": "#6f8fb8",
    },
    chart: {
      background: "#14120e",
      text: "#f1ecdd",
      grid: "#2a2618",
      series: {
        sma20: "#7dd3fc", sma30: "#a7f3d0", sma50: "#fbbf24", sma60: "#6ee7b7", sma90: "#34d399", sma200: "#f87171",
        ema20: "#a78bfa", ema30: "#bae6fd", ema50: "#fbbf24", ema60: "#7dd3fc", ema90: "#38bdf8", ema200: "#f87171",
        bollUpper: "#34d399", bollMiddle: "#94a3b8", bollLower: "#34d399",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
        equity: "#22d3ee", equityTop: "rgba(34,211,238,0.30)", equityBottom: "rgba(34,211,238,0)",
        drawdown: "#ef4444", drawdownTop: "rgba(239,68,68,0.30)", drawdownBottom: "rgba(239,68,68,0)",
        candleUp: "#22c55e", candleDown: "#ef4444",
        positionLong: "rgba(34,197,94,0.08)", positionShort: "rgba(239,68,68,0.08)",
        markerBuy: "#22c55e", markerSell: "#ef4444", markerVeto: "#facc15", markerHold: "#94a3b8",
        rsi: "#a78bfa", guide: "#475569", macdLine: "#22d3ee", macdSignal: "#f97316", macdHistogram: "#94a3b8", atr: "#fbbf24",
      },
    },
  },
  black: {
    id: "black",
    label: "Black",
    mode: "dark",
    metaColor: "#000000",
    swatch: { bg: "#000000", surface: "#080808", border: "#202020", text: "#f5f5f5", accent: "#f0c75e" },
    cssVars: {
      "--bg": "#000000",
      "--surface-sidebar": "#050505",
      "--surface-card": "#080808",
      "--surface-elev": "#101010",
      "--surface-panel": "#151515",
      "--surface-hover": "#181818",
      "--border": "#202020",
      "--border-strong": "#343434",
      "--border-soft": "#171717",
      "--text": "#f5f5f5",
      "--text-2": "#b8b8b8",
      "--text-3": "#858585",
      "--text-4": "#5f5f5f",
      "--gold": "#f0c75e",
      "--gold-soft": "#c99b32",
      "--gold-bg": "rgba(240, 199, 94, 0.10)",
      "--gold-bg-strong": "rgba(240, 199, 94, 0.18)",
      "--warn": "#e0a03a",
      "--danger": "#e05249",
      "--info": "#7aa7d9",
    },
    chart: {
      background: "#000000",
      text: "#f5f5f5",
      grid: "#1f1f1f",
      series: {
        sma20: "#7dd3fc", sma30: "#99f6e4", sma50: "#f0c75e", sma60: "#5eead4", sma90: "#2dd4bf", sma200: "#fb7185",
        ema20: "#c4b5fd", ema30: "#bae6fd", ema50: "#f0c75e", ema60: "#67e8f9", ema90: "#22d3ee", ema200: "#fb7185",
        bollUpper: "#4ade80", bollMiddle: "#a3a3a3", bollLower: "#4ade80",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
        equity: "#22d3ee", equityTop: "rgba(34,211,238,0.26)", equityBottom: "rgba(34,211,238,0)",
        drawdown: "#f87171", drawdownTop: "rgba(248,113,113,0.26)", drawdownBottom: "rgba(248,113,113,0)",
        candleUp: "#22c55e", candleDown: "#ef4444",
        positionLong: "rgba(34,197,94,0.09)", positionShort: "rgba(239,68,68,0.09)",
        markerBuy: "#22c55e", markerSell: "#ef4444", markerVeto: "#facc15", markerHold: "#a3a3a3",
        rsi: "#c4b5fd", guide: "#525252", macdLine: "#22d3ee", macdSignal: "#fb923c", macdHistogram: "#a3a3a3", atr: "#f0c75e",
      },
    },
  },
};

export function coerceThemePreference(value: string | null): ThemePreference {
  return value === "auto" || value === "light" || value === "folio-dark" || value === "black"
    ? value
    : "folio-dark";
}

export function coerceDarkTheme(value: string | null): Extract<ResolvedTheme, "folio-dark" | "black"> {
  return value === "black" ? "black" : "folio-dark";
}

export function resolveTheme(preference: ThemePreference, systemTheme: SystemTheme): ResolvedTheme {
  if (preference === "auto") return systemTheme === "light" ? "light" : "folio-dark";
  return preference;
}
```

- [ ] **Step 4: Run tests to verify green**

Run:

```bash
corepack pnpm --dir frontend/web test -- themes
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/theme/themes.ts frontend/web/src/theme/themes.test.ts
git commit -m "feat: add dashboard theme model"
```

## Task 2: Theme Store, Provider, and CSS Tokens

**Files:**
- Create: `frontend/web/src/theme/useTheme.ts`
- Create: `frontend/web/src/theme/useTheme.test.tsx`
- Create: `frontend/web/src/theme/ThemeProvider.tsx`
- Modify: `frontend/web/src/App.tsx`
- Modify: `frontend/web/src/styles/tokens.css`
- Modify: `frontend/web/index.html`

- [ ] **Step 1: Write failing provider/store tests**

Add `frontend/web/src/theme/useTheme.test.tsx`:

```tsx
import { afterEach, describe, expect, it, vi } from "vitest";
import { render, screen, fireEvent, cleanup } from "@testing-library/react";
import { ThemeProvider } from "./ThemeProvider";
import { THEME_DARK_KEY, THEME_PREFERENCE_KEY } from "./themes";
import { useTheme } from "./useTheme";

function installMatchMedia(matches: boolean) {
  const listeners = new Set<(event: MediaQueryListEvent) => void>();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: vi.fn().mockImplementation(() => ({
      matches,
      media: "(prefers-color-scheme: dark)",
      onchange: null,
      addEventListener: (_: "change", cb: (event: MediaQueryListEvent) => void) => listeners.add(cb),
      removeEventListener: (_: "change", cb: (event: MediaQueryListEvent) => void) => listeners.delete(cb),
      dispatch(next: boolean) {
        listeners.forEach((cb) => cb({ matches: next } as MediaQueryListEvent));
      },
    })),
  });
}

function Probe() {
  const { preference, resolvedTheme, setPreference, setLightTheme, setDarkTheme } = useTheme();
  return (
    <div>
      <div data-testid="preference">{preference}</div>
      <div data-testid="resolved">{resolvedTheme}</div>
      <button onClick={() => setPreference("black")}>Black</button>
      <button onClick={() => setPreference("auto")}>Auto</button>
      <button onClick={setLightTheme}>Sun</button>
      <button onClick={setDarkTheme}>Moon</button>
    </div>
  );
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
  document.querySelector('meta[name="theme-color"]')?.setAttribute("content", "#0F0E0C");
  vi.restoreAllMocks();
});

describe("ThemeProvider", () => {
  it("defaults to folio dark and applies DOM attributes", () => {
    installMatchMedia(true);
    render(<ThemeProvider><Probe /></ThemeProvider>);
    expect(screen.getByTestId("preference")).toHaveTextContent("folio-dark");
    expect(document.documentElement.dataset.theme).toBe("folio-dark");
    expect(document.documentElement.classList.contains("dark")).toBe(true);
  });

  it("persists explicit preferences and remembers black as dark theme", () => {
    installMatchMedia(true);
    render(<ThemeProvider><Probe /></ThemeProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Black" }));
    expect(localStorage.getItem(THEME_PREFERENCE_KEY)).toBe("black");
    expect(localStorage.getItem(THEME_DARK_KEY)).toBe("black");
    expect(document.documentElement.dataset.theme).toBe("black");
  });

  it("uses sidebar sun and moon actions", () => {
    installMatchMedia(true);
    localStorage.setItem(THEME_DARK_KEY, "black");
    render(<ThemeProvider><Probe /></ThemeProvider>);
    fireEvent.click(screen.getByRole("button", { name: "Sun" }));
    expect(screen.getByTestId("resolved")).toHaveTextContent("light");
    fireEvent.click(screen.getByRole("button", { name: "Moon" }));
    expect(screen.getByTestId("resolved")).toHaveTextContent("black");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
corepack pnpm --dir frontend/web test -- useTheme
```

Expected: FAIL because `useTheme.ts` and `ThemeProvider.tsx` do not exist.

- [ ] **Step 3: Implement theme hook/store**

Create `frontend/web/src/theme/useTheme.ts`:

```ts
import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";
import {
  coerceDarkTheme,
  coerceThemePreference,
  resolveTheme,
  THEME_DARK_KEY,
  THEME_PREFERENCE_KEY,
  themeDefinitions,
  type ResolvedTheme,
  type SystemTheme,
  type ThemePreference,
} from "./themes";

type Snapshot = {
  preference: ThemePreference;
  darkTheme: Extract<ResolvedTheme, "folio-dark" | "black">;
  systemTheme: SystemTheme;
};

const listeners = new Set<() => void>();

function getSystemTheme(): SystemTheme {
  return typeof window !== "undefined" && window.matchMedia?.("(prefers-color-scheme: dark)").matches
    ? "dark"
    : "light";
}

function getSnapshot(): Snapshot {
  return {
    preference: coerceThemePreference(safeStorageGet(THEME_PREFERENCE_KEY)),
    darkTheme: coerceDarkTheme(safeStorageGet(THEME_DARK_KEY)),
    systemTheme: getSystemTheme(),
  };
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function emit() {
  listeners.forEach((listener) => listener());
}

export function setThemePreference(preference: ThemePreference) {
  safeStorageSet(THEME_PREFERENCE_KEY, preference);
  if (preference === "folio-dark" || preference === "black") {
    safeStorageSet(THEME_DARK_KEY, preference);
  }
  emit();
}

export function useTheme() {
  const snapshot = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const resolvedTheme = resolveTheme(snapshot.preference, snapshot.systemTheme);
  const definition = themeDefinitions[resolvedTheme];

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!query) return;
    const onChange = () => emit();
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const setPreference = useCallback((preference: ThemePreference) => {
    setThemePreference(preference);
  }, []);

  const setLightTheme = useCallback(() => setThemePreference("light"), []);
  const setDarkTheme = useCallback(() => setThemePreference(getSnapshot().darkTheme), []);

  return useMemo(
    () => ({
      preference: snapshot.preference,
      darkTheme: snapshot.darkTheme,
      resolvedTheme,
      definition,
      setPreference,
      setLightTheme,
      setDarkTheme,
    }),
    [definition, resolvedTheme, setDarkTheme, setLightTheme, setPreference, snapshot.darkTheme, snapshot.preference],
  );
}
```

- [ ] **Step 4: Implement ThemeProvider**

Create `frontend/web/src/theme/ThemeProvider.tsx`:

```tsx
import { useEffect, type ReactNode } from "react";
import { useTheme } from "./useTheme";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { resolvedTheme, definition } = useTheme();

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.classList.toggle("dark", definition.mode === "dark");
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", definition.metaColor);
  }, [definition.metaColor, definition.mode, resolvedTheme]);

  return <>{children}</>;
}
```

Modify `frontend/web/src/App.tsx`:

```tsx
import { RouterProvider } from "react-router-dom";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { router } from "./routes";
import { ThemeProvider } from "./theme/ThemeProvider";

const queryClient = new QueryClient({
  defaultOptions: {
    queries: {
      staleTime: 30_000,
      retry: 1,
      refetchOnWindowFocus: false,
    },
  },
});

export function App() {
  return (
    <ThemeProvider>
      <QueryClientProvider client={queryClient}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </ThemeProvider>
  );
}
```

- [ ] **Step 5: Add token scopes**

Modify `frontend/web/src/styles/tokens.css` to keep the current `:root` block as the fallback and append:

```css
[data-theme="folio-dark"] {
  /* same values as :root */
}

[data-theme="black"] {
  --bg: #000000;
  --surface-sidebar: #050505;
  --surface-card: #080808;
  --surface-elev: #101010;
  --surface-panel: #151515;
  --surface-hover: #181818;
  --border: #202020;
  --border-strong: #343434;
  --border-soft: #171717;
  --text: #f5f5f5;
  --text-2: #b8b8b8;
  --text-3: #858585;
  --text-4: #5f5f5f;
  --gold: #f0c75e;
  --gold-soft: #c99b32;
  --gold-bg: rgba(240, 199, 94, 0.1);
  --gold-bg-strong: rgba(240, 199, 94, 0.18);
  --warn: #e0a03a;
  --danger: #e05249;
  --info: #7aa7d9;
}

[data-theme="light"] {
  --bg: #f7f5ef;
  --surface-sidebar: #ece6d8;
  --surface-card: #fffaf0;
  --surface-elev: #f3ecdd;
  --surface-panel: #e8dfce;
  --surface-hover: #eee5d5;
  --border: #d8d0c2;
  --border-strong: #b9ad99;
  --border-soft: #e2d9c8;
  --text: #201d18;
  --text-2: #5f584b;
  --text-3: #817767;
  --text-4: #a89d8a;
  --gold: #8a5f16;
  --gold-soft: #a87922;
  --gold-bg: rgba(138, 95, 22, 0.1);
  --gold-bg-strong: rgba(138, 95, 22, 0.18);
  --warn: #a65f00;
  --danger: #b42318;
  --info: #2563a8;
}
```

Remove the hardcoded permanent `class="dark"` from `frontend/web/index.html`. Keep the meta theme color; the provider updates it after startup.

- [ ] **Step 6: Run tests to verify green**

Run:

```bash
corepack pnpm --dir frontend/web test -- useTheme themes
```

Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add frontend/web/src/theme frontend/web/src/App.tsx frontend/web/src/styles/tokens.css frontend/web/index.html
git commit -m "feat: apply dashboard theme preference"
```

## Task 3: General Settings Route

**Files:**
- Create: `frontend/web/src/routes/settings/general.tsx`
- Create: `frontend/web/src/routes/settings/general.test.tsx`
- Modify: `frontend/web/src/routes.tsx`
- Modify: `frontend/web/src/routes/settings/index.tsx`
- Modify: `frontend/web/src/routes/settings/settings-layout.test.tsx`

- [ ] **Step 1: Write failing route and settings tests**

Add `frontend/web/src/routes/settings/general.test.tsx`:

```tsx
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { ThemeProvider } from "@/theme/ThemeProvider";
import { SettingsGeneralRoute } from "./general";

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
});

describe("SettingsGeneralRoute", () => {
  it("renders all appearance choices and persists selection", () => {
    render(
      <ThemeProvider>
        <SettingsGeneralRoute />
      </ThemeProvider>,
    );

    expect(screen.getByRole("heading", { name: "Appearance" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Auto" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Light" })).toBeInTheDocument();
    expect(screen.getByRole("radio", { name: "Folio dark" })).toBeChecked();
    expect(screen.getByRole("radio", { name: "Black" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("radio", { name: "Black" }));
    expect(document.documentElement.dataset.theme).toBe("black");
  });
});
```

Update `frontend/web/src/routes/settings/settings-layout.test.tsx` so it expects the General tab:

```tsx
expect(screen.getByRole("link", { name: "General" })).toBeInTheDocument();
expect(screen.queryByRole("link", { name: "Skills" })).not.toBeInTheDocument();
expect(screen.getByRole("link", { name: "Providers" })).toBeInTheDocument();
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
corepack pnpm --dir frontend/web test -- settings-layout general
```

Expected: FAIL because `SettingsGeneralRoute` does not exist and the General tab is not present.

- [ ] **Step 3: Implement General settings UI**

Create `frontend/web/src/routes/settings/general.tsx`:

```tsx
import { Card } from "@/components/primitives/Card";
import { themeDefinitions, themePreferenceOptions, type ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";

function swatchFor(value: string) {
  const id: ResolvedTheme = value === "auto" ? "folio-dark" : (value as ResolvedTheme);
  return themeDefinitions[id].swatch;
}

export function SettingsGeneralRoute() {
  const { preference, setPreference } = useTheme();

  return (
    <Card className="p-5">
      <div className="mb-4">
        <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
          Appearance
        </h3>
        <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
          Choose the dashboard color theme. This changes colors only.
        </p>
      </div>
      <div role="radiogroup" aria-label="Theme preference" className="grid gap-2 sm:grid-cols-2 xl:grid-cols-4">
        {themePreferenceOptions.map((option) => {
          const swatch = swatchFor(option.value);
          const checked = preference === option.value;
          return (
            <label
              key={option.value}
              className={[
                "flex items-center gap-3 rounded border px-3 py-2 cursor-pointer transition-colors",
                checked
                  ? "border-gold bg-gold/[0.08] text-text"
                  : "border-border bg-surface-elev text-text-2 hover:border-text-3 hover:text-text",
              ].join(" ")}
            >
              <input
                type="radio"
                name="theme"
                value={option.value}
                checked={checked}
                onChange={() => setPreference(option.value)}
                className="sr-only"
              />
              <span
                aria-hidden
                className="grid h-7 w-7 grid-cols-2 overflow-hidden rounded border border-border"
                style={{ background: swatch.bg }}
              >
                <span style={{ background: swatch.surface }} />
                <span style={{ background: swatch.accent }} />
                <span style={{ background: swatch.border }} />
                <span style={{ background: swatch.text }} />
              </span>
              <span className="text-[13px]">{option.label}</span>
            </label>
          );
        })}
      </div>
    </Card>
  );
}
```

Modify `frontend/web/src/routes/settings/index.tsx`:

```tsx
const TABS = [
  { to: "general", label: "General" },
  { to: "providers", label: "Providers" },
  { to: "brokers", label: "Brokers" },
  { to: "danger", label: "Danger zone" },
];

export { SettingsGeneralRoute } from "./general";
```

Modify `frontend/web/src/routes.tsx`:

```tsx
const SettingsGeneralRoute = lazy(() => import("./routes/settings").then((m) => ({ default: m.SettingsGeneralRoute })));
```

Inside the settings children:

```tsx
{ index: true, element: <Navigate to="general" replace /> },
{ path: "general", element: page(<SettingsGeneralRoute />) },
```

- [ ] **Step 4: Run tests to verify green**

Run:

```bash
corepack pnpm --dir frontend/web test -- settings-layout general
```

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add frontend/web/src/routes.tsx frontend/web/src/routes/settings/index.tsx frontend/web/src/routes/settings/general.tsx frontend/web/src/routes/settings/general.test.tsx frontend/web/src/routes/settings/settings-layout.test.tsx
git commit -m "feat: add general theme settings"
```

## Task 4: Sidebar Sun/Moon Toggle

**Files:**
- Modify: `frontend/web/src/components/primitives/Icon.tsx`
- Modify: `frontend/web/src/components/shell/Sidebar.tsx`
- Create: `frontend/web/src/components/shell/Sidebar.test.tsx`

- [ ] **Step 1: Write failing sidebar tests**

Add `frontend/web/src/components/shell/Sidebar.test.tsx`:

```tsx
import { afterEach, describe, expect, it } from "vitest";
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router-dom";
import { Sidebar } from "./Sidebar";
import { ThemeProvider } from "@/theme/ThemeProvider";
import { THEME_DARK_KEY } from "@/theme/themes";

function renderSidebar() {
  return render(
    <ThemeProvider>
      <MemoryRouter>
        <Sidebar />
      </MemoryRouter>
    </ThemeProvider>,
  );
}

afterEach(() => {
  cleanup();
  localStorage.clear();
  document.documentElement.removeAttribute("data-theme");
  document.documentElement.className = "";
});

describe("Sidebar theme toggle", () => {
  it("switches to light with the sun button", () => {
    renderSidebar();
    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    expect(document.documentElement.dataset.theme).toBe("light");
  });

  it("switches to the remembered dark theme with the moon button", () => {
    localStorage.setItem(THEME_DARK_KEY, "black");
    renderSidebar();
    fireEvent.click(screen.getByRole("button", { name: "Switch to light theme" }));
    fireEvent.click(screen.getByRole("button", { name: "Switch to dark theme" }));
    expect(document.documentElement.dataset.theme).toBe("black");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
corepack pnpm --dir frontend/web test -- Sidebar
```

Expected: FAIL because the sidebar buttons and icons do not exist.

- [ ] **Step 3: Add sun/moon icons**

Modify `frontend/web/src/components/primitives/Icon.tsx`:

```tsx
export type IconName =
  | "home"
  // existing entries
  | "sliders"
  | "sun"
  | "moon";
```

Add paths:

```tsx
sun: (
  <>
    <circle cx="10" cy="10" r="3" />
    <path d="M10 2.5v2M10 15.5v2M4.7 4.7l1.4 1.4M13.9 13.9l1.4 1.4M2.5 10h2M15.5 10h2M4.7 15.3l1.4-1.4M13.9 6.1l1.4-1.4" />
  </>
),
moon: <path d="M14.5 13.8A6.5 6.5 0 016.2 5.5 6.5 6.5 0 1014.5 13.8z" />,
```

- [ ] **Step 4: Add sidebar quick controls**

Modify `frontend/web/src/components/shell/Sidebar.tsx`:

```tsx
import { NavLink } from "react-router-dom";
import { Icon, type IconName } from "@/components/primitives/Icon";
import { useTheme } from "@/theme/useTheme";
```

Inside `Sidebar()`:

```tsx
const { resolvedTheme, setLightTheme, setDarkTheme } = useTheme();
const isLight = resolvedTheme === "light";
```

Add above the profile block:

```tsx
<div className="mx-4 mb-3 flex items-center gap-1 rounded border border-border-soft bg-surface-elev p-1">
  <button
    type="button"
    onClick={setLightTheme}
    aria-label="Switch to light theme"
    className={[
      "flex h-7 flex-1 items-center justify-center rounded text-text-3 transition-colors hover:text-text",
      isLight ? "bg-gold/[0.12] text-gold" : "",
    ].join(" ")}
  >
    <Icon name="sun" size={15} />
  </button>
  <button
    type="button"
    onClick={setDarkTheme}
    aria-label="Switch to dark theme"
    className={[
      "flex h-7 flex-1 items-center justify-center rounded text-text-3 transition-colors hover:text-text",
      !isLight ? "bg-gold/[0.12] text-gold" : "",
    ].join(" ")}
  >
    <Icon name="moon" size={15} />
  </button>
</div>
```

- [ ] **Step 5: Run tests to verify green**

Run:

```bash
corepack pnpm --dir frontend/web test -- Sidebar
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/components/primitives/Icon.tsx frontend/web/src/components/shell/Sidebar.tsx frontend/web/src/components/shell/Sidebar.test.tsx
git commit -m "feat: add sidebar theme toggle"
```

## Task 5: Chart Theme Integration

**Files:**
- Modify: `frontend/web/src/components/chart/chart-theme.ts`
- Modify: `frontend/web/src/components/chart/RunChart.tsx`
- Modify: `frontend/web/src/components/chart/CompareChart.tsx`
- Modify: `frontend/web/src/components/chart/ScenarioChart.tsx`
- Modify: `frontend/web/src/components/chart/StrategyChart.tsx`
- Modify: `frontend/web/src/components/chart/LiveChart.tsx`
- Modify tests: `RunChart.test.tsx`, `ScenarioChart.test.tsx`, `LiveChart.test.tsx`

- [ ] **Step 1: Write failing chart theme tests**

Update `frontend/web/src/components/chart/RunChart.test.tsx` with a focused assertion around black mode by rerendering with `theme="black"` or through `ThemeProvider`, matching the final prop shape.

Add direct tests to `frontend/web/src/components/chart/chart-theme.ts` by creating `frontend/web/src/components/chart/chart-theme.test.ts`:

```ts
import { describe, expect, it } from "vitest";
import { chartTheme } from "./chart-theme";

describe("chartTheme", () => {
  it("uses black chart surfaces for black theme", () => {
    expect(chartTheme("black").background).toBe("#000000");
    expect(chartTheme("black").grid).toBe("#1f1f1f");
  });

  it("keeps folio dark as the dark default", () => {
    expect(chartTheme("folio-dark").background).toBe("#14120e");
  });

  it("supports light chart colors", () => {
    expect(chartTheme("light").text).toBe("#201d18");
  });
});
```

- [ ] **Step 2: Run tests to verify they fail**

Run:

```bash
corepack pnpm --dir frontend/web test -- chart-theme RunChart ScenarioChart LiveChart
```

Expected: FAIL because `chartTheme` still accepts only `"dark" | "light"` and chart components do not use the app theme.

- [ ] **Step 3: Update chart-theme**

Replace `frontend/web/src/components/chart/chart-theme.ts` with:

```ts
import { themeDefinitions, type ResolvedTheme } from "@/theme/themes";

export function chartTheme(theme: ResolvedTheme = "folio-dark") {
  return themeDefinitions[theme].chart;
}
```

- [ ] **Step 4: Update chart components**

For each chart component, replace `themeMode?: "dark" | "light"` with:

```ts
import type { ResolvedTheme } from "@/theme/themes";
import { useTheme } from "@/theme/useTheme";

type Props = {
  // existing props
  theme?: ResolvedTheme;
};
```

Inside the component:

```ts
const appTheme = useTheme();
const resolvedChartTheme = theme ?? appTheme.resolvedTheme;
const palette = chartTheme(resolvedChartTheme);
```

Replace hardcoded chart colors in `RunChart.tsx` with `palette.series.*`, including `sma30`, `sma60`, `sma90`, `ema30`, `ema60`, `ema90`, `rsi`, `guide`, `macdLine`, `macdSignal`, `macdHistogram`, `atr`, `equityTop`, `equityBottom`, `drawdownTop`, and `drawdownBottom`.

Update `LiveChart.tsx` to pass `theme={theme}` instead of `themeMode={themeMode}` if the explicit prop remains for tests.

- [ ] **Step 5: Run tests to verify green**

Run:

```bash
corepack pnpm --dir frontend/web test -- chart-theme RunChart ScenarioChart LiveChart
```

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add frontend/web/src/components/chart
git commit -m "feat: connect charts to app themes"
```

## Task 6: Dark-Class Audit and Final Verification

**Files:**
- Modify as needed: `frontend/web/src/routes/setup.tsx`
- Modify as needed: `frontend/web/src/routes/settings/danger.tsx`
- Modify as needed: `frontend/web/src/components/shell/ChatRail.tsx`
- Modify as needed: tests covering changed files

- [ ] **Step 1: Locate dark-only and hardcoded status colors**

Run:

```bash
rg "dark:|text-rose|text-blue|text-emerald|bg-blue|border-blue" frontend/web/src
```

Expected: list known areas from the spec.

- [ ] **Step 2: Convert theme-sensitive colors to tokens**

For color usages that represent semantic states and must work in Light and Black themes, prefer existing tokens:

```tsx
"text-danger"
"text-info"
"text-warn"
"bg-danger/10"
"bg-info/10"
"border-danger/40"
"border-info/30"
```

Leave non-theme behavioral classes untouched when the existing token mapping already handles the concrete theme.

- [ ] **Step 3: Run focused tests**

Run:

```bash
corepack pnpm --dir frontend/web test -- theme settings-layout general Sidebar RunChart ScenarioChart LiveChart ChatRail setup
```

Expected: PASS.

- [ ] **Step 4: Run typecheck**

Run:

```bash
corepack pnpm --dir frontend/web typecheck
```

Expected: PASS.

- [ ] **Step 5: Run full frontend test suite if focused tests pass**

Run:

```bash
corepack pnpm --dir frontend/web test
```

Expected: PASS.

- [ ] **Step 6: Commit audit fixes**

```bash
git add frontend/web/src
git commit -m "fix: audit theme-sensitive color classes"
```

## Self-Review Checklist

- Spec coverage: Tasks cover preference state, CSS tokens, General settings, sidebar toggle, chart palette integration, dark-class audit, persistence, and tests.
- Placeholder scan: This plan contains no placeholder tasks; each implementation step names files, code shape, and commands.
- Type consistency: `ThemePreference`, `ResolvedTheme`, `ThemeMode`, and `ChartThemeDefinition` are defined once in `frontend/web/src/theme/themes.ts` and reused by hooks, settings, sidebar, and charts.
