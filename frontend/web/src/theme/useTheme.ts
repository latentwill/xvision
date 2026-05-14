import { useCallback, useEffect, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";
import {
  coerceDarkTheme,
  coerceThemePreference,
  resolveTheme,
  themeDefinitions,
  THEME_DARK_KEY,
  THEME_PREFERENCE_KEY,
  type ResolvedTheme,
  type SystemTheme,
  type ThemePreference,
} from "./themes";

type DarkTheme = Extract<ResolvedTheme, "folio-dark" | "black">;

type Snapshot = {
  preference: ThemePreference;
  darkTheme: DarkTheme;
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
    darkTheme: coerceDarkTheme(safeStorageGet(THEME_DARK_KEY)),
    systemTheme: readSystemTheme(),
  };
}

function sameSnapshot(a: Snapshot, b: Snapshot) {
  return (
    a.preference === b.preference &&
    a.darkTheme === b.darkTheme &&
    a.systemTheme === b.systemTheme
  );
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

export function setThemePreference(preference: ThemePreference) {
  safeStorageSet(THEME_PREFERENCE_KEY, preference);
  if (preference === "folio-dark" || preference === "black") {
    safeStorageSet(THEME_DARK_KEY, preference);
  }
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
  const setDarkTheme = useCallback(() => {
    setThemePreference(readSnapshot().darkTheme);
  }, []);

  return useMemo(
    () => ({
      preference: current.preference,
      darkTheme: current.darkTheme,
      resolvedTheme,
      definition,
      setPreference,
      setLightTheme,
      setDarkTheme,
    }),
    [
      current.darkTheme,
      current.preference,
      definition,
      resolvedTheme,
      setDarkTheme,
      setLightTheme,
      setPreference,
    ],
  );
}
