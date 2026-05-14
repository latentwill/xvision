import { useEffect, useState } from "react";

export type ViewportMode = "phone" | "tablet" | "desktop";

function currentMode(): ViewportMode {
  if (typeof window === "undefined") return "desktop";
  if (window.matchMedia("(min-width: 1280px)").matches) return "desktop";
  if (window.matchMedia("(min-width: 768px)").matches) return "tablet";
  return "phone";
}

export function useViewportMode(): ViewportMode {
  const [mode, setMode] = useState<ViewportMode>(() => currentMode());

  useEffect(() => {
    const phone = window.matchMedia("(max-width: 767px)");
    const tablet = window.matchMedia("(min-width: 768px) and (max-width: 1279px)");
    const desktop = window.matchMedia("(min-width: 1280px)");
    const update = () => setMode(currentMode());

    phone.addEventListener("change", update);
    tablet.addEventListener("change", update);
    desktop.addEventListener("change", update);
    update();

    return () => {
      phone.removeEventListener("change", update);
      tablet.removeEventListener("change", update);
      desktop.removeEventListener("change", update);
    };
  }, []);

  return mode;
}
