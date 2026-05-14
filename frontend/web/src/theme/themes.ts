export type ThemePreference = "auto" | "light" | "folio-dark" | "black";
export type ResolvedTheme = "light" | "folio-dark" | "black";
type ThemeMode = "light" | "dark";
export type SystemTheme = "light" | "dark";

type ChartThemeDefinition = {
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

export const themePreferenceOptions: { value: ThemePreference; label: string }[] =
  [
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
    swatch: {
      bg: "#f7f5ef",
      surface: "#fffaf0",
      border: "#d8d0c2",
      text: "#201d18",
      accent: "#8a5f16",
    },
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
      "--gold-bg": "rgba(138, 95, 22, 0.1)",
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
        sma20: "#0284c7",
        sma30: "#047857",
        sma50: "#a16207",
        sma60: "#059669",
        sma90: "#16a34a",
        sma200: "#b91c1c",
        ema20: "#7c3aed",
        ema30: "#0369a1",
        ema50: "#a16207",
        ema60: "#0284c7",
        ema90: "#0ea5e9",
        ema200: "#b91c1c",
        bollUpper: "#15803d",
        bollMiddle: "#64748b",
        bollLower: "#15803d",
        donchianUpper: "#c2410c",
        donchianLower: "#c2410c",
        equity: "#0891b2",
        equityTop: "rgba(8, 145, 178, 0.24)",
        equityBottom: "rgba(8, 145, 178, 0)",
        drawdown: "#dc2626",
        drawdownTop: "rgba(220, 38, 38, 0.22)",
        drawdownBottom: "rgba(220, 38, 38, 0)",
        candleUp: "#16a34a",
        candleDown: "#dc2626",
        positionLong: "rgba(34, 197, 94, 0.1)",
        positionShort: "rgba(239, 68, 68, 0.1)",
        markerBuy: "#16a34a",
        markerSell: "#dc2626",
        markerVeto: "#ca8a04",
        markerHold: "#475569",
        rsi: "#7c3aed",
        guide: "#94a3b8",
        macdLine: "#0891b2",
        macdSignal: "#ea580c",
        macdHistogram: "#64748b",
        atr: "#a16207",
      },
    },
  },
  "folio-dark": {
    id: "folio-dark",
    label: "Folio dark",
    mode: "dark",
    metaColor: "#0f0e0c",
    swatch: {
      bg: "#0f0e0c",
      surface: "#14120e",
      border: "#2a2618",
      text: "#f1ecdd",
      accent: "#d4a547",
    },
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
        sma20: "#7dd3fc",
        sma30: "#a7f3d0",
        sma50: "#fbbf24",
        sma60: "#6ee7b7",
        sma90: "#34d399",
        sma200: "#f87171",
        ema20: "#a78bfa",
        ema30: "#bae6fd",
        ema50: "#fbbf24",
        ema60: "#7dd3fc",
        ema90: "#38bdf8",
        ema200: "#f87171",
        bollUpper: "#34d399",
        bollMiddle: "#94a3b8",
        bollLower: "#34d399",
        donchianUpper: "#fb923c",
        donchianLower: "#fb923c",
        equity: "#22d3ee",
        equityTop: "rgba(34, 211, 238, 0.3)",
        equityBottom: "rgba(34, 211, 238, 0)",
        drawdown: "#ef4444",
        drawdownTop: "rgba(239, 68, 68, 0.3)",
        drawdownBottom: "rgba(239, 68, 68, 0)",
        candleUp: "#22c55e",
        candleDown: "#ef4444",
        positionLong: "rgba(34, 197, 94, 0.08)",
        positionShort: "rgba(239, 68, 68, 0.08)",
        markerBuy: "#22c55e",
        markerSell: "#ef4444",
        markerVeto: "#facc15",
        markerHold: "#94a3b8",
        rsi: "#a78bfa",
        guide: "#475569",
        macdLine: "#22d3ee",
        macdSignal: "#f97316",
        macdHistogram: "#94a3b8",
        atr: "#fbbf24",
      },
    },
  },
  black: {
    id: "black",
    label: "Black",
    mode: "dark",
    metaColor: "#000000",
    swatch: {
      bg: "#000000",
      surface: "#080808",
      border: "#202020",
      text: "#f5f5f5",
      accent: "#f0c75e",
    },
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
      "--gold-bg": "rgba(240, 199, 94, 0.1)",
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
        sma20: "#7dd3fc",
        sma30: "#99f6e4",
        sma50: "#f0c75e",
        sma60: "#5eead4",
        sma90: "#2dd4bf",
        sma200: "#fb7185",
        ema20: "#c4b5fd",
        ema30: "#bae6fd",
        ema50: "#f0c75e",
        ema60: "#67e8f9",
        ema90: "#22d3ee",
        ema200: "#fb7185",
        bollUpper: "#4ade80",
        bollMiddle: "#a3a3a3",
        bollLower: "#4ade80",
        donchianUpper: "#fb923c",
        donchianLower: "#fb923c",
        equity: "#22d3ee",
        equityTop: "rgba(34, 211, 238, 0.26)",
        equityBottom: "rgba(34, 211, 238, 0)",
        drawdown: "#f87171",
        drawdownTop: "rgba(248, 113, 113, 0.26)",
        drawdownBottom: "rgba(248, 113, 113, 0)",
        candleUp: "#22c55e",
        candleDown: "#ef4444",
        positionLong: "rgba(34, 197, 94, 0.09)",
        positionShort: "rgba(239, 68, 68, 0.09)",
        markerBuy: "#22c55e",
        markerSell: "#ef4444",
        markerVeto: "#facc15",
        markerHold: "#a3a3a3",
        rsi: "#c4b5fd",
        guide: "#525252",
        macdLine: "#22d3ee",
        macdSignal: "#fb923c",
        macdHistogram: "#a3a3a3",
        atr: "#f0c75e",
      },
    },
  },
};

export function coerceThemePreference(value: string | null): ThemePreference {
  return value === "auto" ||
    value === "light" ||
    value === "folio-dark" ||
    value === "black"
    ? value
    : "folio-dark";
}

export function coerceDarkTheme(
  value: string | null,
): Extract<ResolvedTheme, "folio-dark" | "black"> {
  return value === "black" ? "black" : "folio-dark";
}

export function resolveTheme(
  preference: ThemePreference,
  systemTheme: SystemTheme,
): ResolvedTheme {
  if (preference === "auto") {
    return systemTheme === "light" ? "light" : "folio-dark";
  }
  return preference;
}
