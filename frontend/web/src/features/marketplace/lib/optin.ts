import { useCallback, useMemo, useSyncExternalStore } from "react";
import { safeStorageGet, safeStorageRemove, safeStorageSet } from "@/lib/storage";

// Client-side opt-in for the marketplace (a Mantle *testnet* feature).
// Persisted to localStorage, DEFAULT OFF. No backend endpoint / migration —
// matches the repo's local-toggle precedent (useTheme / useWallet) and keeps
// the testnet-phase footprint minimal.
export const MARKETPLACE_OPTIN_KEY = "xvn_marketplace_optin";

const listeners = new Set<() => void>();
let snapshot = readSnapshot();

function readSnapshot(): boolean {
  return safeStorageGet(MARKETPLACE_OPTIN_KEY) === "1";
}

function refreshSnapshot() {
  const next = readSnapshot();
  if (next === snapshot) return;
  snapshot = next;
  listeners.forEach((listener) => listener());
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  return () => listeners.delete(listener);
}

function getSnapshot() {
  // Re-read on every read so external writes (other tabs, tests setting
  // localStorage directly, the settings toggle in another mount) are observed.
  // Mirrors the theme store. The cached `snapshot` stays referentially stable
  // when the value is unchanged, so useSyncExternalStore won't loop.
  refreshSnapshot();
  return snapshot;
}

// SSR-safe: the server has no localStorage and defaults to off.
function getServerSnapshot() {
  return false;
}

function writeEnabled(enabled: boolean) {
  if (enabled) {
    safeStorageSet(MARKETPLACE_OPTIN_KEY, "1");
  } else {
    safeStorageRemove(MARKETPLACE_OPTIN_KEY);
  }
  refreshSnapshot();
}

export interface MarketplaceOptInState {
  enabled: boolean;
  setEnabled: (enabled: boolean) => void;
}

export function useMarketplaceOptIn(): MarketplaceOptInState {
  const enabled = useSyncExternalStore(subscribe, getSnapshot, getServerSnapshot);
  const setEnabled = useCallback((next: boolean) => writeEnabled(next), []);
  return useMemo(() => ({ enabled, setEnabled }), [enabled, setEnabled]);
}
