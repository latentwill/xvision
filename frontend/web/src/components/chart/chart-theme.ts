import { themeDefinitions, type ResolvedTheme } from "@/theme/themes";

export type ChartThemeInput = ResolvedTheme;

export function normalizeChartTheme(
  theme: ChartThemeInput | undefined,
  fallback: ResolvedTheme = "dark",
): ResolvedTheme {
  if (!theme) return fallback;
  return theme;
}

export function chartTheme(theme: ChartThemeInput = "dark") {
  return themeDefinitions[normalizeChartTheme(theme)].chart;
}
