import { useEffect, type ReactNode } from "react";
import { useTheme } from "./useTheme";

export function ThemeProvider({ children }: { children: ReactNode }) {
  const { definition, resolvedTheme } = useTheme();

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
