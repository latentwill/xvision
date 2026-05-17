// frontend/web/src/features/agent-runs/DecisionJump.tsx
import { useEffect, useState } from "react";

export type DecisionRef = { i: number };

export function DecisionJump({
  value,
  onChange,
  decisions,
}: {
  value: string;
  onChange: (next: string) => void;
  decisions: DecisionRef[];
}) {
  const ids = decisions.map((d) => d.i);
  const active = value !== "all";
  const curIdx = active ? ids.indexOf(parseInt(value, 10)) : -1;
  const [draft, setDraft] = useState("");

  useEffect(() => {
    setDraft(active ? String(value) : "");
  }, [value, active]);

  const commit = (raw: string) => {
    const n = parseInt(String(raw).replace(/[^0-9]/g, ""), 10);
    if (!Number.isFinite(n) || ids.length === 0) return;
    if (ids.includes(n)) onChange(String(n));
    else {
      const nearest = ids.reduce((a, b) => (Math.abs(b - n) < Math.abs(a - n) ? b : a), ids[0]!);
      onChange(String(nearest));
    }
  };

  const step = (delta: number) => {
    if (ids.length === 0) return;
    if (curIdx === -1) {
      onChange(String(ids[0]));
      return;
    }
    const next = Math.min(ids.length - 1, Math.max(0, curIdx + delta));
    onChange(String(ids[next]));
  };

  return (
    <div
      className="flex items-center gap-1 h-6 pl-1.5 pr-0.5"
      style={{ background: "var(--bg)", border: `1px solid ${active ? "var(--gold-soft)" : "var(--border)"}`, borderRadius: 4 }}
    >
      <span
        className="text-[10px] font-mono tracking-[0.16em] whitespace-nowrap"
        style={{ color: active ? "var(--gold-soft)" : "var(--text-4)" }}
      >
        DECISION&nbsp;#
      </span>
      <input
        value={draft}
        onChange={(e) => setDraft(e.target.value.replace(/[^0-9]/g, ""))}
        onKeyDown={(e) => {
          if (e.key === "Enter") commit(draft);
          else if (e.key === "ArrowUp")   { e.preventDefault(); step(+1); }
          else if (e.key === "ArrowDown") { e.preventDefault(); step(-1); }
          else if (e.key === "Escape" && active) onChange("all");
        }}
        onBlur={() => { if (draft) commit(draft); }}
        placeholder="—"
        className="w-9 h-full bg-transparent text-[11px] font-mono tabular-nums outline-none"
        style={{ color: active ? "var(--gold)" : "var(--text)" }}
      />
      <button
        type="button"
        onClick={() => step(-1)}
        title="Prev decision"
        aria-label="prev decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text"
      >
        ‹
      </button>
      <button
        type="button"
        onClick={() => step(+1)}
        title="Next decision"
        aria-label="next decision"
        className="h-full w-5 flex items-center justify-center text-text-3 hover:text-text"
      >
        ›
      </button>
      <span className="text-[10px] font-mono text-text-4 px-1 tabular-nums whitespace-nowrap leading-none">
        {active ? `${curIdx + 1}/${ids.length}` : `of ${ids.length}`}
      </span>
      {active ? (
        <button
          type="button"
          onClick={() => onChange("all")}
          title="Clear decision filter"
          className="h-full w-5 flex items-center justify-center text-text-3 hover:text-danger text-[12px] leading-none"
        >
          ×
        </button>
      ) : null}
    </div>
  );
}
