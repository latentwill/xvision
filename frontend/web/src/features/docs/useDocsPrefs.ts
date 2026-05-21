import { useCallback, useEffect, useState } from "react";

// Two non-destructive display preferences for the docs reader, mirroring
// the prototype's Tweaks panel (`docs/design/xvnwiki/docs/docs.js`):
//   - density: compact | comfortable — line-height + heading size
//   - toc:     shown   | hidden      — right-rail TOC visibility
//
// Persisted in localStorage. Theme is intentionally NOT included; theme
// is already a global app concern (light/dark) handled by the shell.

export type DocsDensity = "compact" | "comfortable";
export type DocsTocVisibility = "shown" | "hidden";

export type DocsPrefs = {
  density: DocsDensity;
  toc: DocsTocVisibility;
};

const KEY = "xvn-docs-prefs";
const DEFAULTS: DocsPrefs = { density: "compact", toc: "shown" };

function read(): DocsPrefs {
  if (typeof window === "undefined") return DEFAULTS;
  try {
    const raw = window.localStorage.getItem(KEY);
    if (!raw) return DEFAULTS;
    const parsed = JSON.parse(raw) as Partial<DocsPrefs>;
    return {
      density: parsed.density === "comfortable" ? "comfortable" : "compact",
      toc: parsed.toc === "hidden" ? "hidden" : "shown",
    };
  } catch {
    return DEFAULTS;
  }
}

export function useDocsPrefs() {
  const [prefs, setPrefs] = useState<DocsPrefs>(() => read());

  useEffect(() => {
    try {
      window.localStorage.setItem(KEY, JSON.stringify(prefs));
    } catch {
      // localStorage may be unavailable (private mode); preferences just
      // don't persist across reloads in that case.
    }
  }, [prefs]);

  const setDensity = useCallback((density: DocsDensity) => {
    setPrefs((p) => ({ ...p, density }));
  }, []);
  const setToc = useCallback((toc: DocsTocVisibility) => {
    setPrefs((p) => ({ ...p, toc }));
  }, []);

  return { prefs, setDensity, setToc };
}
