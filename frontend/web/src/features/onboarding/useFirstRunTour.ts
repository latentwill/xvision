import { useEffect } from "react";
import {
  safeStorageGet,
  safeStorageRemove,
  safeStorageSet,
} from "@/lib/storage";
import { TOUR_COMPLETED_KEY } from "./keys";
import { firstRunTourSteps } from "./steps";
import { tourThemeConfig } from "./tour-theme";

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
    // CSS side-effect import; typed by vite/client (src/vite-env.d.ts),
    // handled by Vite.
    await import("driver.js/dist/driver.css");
    await import("./tour-theme.css");
  } catch {
    // Driver.js unavailable (e.g. test env without the chunk). Skip silently.
    markCompleted();
    tourLaunching = false;
    return;
  }
  const theme = tourThemeConfig(firstRunTourSteps);
  const drv = mod.driver({
    ...theme,
    allowClose: true,
    onCloseClick: () => {
      markCompleted();
      drv.destroy();
    },
    onDestroyed: () => {
      theme.__teardown();
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
  // An explicit restart must always win. A prior tour run can leave the
  // module-level `tourLaunching` guard set (e.g. it was completed/closed and
  // the driver torn down outside our onDestroyed path), which would make
  // runTour bail early and strand the restart on step 1. Clear it first.
  tourLaunching = false;
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
