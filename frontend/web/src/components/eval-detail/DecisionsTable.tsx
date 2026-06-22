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
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";
import type { FilterSummary } from "@/api/types.gen/FilterSummary";
import { ActionPill } from "./ActionPill";
import { PhaseChip } from "./PhaseChip";
import { DecisionTimeline } from "./DecisionTimeline";
import {
  decisionCounts,
  fmtStepStamp,
  shortAsset,
  stepOrdinalsByDecision,
  type TimelineDecision,
} from "./decision-view";
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
    k: "SHORT",
    label: "Short",
    dotColor: "rgba(200,0,0,0.85)",
    activeBg: "rgba(200,0,0,0.15)",
    activeBd: "rgba(200,0,0,0.7)",
    activeFg: "rgba(200,0,0,0.9)",
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
];

/** Aggregate the engine-filter activity across all `FilterSummary` entries.
 *
 *  Returns:
 *  - `barsScanned` — total cadence-gated bars the engine evaluated.
 *  - `wakeups` — bars where the filter fired and the trader was woken.
 *  - `suppressed` — bars that DID NOT wake the trader, for any reason.
 *    This is `bars_scanned - wakeups` (= `llm_calls_saved`), which folds
 *    together "filter conditions evaluated false" AND the three rule-based
 *    suppression counters (in-position / cooldown / daily-cap). The pill
 *    row already separates row-level NO-OP (synthesized decisions); this
 *    counter is strictly the engine-gate-rejected bars that never produced
 *    a decision row at all — the number the operator means when they say
 *    "the filter rejected 1399 of 1404 bars."
 *
 *  Returns `null` when there's no activity to report (no summaries, or
 *  `bars_scanned` is zero across all of them) so the activity line is
 *  omitted entirely on EveryBar runs. */
function aggregateFilterActivity(
  summaries: FilterSummary[] | undefined,
): { barsScanned: number; wakeups: number; suppressed: number } | null {
  if (!summaries || summaries.length === 0) return null;
  let barsScanned = 0;
  let wakeups = 0;
  for (const s of summaries) {
    barsScanned += s.bars_scanned;
    wakeups += s.wakeups;
  }
  if (barsScanned === 0) return null;
  return { barsScanned, wakeups, suppressed: barsScanned - wakeups };
}

const EXIT_REASON_TONE: Record<string, string> = {
  stop_loss: "text-danger border-danger/30 bg-danger/10",
  take_profit: "text-gold border-gold/30 bg-gold/10",
  trailing_stop: "text-warn border-warn/30 bg-warn/10",
  time_expiry: "text-text-2 border-border bg-surface-elev",
  signal: "text-info border-info/30 bg-info/10",
  manual: "text-text-2 border-border bg-surface-elev",
};

function exitReasonLabel(reason: string): string {
  return reason.replace(/_/g, " ");
}

