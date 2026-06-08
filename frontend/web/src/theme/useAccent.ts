import { useCallback, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageSet } from "@/lib/storage";
import {
  coerceAccentPreference,
  ACCENT_PREFERENCE_KEY,
  type AccentKey,
} from "./themes";

const listeners = new Set<() => void>();
let snapshot: AccentKey = readSnapshot();

function readSnapshot(): AccentKey {
  return coerceAccentPreference(safeStorageGet(ACCENT_PREFERENCE_KEY));
}

function sameSnapshot(a: AccentKey, b: AccentKey) {
  return a === b;
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

export function useAccent() {
  const accentKey = useSyncExternalStore(subscribe, getSnapshot, getSnapshot);

  const setAccent = useCallback((key: AccentKey) => {
    safeStorageSet(ACCENT_PREFERENCE_KEY, key);
    refreshSnapshot();
  }, []);

  return useMemo(() => ({ accentKey, setAccent }), [accentKey, setAccent]);
}
