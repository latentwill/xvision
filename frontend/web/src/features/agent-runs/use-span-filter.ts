// frontend/web/src/features/agent-runs/use-span-filter.ts
import { useEffect, useMemo, useState, type SetStateAction } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { categoryOfSpan, type SpanCategory } from "./span-colors";

export type StatusFilter = "all" | "green" | "blue" | "amber" | "red";

type SerializedState = {
  q: string;
  k: SpanCategory[];
  s: StatusFilter;
  d: string;
};

type RunFilterState = SerializedState & {
  runId: string;
};

function lsKey(runId: string): string {
  return `xvn.agent-runs.filter.${runId}`;
}

function loadInitial(runId: string, includeUrl = true): SerializedState {
  try {
    if (includeUrl) {
      const url = new URL(window.location.href);
      const fromUrl = url.searchParams.get("q");
      if (fromUrl) {
        return parseQueryString(fromUrl);
      }
    }
    const raw = localStorage.getItem(lsKey(runId));
    if (raw) return JSON.parse(raw) as SerializedState;
  } catch {
    /* fall through */
  }
  return { q: "", k: [], s: "all", d: "all" };
}

function initialForRun(runId: string, includeUrl = true): RunFilterState {
  return { runId, ...loadInitial(runId, includeUrl) };
}

function stateForRun(state: RunFilterState, runId: string): RunFilterState {
  return state.runId === runId ? state : initialForRun(runId, false);
}

function parseQueryString(qs: string): SerializedState {
  const out: SerializedState = { q: "", k: [], s: "all", d: "all" };
  const tokens = qs.split(/\s+/).filter(Boolean);
  const remaining: string[] = [];
  for (const tok of tokens) {
    if (tok.startsWith("kind:")) {
      const v = tok.slice(5) as SpanCategory;
      // F-7 (qa round 7): `broker` is now a first-class kind chip;
      // `supervisor` is no longer surfaced in the filter row but is
      // still accepted from URL/localStorage state for back-compat
      // with already-shared deep links.
      if (["agent", "model", "tool", "broker", "supervisor", "artifact"].includes(v)) out.k.push(v);
    } else if (tok.startsWith("status:")) {
      const v = tok.slice(7) as StatusFilter;
      if (["green", "blue", "amber", "red", "all"].includes(v)) out.s = v;
    } else if (tok.startsWith("decision:")) {
      out.d = tok.slice(9);
    } else {
      remaining.push(tok);
    }
  }
  out.q = remaining.join(" ");
  return out;
}

function serialize(s: SerializedState): string {
  const parts: string[] = [];
  if (s.q) parts.push(s.q);
  s.k.forEach((k) => parts.push(`kind:${k}`));
  if (s.s !== "all") parts.push(`status:${s.s}`);
  if (s.d !== "all") parts.push(`decision:${s.d}`);
  return parts.join(" ");
}

export function useSpanFilter({ runId, spans }: { runId: string; spans: RunSpan[] }) {
  const [state, setState] = useState<RunFilterState>(() => initialForRun(runId));

  useEffect(() => {
    setState((prev) => (prev.runId === runId ? prev : initialForRun(runId, false)));
  }, [runId]);

  const activeState = state.runId === runId ? state : initialForRun(runId, false);
  const kinds = useMemo(() => new Set(activeState.k), [activeState.k]);

  useEffect(() => {
    if (!runId || state.runId !== runId) return;
    const qs = serialize(state);
    try {
      localStorage.setItem(lsKey(runId), JSON.stringify(state));
      const url = new URL(window.location.href);
      if (qs) url.searchParams.set("q", qs);
      else url.searchParams.delete("q");
      window.history.replaceState({}, "", url.toString());
    } catch {
      /* swallow */
    }
  }, [runId, state]);

  const setQuery = (next: SetStateAction<string>) =>
    setState((prev) => {
      const base = stateForRun(prev, runId);
      const q = typeof next === "function" ? next(base.q) : next;
      return { ...base, q };
    });

  const toggleKind = (k: SpanCategory) =>
    setState((prev) => {
      const base = stateForRun(prev, runId);
      const next = new Set(base.k);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return { ...base, k: [...next] };
    });

  // F-7 (qa round 7): the Trade quick-filter on the dock header sets
  // multiple kind chips at once, so callers need an explicit setter
  // instead of N sequential toggleKind() calls (each of which would
  // race with the prior setState).
  const setKinds = (next: Iterable<SpanCategory>) =>
    setState((prev) => ({ ...stateForRun(prev, runId), k: [...new Set(next)] }));

  const setStatus = (s: StatusFilter) =>
    setState((prev) => ({ ...stateForRun(prev, runId), s }));

  const setDecisionFilter = (d: string) =>
    setState((prev) => ({ ...stateForRun(prev, runId), d }));

  const filtered = useMemo(() => {
    const q = activeState.q.trim().toLowerCase();
    return spans.filter((s) => {
      const cat = categoryOfSpan(s);
      // WS-8: engine.event rows are a parallel lifecycle band, not a span
      // category, so the span-category kind chips don't gate them — a MODEL/
      // TOOL chip must never silently drop a risk veto / regime transition /
      // order-state signal. They still respect status + free-text search.
      if (s.kind !== "engine.event" && kinds.size > 0 && !kinds.has(cat)) {
        return false;
      }
      if (activeState.d !== "all" && String(s.decision_idx ?? "") !== activeState.d) return false;
      // Status predicate
      if (activeState.s === "green" && s.status !== "ok") return false;
      if (activeState.s === "blue" && s.status !== "in_progress") return false;
      if (activeState.s === "red" && s.status !== "error") return false;
      // TODO(agent-run-observability): wire amber to warning_count when backend adds it
      if (!q) return true;
      const tokens = q.split(/\s+/);
      return tokens.every((tok) => {
        if (tok.startsWith("title:")) return s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("model:")) return (s.model ?? "").toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("tool:"))  return cat === "tool" && s.name.toLowerCase().includes(tok.slice(5));
        if (tok.startsWith("agent:")) return cat === "agent" && s.name.toLowerCase().includes(tok.slice(6));
        if (tok.startsWith("decision:")) return String(s.decision_idx ?? "") === tok.slice(9);
        return (
          s.name.toLowerCase().includes(tok) ||
          (s.model ?? "").toLowerCase().includes(tok) ||
          (s.provider ?? "").toLowerCase().includes(tok) ||
          String(s.decision_idx ?? "").includes(tok)
        );
      });
    });
  }, [spans, activeState.q, activeState.d, activeState.s, kinds]);

  return {
    query: activeState.q, setQuery,
    kinds, toggleKind, setKinds,
    status: activeState.s, setStatus,
    decisionFilter: activeState.d, setDecisionFilter,
    filtered,
    summary: { total: spans.length, filtered: filtered.length },
  };
}
