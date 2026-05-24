import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";
import {
  coerceThemePreference,
  resolveTheme,
  themeDefinitions,
  THEME_PREFERENCE_KEY,
  type SystemTheme,
  type ThemePreference,
} from "./themes";

type Snapshot = {
  preference: ThemePreference;
  systemTheme: SystemTheme;
};

const listeners = new Set<() => void>();
let snapshot: Snapshot = readSnapshot();

function readSystemTheme(): SystemTheme {
  if (
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-color-scheme: dark)").matches
  ) {
    return "dark";
  }
  return "light";
}

function readSnapshot(): Snapshot {
  return {
    preference: coerceThemePreference(safeStorageGet(THEME_PREFERENCE_KEY)),
    systemTheme: readSystemTheme(),
  };
}

function sameSnapshot(a: Snapshot, b: Snapshot) {
  return a.preference === b.preference && a.systemTheme === b.systemTheme;
}

function refreshSnapshot() {
  const next = readSnapshot();
  if (sameSnapshot(snapshot, next)) return;
  snapshot = next;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot() {
  refreshSnapshot();
  return snapshot;
}

function setThemePreference(preference: ThemePreference) {
  safeStorageSet(THEME_PREFERENCE_KEY, preference);
  refreshSnapshot();
}

export function useTheme() {
  const current = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);
  const resolvedTheme = resolveTheme(current.preference, current.systemTheme);
  const definition = themeDefinitions[resolvedTheme];

  useEffect(() => {
    const query = window.matchMedia?.("(prefers-color-scheme: dark)");
    if (!query) return;
    const onChange = () => refreshSnapshot();
    query.addEventListener("change", onChange);
    return () => query.removeEventListener("change", onChange);
  }, []);

  const setPreference = useCallback((preference: ThemePreference) => {
    setThemePreference(preference);
  }, []);
  const setLightTheme = useCallback(() => setThemePreference("light"), []);
  const setDarkTheme = useCallback(() => setThemePreference("dark"), []);

  return useMemo(
    () => ({
      preference: current.preference,
      resolvedTheme,
      definition,
      setPreference,
      setLightTheme,
      setDarkTheme,
    }),
    [current.preference, definition, resolvedTheme, setDarkTheme, setLightTheme, setPreference],
  );
}
