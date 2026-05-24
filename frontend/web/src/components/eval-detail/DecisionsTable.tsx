// Signal Decisions card — toolbar (search + sort) + mutually-exclusive action
// filter-pill row + density strip + the decisions table. README §7 / Task B4
// step 8.
//
// State (search / actionFilter / sortKey) is component-local. Clicking a row or
// a density tick calls `onJump(i)` — the same handler — and the focused row /
// tick gets the gold treatment. Filtered rows dim to 0.78 and show `—` in
// `--text-4` for every engaged-only cell (action/conviction/justification/pnl).

import { useMemo, useState } from "react";
import { Icon } from "@/components/primitives/Icon";
import { ActionPill } from "./ActionPill";
import { PhaseChip } from "./PhaseChip";
import { DecisionTimeline } from "./DecisionTimeline";
import type { TimelineDecision } from "./decision-view";
import {
  actionCounts,
  matchesActionFilter,
  searchHay,
  sortDecisions,
  type ActionFilter,
  type SortKey,
} from "./decision-table-state";

const PILLS: {
  k: ActionFilter;
  label: string;
  dotColor: string;
  activeBg: string;
  activeBd: string;
  activeFg: string;
  filled: boolean;
}[] = [
  {
    k: "all",
    label: "All",
    dotColor: "var(--text-2)",
    activeBg: "var(--surface-elev)",
    activeBd: "var(--border-strong)",
    activeFg: "var(--text)",
    filled: true,
  },
  {
    k: "BUY",
    label: "Buy",
    dotColor: "var(--gold)",
    activeBg: "var(--gold-bg)",
    activeBd: "var(--gold-soft)",
    activeFg: "var(--gold)",
    filled: true,
  },
  {
    k: "SELL",
    label: "Sell",
    dotColor: "var(--danger)",
    activeBg: "rgba(255,77,77,0.10)",
    activeBd: "rgba(255,77,77,0.45)",
    activeFg: "var(--danger)",
    filled: true,
  },
  {
    k: "HOLD",
    label: "Hold",
    dotColor: "var(--text-3)",
    activeBg: "var(--surface-elev)",
    activeBd: "var(--border-strong)",
    activeFg: "var(--text)",
    filled: true,
  },
  {
    k: "FILTERED",
    label: "Filtered",
    dotColor: "var(--text-3)",
    activeBg: "transparent",
    activeBd: "var(--text-3)",
    activeFg: "var(--text-2)",
    filled: false,
  },
];

function fmtRowTime(t: string): string {
  const d = new Date(t);
  if (Number.isNaN(d.getTime())) return t;
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  const ss = String(d.getUTCSeconds()).padStart(2, "0");
  const ms = String(d.getUTCMilliseconds()).padStart(3, "0");
  return `${hh}:${mm}:${ss}.${ms}`;
}

function fmtPnl(pnl: number | null | undefined): string {
  if (pnl == null || pnl === 0) return "—";
  const abs = Math.abs(pnl).toLocaleString("en-US", { maximumFractionDigits: 2 });
  return pnl > 0 ? `+$${abs}` : `−$${abs}`;
}

