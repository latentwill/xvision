export type ThemePreference = "auto" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";
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

export type Chart2WarmPalette = {
  gold: string;
  amber: string;
  bronze: string;
  ember: string;
  copper: string;
  plum: string;
  teal: string;
  info: string;
  warn: string;
  danger: string;
};

export type Chart2StrategyRotationEntry = {
  id: string;
  name: string;
  short: string;
  color: string;
  kind: "Trend" | "Momentum" | "Reversion" | "Vol" | "Bench";
  dashed?: boolean;
};

export type Chart2HeatRampStop = { color: string; alpha: number };

export type Chart2HeatRamp = {
  scorching: Chart2HeatRampStop;
  hot: Chart2HeatRampStop;
  warm: Chart2HeatRampStop;
  cool: Chart2HeatRampStop;
  cold: Chart2HeatRampStop;
};

export type Chart2Typography = {
  fontSerif: string;
  fontSans: string;
  fontMono: string;
};

export type Chart2Radius = {
  card: string;
  sm: string;
};

export type Chart2ThemeDefinition = {
  surface: {
    bg: string;
    panelBg: string;
    gridStrong: string;
    gridSoft: string;
    axisText: string;
    axisTick: string;
    crosshair: string;
  };
  candle: {
    up: string;
    down: string;
    wickUp: string;
    wickDown: string;
    borderUp: string;
    borderDown: string;
  };
  overlay: {
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
  };
  marker: {
    buy: string;
    sell: string;
    veto: string;
    hold: string;
    halo: string;
    textOnAccent: string;
  };
  position: {
    longBand: string;
    shortBand: string;
    longLine: string;
    shortLine: string;
  };
  panes: {
    equity: string;
    equityFillTop: string;
    equityFillBottom: string;
    drawdown: string;
    drawdownFillTop: string;
    drawdownFillBottom: string;
    volumeUp: string;
    volumeDown: string;
    rsi: string;
    rsiGuide: string;
    macdLine: string;
    macdSignal: string;
    macdHist: string;
    atr: string;
  };
  compare: {
    palette: [string, string, string, string, string, string, string, string];
  };
  motion: {
    hoverMs: number;
    animMs: number;
  };
  density: {
    axisFont: string;
    axisGap: number;
    paneGap: number;
  };
  // Track B additions (chart-rework spec, 2026-05-23) — design tokens
  // mirrored verbatim from docs/design/trading-charts/XVN.zip →
  // design_handoff_charts/source/charts/chart-theme.css.
  warm: Chart2WarmPalette;
  strategyRotation: Chart2StrategyRotationEntry[];
  heatRamp: Chart2HeatRamp;
  typography: Chart2Typography;
  radius: Chart2Radius;
};

// Signal multi-hue rotation. Stable across themes; chosen mid-tone so each
// hue survives both pure-black and near-white backgrounds. The lead (`color`)
// of each strategy is Signal green; the rest are the chart palette hues.
export const CHART2_STRATEGY_ROTATION: Chart2StrategyRotationEntry[] = [
  { id: "fib", name: "Fibonacci Golden Cross", short: "Fib · GC", color: "#00C16A", kind: "Trend" },
  { id: "ema", name: "EMA Pullback", short: "EMA · 50/200", color: "#5EEAD4", kind: "Trend" },
  { id: "brk", name: "Breakout Retest", short: "BRK · 4h", color: "#FB923C", kind: "Momentum" },
  { id: "msw", name: "Momentum Swing", short: "MSW · 1d", color: "#A78BFA", kind: "Momentum" },
  { id: "mvr", name: "Mean Reversion AI", short: "MVR · 15m", color: "#22D3EE", kind: "Reversion" },
  { id: "vsc", name: "Volatility Scalper", short: "VSC · 5m", color: "#F472B6", kind: "Vol" },
  { id: "lqh", name: "Liquidation Hunter", short: "LQH · 1h", color: "#E0B341", kind: "Vol" },
  { id: "btc", name: "BTC Buy & Hold", short: "BTC · HOLD", color: "#6B7682", kind: "Bench", dashed: true },
];

// Heat ramp = loss/intensity ramp (monthly-returns "hot" cells). Negative
// returns ride this red ramp; positive returns use the green fill in the
// component. Unchanged hues — this is a semantic loss ramp, not the brand.
export const CHART2_HEAT_RAMP: Chart2HeatRamp = {
  scorching: { color: "#FF6B5C", alpha: 0.48 },
  hot: { color: "#E04A3A", alpha: 0.42 },
  warm: { color: "#A93428", alpha: 0.36 },
  cool: { color: "#6A2A22", alpha: 0.30 },
  cold: { color: "#3A1E1A", alpha: 0.22 },
};

