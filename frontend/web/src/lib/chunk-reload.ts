// Detection + once-per-session reload helper for Vite SPA chunk-load
// failures. Background: when a new deploy ships, the running tab still
// holds the previous `index.html` which references hash-suffixed chunk
// filenames the new build replaced. The next lazy import 404s with
// "TypeError: Failed to fetch dynamically imported module". A hard
// refresh recovers because the new `index.html` ships the new hashes.
//
// `AppErrorBoundary` calls `attemptChunkReload` when it catches a
// chunk-load error; this module owns the one-reload-per-session guard
// and the post-reload notification flag.

const RELOAD_FLAG = "xvn:chunk-reload-attempted";
const NOTICE_FLAG = "xvn:chunk-reload-just-completed";

const CHUNK_ERROR_PATTERNS = [
  "Failed to fetch dynamically imported module",
  "error loading dynamically imported module",
  "Importing a module script failed",
];

function getSessionStorage(): Storage | null {
  try {
    if (typeof window === "undefined") return null;
    return window.sessionStorage ?? null;
  } catch {
    return null;
  }
}

export function isChunkLoadError(error: unknown): boolean {
  if (!error) return false;

  // Common "ChunkLoadError" name from bundlers other than Vite.
  if (typeof error === "object" && error !== null) {
    const name = (error as { name?: unknown }).name;
    if (typeof name === "string" && name === "ChunkLoadError") {
      return true;
    }
    const message = (error as { message?: unknown }).message;
    if (typeof message === "string") {
      for (const pattern of CHUNK_ERROR_PATTERNS) {
        if (message.includes(pattern)) return true;
      }
    }
  }

  if (typeof error === "string") {
    for (const pattern of CHUNK_ERROR_PATTERNS) {
      if (error.includes(pattern)) return true;
    }
  }

  return false;
}

/**
 * If `error` is a chunk-load error and we haven't already attempted a
 * reload in this browser session, set the reload-attempted flag and
 * trigger `window.location.reload()`. Returns `true` if a reload was
 * triggered, `false` otherwise (either not a chunk error, or already
 * attempted this session).
 *
 * The session flag is the loop-guard: if the reload itself fails (still
 * fetching a stale chunk somehow), the second pass through the
 * boundary will get `false` and fall through to the manual-refresh
 * hint render.
 */
export function attemptChunkReload(error: unknown): boolean {
  if (!isChunkLoadError(error)) return false;
  const storage = getSessionStorage();
  if (!storage) return false;
  if (storage.getItem(RELOAD_FLAG)) return false;

  try {
    storage.setItem(RELOAD_FLAG, "1");
    // Notice flag survives the reload and triggers the post-reload toast.
    storage.setItem(NOTICE_FLAG, "1");
  } catch {
    // sessionStorage write can fail in private mode; bail out rather
    // than risk a reload loop with no guard.
    return false;
  }

  try {
    window.location.reload();
  } catch {
    return false;
  }
  return true;
}

/**
 * Called on app boot AFTER the initial lazy chunks have resolved
 * (i.e., from a `useEffect` at the top of the React tree, which only
 * fires after the first successful commit). Clearing the flag here is
 * the documented clear-trigger: a future deploy within the same
 * session is allowed to retry the reload once the current bundle has
 * proven itself loadable.
 */
export function noteSuccessfulPageLoad(): void {
  const storage = getSessionStorage();
  if (!storage) return;
  try {
    storage.removeItem(RELOAD_FLAG);
  } catch {
    // best-effort
  }
}

/**
 * If the previous page-lifecycle just performed a chunk-reload, returns
 * true once and clears the notice flag. Callers (App.tsx) use this to
 * surface a one-shot post-reload notification.
 */
export function consumePostReloadNotice(): boolean {
  const storage = getSessionStorage();
  if (!storage) return false;
  try {
    const value = storage.getItem(NOTICE_FLAG);
    if (!value) return false;
    storage.removeItem(NOTICE_FLAG);
    return true;
  } catch {
    return false;
  }
}

// Exposed for tests so we don't hard-code the key string across the
// suite. Production callers should use the helpers above.
export const __INTERNAL = {
  RELOAD_FLAG,
  NOTICE_FLAG,
};
