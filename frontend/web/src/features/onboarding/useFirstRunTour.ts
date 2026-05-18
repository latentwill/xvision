import { useEffect } from "react";
import {
  safeStorageGet,
  safeStorageRemove,
  safeStorageSet,
} from "@/lib/storage";
import { TOUR_COMPLETED_KEY } from "./keys";
import { firstRunTourSteps } from "./steps";

// Module-level guard so React StrictMode's deliberate double-invoke of
// effects cannot race two `driver()` instances. Set BEFORE the async
// `import("driver.js")` settles, cleared only when the tour is destroyed
// or its launch path bails out.
let tourLaunching = false;

function markCompleted() {
  safeStorageSet(TOUR_COMPLETED_KEY, "1");
}

function isCompleted(): boolean {
  return safeStorageGet(TOUR_COMPLETED_KEY) === "1";
}

async function runTour(opts: { force: boolean }) {
  if (typeof window === "undefined" || typeof document === "undefined") return;
  if (!opts.force && isCompleted()) return;
  if (tourLaunching) return;
  tourLaunching = true;
  let mod: typeof import("driver.js");
  try {
    mod = await import("driver.js");
    // CSS side-effect import; ignored by tsc, handled by Vite.
    // @ts-expect-error css module has no types
    await import("driver.js/dist/driver.css");
  } catch {
    // Driver.js unavailable (e.g. test env without the chunk). Skip silently.
    markCompleted();
    tourLaunching = false;
    return;
  }
  const drv = mod.driver({
    showProgress: true,
    allowClose: true,
    onCloseClick: () => {
      markCompleted();
      drv.destroy();
    },
    onDestroyed: () => {
      markCompleted();
      tourLaunching = false;
    },
    steps: firstRunTourSteps,
  });
  drv.drive();
}

export function useFirstRunTour() {
  useEffect(() => {
    void runTour({ force: false });
  }, []);
}

export function restartFirstRunTour() {
  safeStorageRemove(TOUR_COMPLETED_KEY);
  void runTour({ force: true });
}

export function hasCompletedFirstRunTour(): boolean {
  return isCompleted();
}

// Test-only escape hatch for resetting the module-level guard between
// tests. Not exported from the package barrel.
export function __resetFirstRunTourForTests() {
  tourLaunching = false;
}