export const CHART2_TYPOGRAPHY: Chart2Typography = {
  fontSerif: "'Geist', sans-serif",
  fontSans: "'Geist', sans-serif",
  fontMono: "'Geist Mono', ui-monospace, monospace",
};

export const CHART2_RADIUS: Chart2Radius = {
  card: "6px",
  sm: "4px",
};

// Signal chart palette — dark surfaces. `warm` is a legacy property name
// kept for consumer compatibility; values are the Signal multi-hue rotation.
export const CHART2_SIGNAL_DARK: Chart2WarmPalette = {
  gold: "#00E676",
  amber: "#38BDF8",
  bronze: "#5EEAD4",
  ember: "#FB923C",
  copper: "#F472B6",
  plum: "#A78BFA",
  teal: "#22D3EE",
  info: "#5FA8FF",
  warn: "#FFB020",
  danger: "#FF4D4D",
};

// Signal chart palette — light surfaces (darker, legible on white).
export const CHART2_SIGNAL_LIGHT: Chart2WarmPalette = {
  gold: "#00A15C",
  amber: "#0284C7",
  bronze: "#0D9488",
  ember: "#EA580C",
  copper: "#DB2777",
  plum: "#7C3AED",
  teal: "#0891B2",
  info: "#1D6FD6",
  warn: "#B45309",
  danger: "#D92D20",
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
  chart2: Chart2ThemeDefinition;
};

export const THEME_PREFERENCE_KEY = "xvn.theme.preference";
export const ACCENT_PREFERENCE_KEY = "xvn.accent.preference";

export type AccentKey = "green" | "azure" | "cyan" | "teal" | "amber" | "magenta" | "mono";

export const ACCENT_PRESETS: Record<
  AccentKey,
  { label: string; dark: string; light: string; onAccent: string }
> = {
  green:   { label: "Green",   dark: "#00e676", light: "#00a15c", onAccent: "#000000" },
  azure:   { label: "Azure",   dark: "#3B82F6", light: "#2563EB", onAccent: "#ffffff" },
  cyan:    { label: "Cyan",    dark: "#22D3EE", light: "#0E96B3", onAccent: "#000000" },
  teal:    { label: "Teal",    dark: "#14C8AE", light: "#0D9488", onAccent: "#000000" },
  amber:   { label: "Amber",   dark: "#F5A524", light: "#B7770C", onAccent: "#000000" },
  magenta: { label: "Magenta", dark: "#D946EF", light: "#A21CAF", onAccent: "#ffffff" },
  mono:    { label: "Mono",    dark: "#9ca3af", light: "#6b7280", onAccent: "#000000" },
};

export function coerceAccentPreference(raw: string | null): AccentKey {
  if (raw && raw in ACCENT_PRESETS) return raw as AccentKey;
  return "green";
}

export const themePreferenceOptions: { value: ThemePreference; label: string }[] =
  [
    { value: "auto", label: "Auto" },
    { value: "light", label: "Light" },
    { value: "dark", label: "Dark" },
  ];