export function DecisionsTable({
  decisions,
  focusedIdx,
  onJump,
}: {
  decisions: TimelineDecision[];
  focusedIdx: number | null;
  onJump: (i: number) => void;
}) {
  const [search, setSearch] = useState("");
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [sortKey, setSortKey] = useState<SortKey>("time-asc");
  const [focused, setFocused] = useState(false);

  const counts = useMemo(() => actionCounts(decisions), [decisions]);

  const filteredView = useMemo(() => {
    let out = decisions.filter((d) => matchesActionFilter(d, actionFilter));
    const q = search.toLowerCase().trim();
    if (q) out = out.filter((d) => searchHay(d).includes(q));
    return sortDecisions(out, sortKey);
  }, [decisions, search, actionFilter, sortKey]);

  const engagedCount = useMemo(
    () => decisions.filter((d) => d.phase !== "filtered").length,
    [decisions],
  );

  return (
    <div className="bg-surface-card border border-border rounded-card">
      <div
        className="flex items-center justify-between px-5 pt-4 pb-3"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        <div className="flex items-baseline gap-3">
          <h2 className="m-0 font-sans text-[22px] tracking-tight text-text" style={{ fontWeight: 600 }}>
            Decisions
          </h2>
          <span className="text-[11px] font-mono text-text-3">
            {filteredView.length} of {decisions.length} steps · {engagedCount} engaged
          </span>
        </div>
        <span className="text-[10px] font-mono text-text-3">click row → focus</span>
      </div>

      {/* Toolbar — search + sort */}
      <div
        className="px-5 pt-4 pb-3 flex items-center gap-3"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        <div
          className="flex items-center gap-2 px-3 h-8 flex-1 max-w-[320px] text-text-3"
          style={{
            background: "var(--surface-elev)",
            border: `1px solid ${focused ? "var(--gold-soft)" : "var(--border)"}`,
            borderRadius: 4,
          }}
        >
          <Icon name="search" size={13} />
          <input
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            onFocus={() => setFocused(true)}
            onBlur={() => setFocused(false)}
            placeholder="Search decisions… (id, justification, action)"
            spellCheck={false}
            aria-label="Search decisions"
            className="flex-1 bg-transparent border-none outline-none text-text text-[12.5px] font-mono"
            style={{ minWidth: 0 }}
          />
          {search && (
            <button
              type="button"
              onClick={() => setSearch("")}
              className="text-text-3 hover:text-text text-[14px] leading-none px-1"
              aria-label="Clear search"
            >
              ×
            </button>
          )}
        </div>

        <div className="ml-auto flex items-center gap-2">
          <span className="text-[10px] font-mono tracking-[0.16em] text-text-3 uppercase">Sort</span>
          <select
            value={sortKey}
            onChange={(e) => setSortKey(e.target.value as SortKey)}
            aria-label="Sort decisions"
            className="h-8 px-2 text-[12px] font-mono text-text"
            style={{
              background: "var(--surface-elev)",
              border: "1px solid var(--border)",
              borderRadius: 4,
              outline: "none",
            }}
          >
            <option value="time-asc">Time ↑ (oldest first)</option>
            <option value="time-desc">Time ↓ (newest first)</option>
            <option value="conv-desc">Conviction high → low</option>
            <option value="pnl-desc">PnL high → low</option>
          </select>
        </div>
      </div>

      {/* Filter pill row */}
      <div
        className="px-5 py-3 flex items-center gap-2 flex-wrap"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        {PILLS.map((p) => {
          const isActive = actionFilter === p.k;
          return (
            <button
              key={p.k}
              type="button"
              onClick={() => setActionFilter(p.k)}
              aria-pressed={isActive}
              className="inline-flex items-center gap-2 h-7 px-2.5 rounded-full text-[11.5px] font-mono transition-colors"
              style={{
                background: isActive ? p.activeBg : "transparent",
                border: `1px solid ${isActive ? p.activeBd : "var(--border)"}`,
                color: isActive ? p.activeFg : "var(--text-2)",
              }}
            >
              <span
                aria-hidden
                style={{
                  width: 6,
                  height: 6,
                  borderRadius: "50%",
                  background: p.filled ? p.dotColor : "transparent",
                  border: p.filled ? "none" : `1px solid ${p.dotColor}`,
                }}
              />
              <span>{p.label}</span>
              <span
                className="px-1.5 h-[16px] inline-flex items-center justify-center tabular-nums"
                style={{
                  fontSize: 10,
                  borderRadius: 2,
                  color: isActive ? p.activeFg : "var(--text-3)",
                  background: "rgba(0,0,0,0.35)",
                }}
              >
                {counts[p.k]}
              </span>
            </button>
          );
        })}
      </div>

      {/* Density strip */}
      <DecisionTimeline
        decisions={decisions}
        focusedIdx={focusedIdx}
        onJump={onJump}
        activeFilter={actionFilter}
      />

      <div className="overflow-x-auto xvn-scroll">
        <table className="w-full text-[11px] font-mono">
          <thead style={{ background: "var(--surface-elev)" }}>
            <tr className="text-left text-text-3">
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-10">#</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-32">TIMESTAMP</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-24">PHASE</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-20">ACTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-28">CONVICTION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px]">JUSTIFICATION</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-24 text-right">PNL</th>
            </tr>
          </thead>
          <tbody>
            {filteredView.length === 0 ? (
              <tr>
                <td colSpan={7} className="px-4 py-8 text-center text-text-3">
                  No decisions match these filters.
                </td>
              </tr>
            ) : (
              filteredView.map((d) => {
                const focus = d.i === focusedIdx;
                const isFiltered = d.phase === "filtered";
                return (
                  <tr
                    key={d.i}
                    onClick={() => onJump(d.i)}
                    className={`cursor-pointer transition-colors ${
                      !focus && !isFiltered ? "hover:bg-surface-hover" : ""
                    }`}
                    style={{
                      borderTop: "1px solid var(--border-soft)",
                      background: focus ? "var(--gold-bg)" : "transparent",
                      opacity: isFiltered ? 0.78 : 1,
                    }}
                  >
                    <td className="px-4 py-2 tabular-nums text-text-3">{d.i}</td>
                    <td className="px-4 py-2 tabular-nums text-text-2">{fmtRowTime(d.t)}</td>
                    <td className="px-4 py-2">
                      <PhaseChip phase={d.phase} />
                    </td>
                    <td className="px-4 py-2">
                      {isFiltered || !d.action ? (
                        <span className="text-text-4">—</span>
                      ) : (
                        <ActionPill action={d.action} />
                      )}
                    </td>
                    <td className="px-4 py-2 tabular-nums text-text">
                      {isFiltered || d.conv == null ? (
                        <span className="text-text-4">—</span>
                      ) : (
                        <div className="flex items-center gap-2">
                          <span className="w-9 text-right">{(d.conv * 100).toFixed(0)}%</span>
                          <span
                            className="flex-1 h-1 rounded-full overflow-hidden max-w-[70px]"
                            style={{ background: "var(--border)" }}
                          >
                            <span
                              className="block h-full"
                              style={{ width: `${d.conv * 100}%`, background: "var(--gold)" }}
                            />
                          </span>
                        </div>
                      )}
                    </td>
                    <td className="px-4 py-2 text-text-2 truncate max-w-[1px]">
                      {isFiltered || !d.just ? <span className="text-text-4">—</span> : d.just}
                    </td>
                    <td
                      className="px-4 py-2 tabular-nums text-right"
                      style={{
                        color: isFiltered
                          ? "var(--text-4)"
                          : d.pnl != null && d.pnl > 0
                            ? "var(--gold)"
                            : d.pnl != null && d.pnl < 0
                              ? "var(--danger)"
                              : "var(--text-4)",
                      }}
                    >
                      {isFiltered ? "—" : fmtPnl(d.pnl)}
                    </td>
                  </tr>
                );
              })
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
