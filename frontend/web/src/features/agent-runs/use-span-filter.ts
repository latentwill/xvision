// frontend/web/src/features/agent-runs/use-span-filter.ts
import { useEffect, useMemo, useState } from "react";
import type { RunSpan } from "@/api/types-agent-runs";
import { categoryOf, type SpanCategory } from "./span-colors";

export type StatusFilter = "all" | "green" | "blue" | "amber" | "red";

type SerializedState = {
  q: string;
  k: SpanCategory[];
  s: StatusFilter;
  d: string;
};

function lsKey(runId: string): string {
  return `xvn.agent-runs.filter.${runId}`;
}

function loadInitial(runId: string): SerializedState {
  try {
    const url = new URL(window.location.href);
    const fromUrl = url.searchParams.get("q");
    if (fromUrl) {
      return parseQueryString(fromUrl);
    }
    const raw = localStorage.getItem(lsKey(runId));
    if (raw) return JSON.parse(raw) as SerializedState;
  } catch {
    /* fall through */
  }
  return { q: "", k: [], s: "all", d: "all" };
}

function parseQueryString(qs: string): SerializedState {
  const out: SerializedState = { q: "", k: [], s: "all", d: "all" };
  const tokens = qs.split(/\s+/).filter(Boolean);
  const remaining: string[] = [];
  for (const tok of tokens) {
    if (tok.startsWith("kind:")) {
      const v = tok.slice(5) as SpanCategory;
      if (["agent", "model", "tool", "supervisor", "artifact"].includes(v)) out.k.push(v);
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
  const initial = useMemo(() => loadInitial(runId), [runId]);
  const [query, setQuery] = useState(initial.q);
  const [kinds, setKinds] = useState<Set<SpanCategory>>(new Set(initial.k));
  const [status, setStatus] = useState<StatusFilter>(initial.s);
  const [decisionFilter, setDecisionFilter] = useState<string>(initial.d);

  useEffect(() => {
    const state: SerializedState = { q: query, k: [...kinds], s: status, d: decisionFilter };
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
  }, [runId, query, kinds, status, decisionFilter]);

  const toggleKind = (k: SpanCategory) =>
    setKinds((prev) => {
      const next = new Set(prev);
      if (next.has(k)) next.delete(k);
      else next.add(k);
      return next;
    });

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    return spans.filter((s) => {
      const cat = categoryOf(s.kind);
      if (kinds.size > 0 && !kinds.has(cat)) return false;
      if (decisionFilter !== "all" && String(s.decision_idx ?? "") !== decisionFilter) return false;
      // Status predicate
      if (status === "green" && s.status !== "ok") return false;
      if (status === "blue" && s.status !== "in_progress") return false;
      if (status === "red" && s.status !== "error") return false;
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
  }, [spans, query, kinds, status, decisionFilter]);

  return {
    query, setQuery,
    kinds, toggleKind,
    status, setStatus,
    decisionFilter, setDecisionFilter,
    filtered,
    summary: { total: spans.length, filtered: filtered.length },
  };
}
