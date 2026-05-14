import { useEffect } from "react";
import { useLocation } from "react-router-dom";

import {
  clearTourCompletion,
  isTourCompleted,
  markTourCompleted,
} from "./tour-state";
import { tourForPath, type RouteTour } from "./tour-registry";

export const RESTART_TOUR_EVENT = "xvn:restart-tour";

export function TourController() {
  const location = useLocation();

  useEffect(() => {
    const tour = tourForPath(location.pathname);
    if (!tour || isTourCompleted(tour.id)) return;
    void startTour(tour);
  }, [location.pathname]);

  useEffect(() => {
    function onRestart() {
      const tour = tourForPath(location.pathname);
      if (!tour) return;
      clearTourCompletion(tour.id);
      void startTour(tour);
    }

    window.addEventListener(RESTART_TOUR_EVENT, onRestart);
    return () => window.removeEventListener(RESTART_TOUR_EVENT, onRestart);
  }, [location.pathname]);

  return null;
}

async function startTour(tour: RouteTour) {
  const visibleSteps = tour.steps.filter((step) =>
    document.querySelector(step.element),
  );
  if (visibleSteps.length === 0) return;

  const { driver } = await import("driver.js");
  await import("driver.js/dist/driver.css");

  const tourDriver = driver({
    allowClose: true,
    showProgress: visibleSteps.length > 1,
    steps: visibleSteps,
    onDestroyed: () => markTourCompleted(tour.id),
  });
  tourDriver.drive();
}
