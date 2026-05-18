import { useEffect, useRef } from "react";
import {
  safeStorageGet,
  safeStorageRemove,
  safeStorageSet,
} from "@/lib/storage";
import { TOUR_COMPLETED_KEY } from "./keys";
import { firstRunTourSteps } from "./steps";

function markCompleted() {
  safeStorageSet(TOUR_COMPLETED_KEY, "1");
}

function isCompleted(): boolean {
  return safeStorageGet(TOUR_COMPLETED_KEY) === "1";
}

async function runTour(opts: { force: boolean }) {
  if (typeof window === "undefined" || typeof document === "undefined") return;
  if (!opts.force && isCompleted()) return;
  let mod: typeof import("driver.js");
  try {
    mod = await import("driver.js");
    // CSS side-effect import; ignored by tsc, handled by Vite.
    // @ts-expect-error css module has no types
    await import("driver.js/dist/driver.css");
  } catch {
    // Driver.js unavailable (e.g. test env without the chunk). Skip silently.
    markCompleted();
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
    },
    steps: firstRunTourSteps,
  });
  drv.drive();
}

export function useFirstRunTour() {
  const ranRef = useRef(false);
  useEffect(() => {
    if (ranRef.current) return;
    ranRef.current = true;
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
