import { useTheme } from "@/theme/useTheme";
import {
  themeDefinitions,
  type Chart2ThemeDefinition,
  type ResolvedTheme,
} from "@/theme/themes";

/**
 * Non-React helper — usable by adapters, tests, and Storybook stories
 * without needing a React component tree.
 */
export function chart2ThemeFor(resolved: ResolvedTheme): Chart2ThemeDefinition {
  return themeDefinitions[resolved].chart2;
}

/**
 * React hook — returns the Chart2ThemeDefinition for the currently
 * active resolved theme. Re-renders when the user changes their theme.
 */
export function useChart2Theme(): Chart2ThemeDefinition {
  const { resolvedTheme } = useTheme();
  return chart2ThemeFor(resolvedTheme);
}
