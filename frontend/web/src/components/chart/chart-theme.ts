import { themeDefinitions, type ResolvedTheme } from "@/theme/themes";

export type ChartThemeInput = ResolvedTheme | "dark";

export function normalizeChartTheme(
  theme: ChartThemeInput | undefined,
  fallback: ResolvedTheme = "folio-dark",
): ResolvedTheme {
  if (!theme) return fallback;
  return theme === "dark" ? "folio-dark" : theme;
}

export function chartTheme(theme: ChartThemeInput = "folio-dark") {
  return themeDefinitions[normalizeChartTheme(theme)].chart;
}
