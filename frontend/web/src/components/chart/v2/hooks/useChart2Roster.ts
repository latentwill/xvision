/**
 * useChart2Roster — URL-synced selection state for the Comparison AB
 * dashboard (B2). Backs `?ids=fib,ema,brk` on `/charts/compare`.
 *
 * Invariants:
 *   - The selected list is always a subset of the canonical rotation
 *     (handled by the caller passing `available`).
 *   - At least 2 strategies are selected at any time. Calls to `remove`
 *     or `toggle` that would push count below 2 are no-ops; the
 *     `canRemove(id)` helper exposes the same gate to UI.
 *   - The first item of the list is treated as "lead". `setLead`
 *     reorders the array; if the id isn't currently selected, it's
 *     added (subject to the standard add behaviour).
 *
 * Hook tests live alongside this file; the *pure* helpers (`add`,
 * `remove`, `toggle`, `setLead`) are exported standalone so they can be
 * tested without React/Router overhead.
 */
import { useCallback, useMemo } from "react";
import { useSearchParams } from "react-router-dom";

export const DEFAULT_ROSTER_MIN = 2;

export interface RosterValue {
  selectedIds: string[];
  count: number;
  add: (id: string) => void;
  remove: (id: string) => void;
  toggle: (id: string) => void;
  setLead: (id: string) => void;
  /** UI-facing gate: can the user remove this id without violating the
   *  min-selection invariant? */
  canRemove: (id: string) => boolean;
}

// ── Pure transition helpers — exported for unit tests ──────────────────────

/** Add `id` if it isn't already selected. */
export function addId(
  current: string[],
  id: string,
  available: readonly string[],
): string[] {
  if (!available.includes(id)) return current;
  if (current.includes(id)) return current;
  return [...current, id];
}

/** Remove `id` unless removal would drop below the min selection. */
export function removeId(
  current: string[],
  id: string,
  min: number,
): string[] {
  if (!current.includes(id)) return current;
  if (current.length <= min) return current;
  return current.filter((x) => x !== id);
}

/** Toggle: add if missing, remove if present (subject to min). */
export function toggleId(
  current: string[],
  id: string,
  available: readonly string[],
  min: number,
): string[] {
  if (current.includes(id)) return removeId(current, id, min);
  return addId(current, id, available);
}

/** Move `id` to the front of the list (the lead position). Adds it if
 *  not currently present. */
export function setLeadId(
  current: string[],
  id: string,
  available: readonly string[],
): string[] {
  if (!available.includes(id)) return current;
  const without = current.filter((x) => x !== id);
  return [id, ...without];
}

/** Parse a `?ids=a,b,c` string into a clean, in-order, deduped id list
 *  filtered to `available`. */
export function parseRosterParam(
  raw: string | null,
  available: readonly string[],
): string[] {
  if (!raw) return [];
  const seen = new Set<string>();
  const out: string[] = [];
  for (const tok of raw.split(",")) {
    const id = tok.trim();
    if (!id) continue;
    if (seen.has(id)) continue;
    if (!available.includes(id)) continue;
    seen.add(id);
    out.push(id);
  }
  return out;
}

/** Serialize the selected ids back into a URL value. */
export function serializeRosterParam(ids: readonly string[]): string {
  return ids.join(",");
}

// ── Hook ──────────────────────────────────────────────────────────────────

export interface UseChart2RosterOpts {
  /** All strategy ids that may appear in the roster, in stable display
   *  order. Drives the on-state palette and the available-pill rail. */
  available: readonly string[];
  /** Min selected ids — defaults to 2 per the spec. */
  min?: number;
  /** Fallback selection used when the URL is empty / invalid.
   *  Defaults to the first `Math.max(min, 6)` ids from `available`. */
  defaultSelected?: readonly string[];
}

export function useChart2Roster(opts: UseChart2RosterOpts): RosterValue {
  const min = opts.min ?? DEFAULT_ROSTER_MIN;
  const [params, setParams] = useSearchParams();

  const fallback = useMemo<string[]>(() => {
    if (opts.defaultSelected) return [...opts.defaultSelected];
    const want = Math.max(min, 6);
    return [...opts.available.slice(0, want)];
  }, [opts.available, opts.defaultSelected, min]);

  const parsed = useMemo<string[]>(() => {
    const fromUrl = parseRosterParam(params.get("ids"), opts.available);
    if (fromUrl.length >= min) return fromUrl;
    return fallback;
  }, [params, opts.available, min, fallback]);

  // Stable callbacks. Each computes the next selection from the latest
  // `parsed` (closed over via the params dep) and writes back to the URL.
  const write = useCallback(
    (next: string[]) => {
      setParams(
        (prev) => {
          const out = new URLSearchParams(prev);
          if (next.length === 0) out.delete("ids");
          else out.set("ids", serializeRosterParam(next));
          return out;
        },
        { replace: true },
      );
    },
    [setParams],
  );

  const add = useCallback(
    (id: string) => write(addId(parsed, id, opts.available)),
    [parsed, opts.available, write],
  );
  const remove = useCallback(
    (id: string) => write(removeId(parsed, id, min)),
    [parsed, min, write],
  );
  const toggle = useCallback(
    (id: string) => write(toggleId(parsed, id, opts.available, min)),
    [parsed, opts.available, min, write],
  );
  const setLead = useCallback(
    (id: string) => write(setLeadId(parsed, id, opts.available)),
    [parsed, opts.available, write],
  );
  const canRemove = useCallback(
    (id: string) => parsed.includes(id) && parsed.length > min,
    [parsed, min],
  );

  return {
    selectedIds: parsed,
    count: parsed.length,
    add,
    remove,
    toggle,
    setLead,
    canRemove,
  };
}
