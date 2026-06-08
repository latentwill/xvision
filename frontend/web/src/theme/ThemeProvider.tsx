import { useEffect, type ReactNode } from "react";
import { useTheme } from "./useTheme";
import { useAccent } from "./useAccent";
import { ACCENT_PRESETS } from "./themes";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { definition, resolvedTheme } = useTheme();
  const { accentKey } = useAccent();

  useEffect(() => {
    const root = document.documentElement;
    root.dataset.theme = resolvedTheme;
    root.classList.toggle("dark", definition.mode === "dark");
    document
      .querySelector('meta[name="theme-color"]')
      ?.setAttribute("content", definition.metaColor);
  }, [definition.metaColor, definition.mode, resolvedTheme]);

  useEffect(() => {
    const preset = ACCENT_PRESETS[accentKey];
    const isDark = definition.mode === "dark";
    const root = document.documentElement;
    root.style.setProperty("--accent", isDark ? preset.dark : preset.light);
    root.style.setProperty("--on-accent", preset.onAccent);
  }, [accentKey, definition.mode]);

  return <>{children}</>;
}