function ExitReasonTag({ reason }: { reason: string }) {
  const tone = EXIT_REASON_TONE[reason] ?? "text-text-2 border-border bg-surface-elev";
  return (
    <span
      className={`inline-flex items-center px-1.5 py-0.5 text-[10px] uppercase tracking-wide rounded-sm border ${tone}`}
    >
      {exitReasonLabel(reason)}
    </span>
  );
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
  filterSummaries,
}: {
  decisions: TimelineDecision[];
  focusedIdx: number | null;
  onJump: (i: number) => void;
  /** Engine-filter activity summary from the run export. The table only shows
   *  bars where the filter fired (= rows in `decisions`); the suppressed bars
   *  are invisible without this context. Surfaced as a one-line header
   *  alongside the steps chip on FilterGated runs. Omit / pass `[]` for
   *  EveryBar runs to hide the line entirely. */
  filterSummaries?: FilterSummary[];
}) {
  const [search, setSearch] = useState("");
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [sortKey, setSortKey] = useState<SortKey>("time-asc");
  const [focused, setFocused] = useState(false);
  const [expandedIdx, setExpandedIdx] = useState<number | null>(null);

  const counts = useMemo(() => actionCounts(decisions), [decisions]);

  const filteredView = useMemo(() => {
    let out = decisions.filter((d) => matchesActionFilter(d, actionFilter));
    const q = search.toLowerCase().trim();
    if (q) out = out.filter((d) => searchHay(d).includes(q));
    return sortDecisions(out, sortKey);
  }, [decisions, search, actionFilter, sortKey]);

  // Step ordinals are keyed by decision_index and computed over the FULL list so
  // they stay stable under filtering. A multi-asset step (BTC+ETH at one
  // timestamp) shares one step number; `summary.totalSteps` is the number of
  // distinct decision steps, which is what the header should report (not the
  // per-asset row count). Blanking the step number on the 2nd+ row of a step
  // only makes sense when same-step rows are adjacent — i.e. in chronological
  // sort.
  const stepByI = useMemo(() => stepOrdinalsByDecision(decisions), [decisions]);
  const summary = useMemo(
    () => decisionCounts(filteredView, decisions),
    [filteredView, decisions],
  );
  const filterActivity = useMemo(
    () => aggregateFilterActivity(filterSummaries),
    [filterSummaries],
  );
  const isChronological = sortKey === "time-asc" || sortKey === "time-desc";

  return (
    <div className="bg-surface-card border border-border rounded-card">
      <div
        className="flex items-start justify-between px-5 pt-4 pb-3 gap-3"
        style={{ borderBottom: "1px solid var(--border-soft)" }}
      >
        <div className="flex flex-col gap-1 min-w-0">
          <div className="flex items-baseline gap-3 flex-wrap">
            <h2 className="m-0 font-sans text-[22px] tracking-tight text-text" style={{ fontWeight: 600 }}>
              Decisions
            </h2>
            {/* Step-centric counts. The legacy chip read
                  "{rows} of {rows} decisions · {steps} steps · {rows} engaged"
                and triple-counted the multi-asset fanout — a 5-step / 5-asset run
                read as "22 of 22 decisions · 5 steps · 22 engaged." We now report
                steps (the strategy's decision moments) as the primary count and
                keep the per-asset row total visible as "trader calls" so the
                operator can still see the fanout cardinality. Both step counts
                follow filtering; trader calls follow the view too. */}
            <span className="text-[11px] font-mono text-text-3">
              {summary.viewedSteps} of {summary.totalSteps}{" "}
              {summary.totalSteps === 1 ? "step" : "steps"} ·{" "}
              {summary.engagedSteps} engaged · {summary.viewedTraderCalls} trader{" "}
              {summary.viewedTraderCalls === 1 ? "call" : "calls"}
            </span>
          </div>
          {/* Engine-filter activity. Without this line the operator reads the
              steps chip in isolation and concludes "every step was engaged" —
              missing the 1399 suppressed bars that never produced a decision
              row at all. Only the bars that wake the trader become rows in
              this table; the rest are visible to the operator only through
              this header (and the FilterSummaryPanel / FilterEventTimeline
              above). Conditional render — EveryBar runs (no filterSummaries)
              get no line, so the layout doesn't shift for them. */}
          {filterActivity && (
            <span
              data-testid="decisions-filter-activity"
              className="text-[11px] font-mono text-text-3"
            >
              engine filter: {filterActivity.barsScanned.toLocaleString()}{" "}
              {filterActivity.barsScanned === 1 ? "bar" : "bars"} scanned ·{" "}
              {filterActivity.wakeups.toLocaleString()} fired ·{" "}
              {filterActivity.suppressed.toLocaleString()} suppressed
            </span>
          )}
        </div>
        <span className="text-[10px] font-mono text-text-3 shrink-0">click row → focus</span>
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
            placeholder="Search decisions… (asset, justification, action)"
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
          <SignalSelectMenu
            ariaLabel="Sort decisions"
            label="Sort"
            value={sortKey}
            options={[
              { value: "time-asc", label: "Time ↑ (oldest first)" },
              { value: "time-desc", label: "Time ↓ (newest first)" },
              { value: "conv-desc", label: "Conviction high → low" },
              { value: "pnl-desc", label: "PnL high → low" },
            ]}
            onChange={(next) => setSortKey(next as SortKey)}
            compact
          />
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

      {/* Density strip — one tick per per-asset row; header reports the
          step count derived from the same source as the summary chip above. */}
      <DecisionTimeline
        decisions={decisions}
        focusedIdx={focusedIdx}
        onJump={onJump}
        activeFilter={actionFilter}
        stepsCount={summary.totalSteps}
      />

      <div className="overflow-x-auto xvn-scroll">
        <table className="w-full text-[11px] font-mono">
          <thead style={{ background: "var(--surface-elev)" }}>
            <tr className="text-left text-text-3">
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-12">STEP</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-16">ASSET</th>
              <th className="px-4 py-2 font-normal tracking-[0.18em] text-[10px] w-44">TIMESTAMP</th>
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
                <td colSpan={8} className="px-4 py-8 text-center text-text-3">
                  No decisions match these filters.
                </td>
              </tr>
            ) : (
              filteredView.map((d, idx) => {
                const focus = d.i === focusedIdx;
                const isFiltered = d.phase === "filtered";
                // Show the step number on the first row of each step; blank the
                // 2nd+ rows of the same step (the per-asset fan-out) so the count
                // tracks decision steps, not rows. Only blank in chronological
                // sort, where same-step rows are adjacent; otherwise number every
                // row since a step's rows may be scattered.
                const prev = filteredView[idx - 1];
                const sameStepAsPrev = isChronological && prev != null && prev.t === d.t;
                return (
                  <tr
                    key={d.i}
                    onClick={() => {
                      onJump(d.i);
                      const expanded = expandedIdx === d.i;
                      setExpandedIdx(expanded ? null : d.i);
                    }}
                    className={`cursor-pointer transition-colors ${
                      !focus && !isFiltered ? "hover:bg-surface-hover" : ""
                    }`}
                    style={{
                      borderTop: "1px solid var(--border-soft)",
                      background: expandedIdx === d.i
                        ? "var(--gold-bg)"
                        : focus
                          ? "var(--gold-bg)"
                          : "transparent",
                      opacity: isFiltered ? 0.78 : 1,
                    }}
                  >
                    <td className="px-4 py-2 tabular-nums text-text-3">
                      {sameStepAsPrev ? "" : stepByI.get(d.i)}
                    </td>
                    <td className="px-4 py-2 text-text-2 whitespace-nowrap" title={d.asset}>
                      {shortAsset(d.asset)}
                    </td>
                    <td
                      className="px-4 py-2 tabular-nums text-text-2 whitespace-nowrap"
                      title={d.t}
                    >
                      {fmtStepStamp(d.t)}
                    </td>
                    <td className="px-4 py-2">
                      {/* PHASE is a step-level concept (filter fired vs not),
                          so blank the chip on per-asset child rows the same
                          way we blank the STEP number — otherwise a single
                          step renders ENGAGED N times for an N-asset universe
                          and reads as "engaged every row." Non-chronological
                          sort numbers every row, so show every chip too. */}
                      {sameStepAsPrev ? null : <PhaseChip phase={d.phase} />}
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
                    <td className={`px-4 py-2 text-text-2 ${expandedIdx === d.i ? "whitespace-normal break-words" : "truncate max-w-[1px]"}`}>
                      {isFiltered ? (
                        <span className="text-text-4">—</span>
                      ) : d.exit_reason ? (
                        <ExitReasonTag reason={d.exit_reason} />
                      ) : d.just ? (
                        d.just
                      ) : (
                        <span className="text-text-4">—</span>
                      )}
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