export const themeDefinitions: Record<ResolvedTheme, ThemeDefinition> = {
  dark: {
    id: "dark",
    label: "Dark",
    mode: "dark",
    metaColor: "#000000",
    swatch: {
      bg: "#000000",
      surface: "#0A0A0A",
      border: "#1A1A1A",
      text: "#FFFFFF",
      accent: "#00E676",
    },
    cssVars: {
      "--bg": "#000000",
      "--surface-sidebar": "#000000",
      "--surface-card": "#0A0A0A",
      "--surface-elev": "#0E0E0E",
      "--surface-panel": "#121212",
      "--surface-hover": "rgba(255,255,255,0.04)",
      "--border": "#1A1A1A",
      "--border-strong": "#2A2A2A",
      "--border-soft": "#141414",
      "--text": "#FFFFFF",
      "--text-2": "#9CA3AF",
      "--text-3": "#5F6670",
      "--text-4": "#3A3F47",
      "--gold": "#00E676",
      "--gold-soft": "#00B85F",
      "--gold-bg": "rgba(0,230,118,0.10)",
      "--gold-bg-strong": "rgba(0,230,118,0.18)",
      "--warn": "#FFB020",
      "--danger": "#FF4D4D",
      "--info": "#5FA8FF",
    },
    chart: {
      background: "#0A0A0A",
      text: "#FFFFFF",
      grid: "#1A1A1A",
      series: {
        sma20: "#7dd3fc", sma30: "#a7f3d0", sma50: "#fbbf24", sma60: "#6ee7b7",
        sma90: "#34d399", sma200: "#f87171",
        ema20: "#a78bfa", ema30: "#bae6fd", ema50: "#fbbf24", ema60: "#7dd3fc",
        ema90: "#38bdf8", ema200: "#f87171",
        bollUpper: "#34d399", bollMiddle: "#94a3b8", bollLower: "#34d399",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
        equity: "#00E676",
        equityTop: "rgba(0,230,118,0.30)",
        equityBottom: "rgba(0,230,118,0)",
        drawdown: "#FF4D4D",
        drawdownTop: "rgba(255,77,77,0.30)",
        drawdownBottom: "rgba(255,77,77,0)",
        candleUp: "#00E676", candleDown: "#FF4D4D",
        positionLong: "rgba(0,230,118,0.08)", positionShort: "rgba(255,77,77,0.08)",
        markerBuy: "#00E676", markerSell: "#FF4D4D", markerVeto: "#FFB020", markerHold: "#9CA3AF",
        rsi: "#a78bfa", guide: "#2A2A2A",
        macdLine: "#38BDF8", macdSignal: "#FB923C", macdHistogram: "#5F6670", atr: "#fbbf24",
      },
    },
    chart2: {
      surface: {
        bg: "#000000", panelBg: "#0E0E0E", gridStrong: "#2A2A2A", gridSoft: "#141414",
        axisText: "#9CA3AF", axisTick: "#2A2A2A", crosshair: "#00E676",
      },
      candle: {
        up: "#00E676", down: "#FF4D4D", wickUp: "#00E676", wickDown: "#FF4D4D",
        borderUp: "#00B85F", borderDown: "#D43A3A",
      },
      overlay: {
        sma20: "#7dd3fc", sma30: "#a7f3d0", sma50: "#fbbf24", sma60: "#6ee7b7",
        sma90: "#34d399", sma200: "#f87171",
        ema20: "#a78bfa", ema30: "#bae6fd", ema50: "#fbbf24", ema60: "#7dd3fc",
        ema90: "#38bdf8", ema200: "#f87171",
        bollUpper: "#34d399", bollMiddle: "#94a3b8", bollLower: "#34d399",
        donchianUpper: "#fb923c", donchianLower: "#fb923c",
      },
      marker: {
        buy: "#00E676", sell: "#FF4D4D", veto: "#FFB020", hold: "#9CA3AF",
        halo: "rgba(0,230,118,0.18)", textOnAccent: "#001A0A",
      },
      position: {
        longBand: "rgba(0,230,118,0.08)", shortBand: "rgba(255,77,77,0.08)",
        longLine: "#00E676", shortLine: "#FF4D4D",
      },
      panes: {
        equity: "#00E676", equityFillTop: "rgba(0,230,118,0.30)", equityFillBottom: "rgba(0,230,118,0)",
        drawdown: "#FF4D4D", drawdownFillTop: "rgba(255,77,77,0.30)", drawdownFillBottom: "rgba(255,77,77,0)",
        volumeUp: "rgba(0,230,118,0.45)", volumeDown: "rgba(255,77,77,0.45)",
        rsi: "#a78bfa", rsiGuide: "#2A2A2A",
        macdLine: "#38BDF8", macdSignal: "#FB923C", macdHist: "#5F6670", atr: "#fbbf24",
      },
      compare: {
        palette: ["#00E676", "#38BDF8", "#5EEAD4", "#FBBF24", "#FB923C", "#F472B6", "#A78BFA", "#22D3EE"],
      },
      motion: { hoverMs: 80, animMs: 160 },
      density: { axisFont: "11px 'Geist Mono', ui-monospace, monospace", axisGap: 6, paneGap: 8 },
      warm: CHART2_SIGNAL_DARK,
      strategyRotation: CHART2_STRATEGY_ROTATION,
      heatRamp: CHART2_HEAT_RAMP,
      typography: CHART2_TYPOGRAPHY,
      radius: CHART2_RADIUS,
    },
  },
  light: {
    id: "light",
    label: "Light",
    mode: "light",
    metaColor: "#F7F8FA",
    swatch: {
      bg: "#F7F8FA",
      surface: "#FFFFFF",
      border: "#E3E6EA",
      text: "#0B0E11",
      accent: "#00A15C",
    },
    cssVars: {
      "--bg": "#F7F8FA",
      "--surface-sidebar": "#FFFFFF",
      "--surface-card": "#FFFFFF",
      "--surface-elev": "#F2F4F7",
      "--surface-panel": "#EAEDF1",
      "--surface-hover": "rgba(0,0,0,0.04)",
      "--border": "#E3E6EA",
      "--border-strong": "#CBD1D8",
      "--border-soft": "#EDEFF2",
      "--text": "#0B0E11",
      "--text-2": "#4A5560",
      "--text-3": "#6B7682",
      "--text-4": "#9AA4AE",
      "--gold": "#00A15C",
      "--gold-soft": "#007A48",
      "--gold-bg": "rgba(0,161,92,0.10)",
      "--gold-bg-strong": "rgba(0,161,92,0.16)",
      "--warn": "#B45309",
      "--danger": "#D92D20",
      "--info": "#1D6FD6",
    },
    chart: {
      background: "#FFFFFF",
      text: "#0B0E11",
      grid: "#E3E6EA",
      series: {
        sma20: "#0284c7", sma30: "#047857", sma50: "#a16207", sma60: "#059669",
        sma90: "#16a34a", sma200: "#b91c1c",
        ema20: "#7c3aed", ema30: "#0369a1", ema50: "#a16207", ema60: "#0284c7",
        ema90: "#0ea5e9", ema200: "#b91c1c",
        bollUpper: "#15803d", bollMiddle: "#64748b", bollLower: "#15803d",
        donchianUpper: "#c2410c", donchianLower: "#c2410c",
        equity: "#00A15C",
        equityTop: "rgba(0,161,92,0.22)",
        equityBottom: "rgba(0,161,92,0)",
        drawdown: "#D92D20",
        drawdownTop: "rgba(217,45,32,0.20)",
        drawdownBottom: "rgba(217,45,32,0)",
        candleUp: "#16a34a", candleDown: "#D92D20",
        positionLong: "rgba(0,161,92,0.10)", positionShort: "rgba(217,45,32,0.10)",
        markerBuy: "#16a34a", markerSell: "#D92D20", markerVeto: "#ca8a04", markerHold: "#475569",
        rsi: "#7c3aed", guide: "#94a3b8",
        macdLine: "#0284c7", macdSignal: "#ea580c", macdHistogram: "#64748b", atr: "#a16207",
      },
    },
    chart2: {
      surface: {
        bg: "#FFFFFF", panelBg: "#F2F4F7", gridStrong: "#E3E6EA", gridSoft: "#EDEFF2",
        axisText: "#4A5560", axisTick: "#CBD1D8", crosshair: "#00A15C",
      },
      candle: {
        up: "#16a34a", down: "#D92D20", wickUp: "#16a34a", wickDown: "#D92D20",
        borderUp: "#15803d", borderDown: "#b91c1c",
      },
      overlay: {
        sma20: "#0284c7", sma30: "#047857", sma50: "#a16207", sma60: "#059669",
        sma90: "#16a34a", sma200: "#b91c1c",
        ema20: "#7c3aed", ema30: "#0369a1", ema50: "#a16207", ema60: "#0284c7",
        ema90: "#0ea5e9", ema200: "#b91c1c",
        bollUpper: "#15803d", bollMiddle: "#64748b", bollLower: "#15803d",
        donchianUpper: "#c2410c", donchianLower: "#c2410c",
      },
      marker: {
        buy: "#16a34a", sell: "#D92D20", veto: "#ca8a04", hold: "#475569",
        halo: "rgba(0,161,92,0.18)", textOnAccent: "#FFFFFF",
      },
      position: {
        longBand: "rgba(0,161,92,0.10)", shortBand: "rgba(217,45,32,0.10)",
        longLine: "#16a34a", shortLine: "#D92D20",
      },
      panes: {
        equity: "#00A15C", equityFillTop: "rgba(0,161,92,0.22)", equityFillBottom: "rgba(0,161,92,0)",
        drawdown: "#D92D20", drawdownFillTop: "rgba(217,45,32,0.20)", drawdownFillBottom: "rgba(217,45,32,0)",
        volumeUp: "rgba(22,163,74,0.6)", volumeDown: "rgba(217,45,32,0.6)",
        rsi: "#7c3aed", rsiGuide: "#94a3b8",
        macdLine: "#0284c7", macdSignal: "#ea580c", macdHist: "#64748b", atr: "#a16207",
      },
      compare: {
        palette: ["#00A15C", "#0284C7", "#0D9488", "#CA8A04", "#EA580C", "#DB2777", "#7C3AED", "#0891B2"],
      },
      motion: { hoverMs: 80, animMs: 160 },
      density: { axisFont: "11px 'Geist Mono', ui-monospace, monospace", axisGap: 6, paneGap: 8 },
      warm: CHART2_SIGNAL_LIGHT,
      strategyRotation: CHART2_STRATEGY_ROTATION,
      heatRamp: CHART2_HEAT_RAMP,
      typography: CHART2_TYPOGRAPHY,
      radius: CHART2_RADIUS,
    },
  },
};

// One-time migration of pre-Signal stored preferences.
// "folio-dark" and "black" both collapse to the single "dark" theme.
export function coerceThemePreference(value: string | null): ThemePreference {
  switch (value) {
    case "auto":
    case "light":
    case "dark":
      return value;
    case "folio-dark":
    case "black":
      return "dark";
    default:
      return "dark";
  }
}

export function resolveTheme(
  preference: ThemePreference,
  systemTheme: SystemTheme,
): ResolvedTheme {
  if (preference === "auto") {
    return systemTheme === "light" ? "light" : "dark";
  }
  return preference;
}
