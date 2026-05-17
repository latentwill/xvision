// frontend/web/src/features/agent-runs/FilterBar.tsx
import type { Dispatch, SetStateAction } from "react";
import { DecisionJump, type DecisionRef } from "./DecisionJump";
import { CATEGORY_STYLES, type SpanCategory } from "./span-colors";
import type { StatusFilter } from "./use-span-filter";

const KIND_ORDER: SpanCategory[] = ["agent", "model", "tool", "supervisor", "artifact"];

const STATUS_DEF: Array<{ k: StatusFilter; glyph: string; tint: string; bg: string; bd: string }> = [
  { k: "green", glyph: "✓", tint: "var(--gold)",   bg: "var(--gold-bg)",         bd: "var(--gold-soft)" },
  { k: "blue",  glyph: "▶", tint: "var(--info)",   bg: "rgba(111,143,184,0.14)", bd: "rgba(111,143,184,0.45)" },
  { k: "amber", glyph: "⚠", tint: "var(--warn)",   bg: "rgba(219,146,48,0.10)",  bd: "rgba(219,146,48,0.45)" },
  { k: "red",   glyph: "✕", tint: "var(--danger)", bg: "rgba(200,68,58,0.10)",   bd: "rgba(200,68,58,0.45)" },
];

export function FilterBar({
  query, setQuery,
  kinds, toggleKind,
  status, setStatus,
  decisionFilter, setDecisionFilter,
  decisions,
  total, filtered,
}: {
  query: string;
  setQuery: Dispatch<SetStateAction<string>> | ((v: string) => void);
  kinds: Set<SpanCategory>;
  toggleKind: (k: SpanCategory) => void;
  status: StatusFilter;
  setStatus: (s: StatusFilter) => void;
  decisionFilter: string;
  setDecisionFilter: (d: string) => void;
  decisions: DecisionRef[];
  total: number;
  filtered: number;
}) {
  return (
    <div
      className="h-9 px-2 flex items-center gap-2 shrink-0 overflow-hidden"
      style={{ borderBottom: "1px solid var(--border)", background: "var(--surface-elev)" }}
    >
      <div
        className="flex items-center gap-1.5 h-6 px-2 flex-1 min-w-[200px] max-w-[380px]"
        style={{ background: "var(--bg)", border: "1px solid var(--border)", borderRadius: 4 }}
      >
        <svg width="10" height="10" viewBox="0 0 16 16" fill="none" aria-hidden>
          <circle cx="7" cy="7" r="4.5" stroke="currentColor" strokeWidth="1.4" />
          <path d="M11 11l3.5 3.5" stroke="currentColor" strokeWidth="1.4" strokeLinecap="round" />
        </svg>
        <input
          value={query}
          onChange={(e) => (setQuery as (v: string) => void)(e.target.value)}
          placeholder='filter   title:agent.plan   model:gpt-5   tool:run_backtest'
          className="flex-1 bg-transparent text-[11px] font-mono text-text outline-none placeholder:text-text-4 min-w-0"
        />
        {query ? (
          <button
            type="button"
            onClick={() => (setQuery as (v: string) => void)("")}
            className="text-text-3 hover:text-text text-[10px] font-mono"
          >
            ×
          </button>
        ) : null}
      </div>

      <div className="flex items-center gap-0.5 shrink-0">
        {KIND_ORDER.map((k) => {
          const on = kinds.has(k);
          const style = CATEGORY_STYLES[k];
          return (
            <button
              type="button"
              key={k}
              onClick={() => toggleKind(k)}
              className="h-6 px-1.5 text-[10px] font-mono tracking-[0.14em] flex items-center gap-1"
              style={{
                background: on ? "var(--surface-card)" : "transparent",
                border: `1px solid ${on ? style.hex : "var(--border)"}`,
                color: on ? style.hex : "var(--text-3)",
                borderRadius: 4,
              }}
            >
              <span className="w-1.5 h-1.5 inline-block" style={{ background: style.hex, opacity: on ? 1 : 0.5 }} />
              {style.label}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} />

      <div className="flex items-center gap-0.5 shrink-0">
        {STATUS_DEF.map((s) => {
          const on = status === s.k;
          return (
            <button
              type="button"
              key={s.k}
              onClick={() => setStatus(on ? "all" : s.k)}
              title={s.k.toUpperCase()}
              aria-label={`status: ${s.k}`}
              className="h-6 w-6 text-[10px] font-mono flex items-center justify-center"
              style={{
                background: on ? s.bg : "transparent",
                border: `1px solid ${on ? s.bd : "var(--border)"}`,
                color: on ? s.tint : "var(--text-3)",
                borderRadius: 4,
              }}
            >
              {s.glyph}
            </button>
          );
        })}
      </div>

      <div className="w-px h-4 shrink-0" style={{ background: "var(--border)" }} />

      <DecisionJump value={decisionFilter} onChange={setDecisionFilter} decisions={decisions} />

      <div className="ml-auto text-[10px] font-mono text-text-3 tabular-nums pr-1 shrink-0 whitespace-nowrap">
        <span className="text-text">{filtered}</span>
        <span className="text-text-4">/</span>
        <span>{total}</span> spans
      </div>
    </div>
  );
}
