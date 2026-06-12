// src/features/marketplace/routes/TradeHistoryTable.tsx
// On-chain trade ledger inside the receipts drawer.
// Filter pills (All/Buy/Sell/Close) + Runner/Window dropdowns (stub) + paginated table.
// TODO(F-runner-filter): Runner and Window dropdowns are stubs in F2 — no real filtering.
import { useState } from "react";
import { AgentIcon } from "@/features/marketplace/components/AgentIcon";
import { TxChip } from "@/features/marketplace/components/TxChip";
import type { TradeRecord } from "@/features/marketplace/data/types";

type ActionFilter = "all" | "buy" | "sell" | "close";

interface TradesMeta {
  totalOnChain: number;
  lastAnchorAt: string;
  receiptKind: string;
  netPnlUsd: number;
  window: string;
  anchorTx: string;
}

interface Props {
  trades: TradeRecord[];
  meta: TradesMeta;
}

const ACTION_CLASSES: Record<string, { fg: string; border: string; bg: string }> = {
  buy: { fg: "text-gold", border: "border-gold-soft", bg: "bg-gold/[0.10]" },
  sell: { fg: "text-danger", border: "border-danger/40", bg: "bg-danger/[0.10]" },
  close: { fg: "text-info", border: "border-info/40", bg: "bg-info/[0.10]" },
};

const PAGE_SIZE = 10;

export function TradeHistoryTable({ trades, meta }: Props) {
  const [actionFilter, setActionFilter] = useState<ActionFilter>("all");
  const [page, setPage] = useState(0);

  const filtered =
    actionFilter === "all" ? trades : trades.filter((t) => t.action === actionFilter);

  const pageSlice = filtered.slice(page * PAGE_SIZE, (page + 1) * PAGE_SIZE);
  const totalPages = Math.ceil(filtered.length / PAGE_SIZE);

  const counts = {
    all: trades.length,
    buy: trades.filter((t) => t.action === "buy").length,
    sell: trades.filter((t) => t.action === "sell").length,
    close: trades.filter((t) => t.action === "close").length,
  };

  return (
    <div className="rounded-md border border-border bg-surface-card">
      {/* Card header */}
      <div className="flex items-center justify-between px-4 py-3 border-b border-border">
        <div>
          <span className="text-[12px] font-medium text-text">Trade history</span>
          <span className="ml-2 font-mono text-[10.5px] text-text-3">
            {meta.totalOnChain} trades on chain · last anchor {meta.lastAnchorAt} ·
            receipt_kind={meta.receiptKind}
          </span>
        </div>
        <button className="px-2.5 py-1 rounded border border-border-strong text-[11px] font-medium text-text-2 hover:text-text transition-colors">
          Export ledger
        </button>
      </div>

      {/* Filter pills row */}
      <div className="flex items-center flex-wrap gap-2 px-4 py-2.5 border-b border-border-soft">
        {(["all", "buy", "sell", "close"] as ActionFilter[]).map((k) => {
          const active = actionFilter === k;
          const cls = k !== "all" ? ACTION_CLASSES[k] : null;
          return (
            <button
              key={k}
              onClick={() => {
                setActionFilter(k);
                setPage(0);
              }}
              className={[
                "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border text-[11.5px] font-medium",
                active
                  ? cls
                    ? `${cls.border} ${cls.bg} ${cls.fg}`
                    : "border-gold-soft bg-gold/[0.10] text-gold"
                  : "border-border-strong bg-transparent text-text-2",
              ].join(" ")}
            >
              <span
                className={[
                  "w-1.5 h-1.5 rounded-full",
                  k === "all"
                    ? "bg-text-3"
                    : (cls?.fg.replace("text-", "bg-") ?? "bg-text-3"),
                ].join(" ")}
              />
              <span className="capitalize">{k}</span>
              <span className="font-mono text-[10px] px-1">{counts[k]}</span>
            </button>
          );
        })}

        <span className="w-px h-4 bg-border mx-1" />

        {/* Runner dropdown stub — TODO(F-runner-filter) */}
        <button className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border border-border-strong text-[11.5px] font-medium text-text-2">
          <span>Runner:</span>{" "}
          <span className="font-mono text-[10.5px] text-text-3">any</span>
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            className="text-text-3"
          >
            <path
              d="M2 3.5l3 3 3-3"
              stroke="currentColor"
              strokeWidth="1.5"
              fill="none"
              strokeLinecap="round"
            />
          </svg>
        </button>

        {/* Window dropdown stub — TODO(F-runner-filter) */}
        <button className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-[3px] border border-border-strong text-[11.5px] font-medium text-text-2">
          Window{" "}
          <span className="font-mono text-[10.5px] text-text-3">{meta.window}</span>
          <svg
            width="10"
            height="10"
            viewBox="0 0 10 10"
            className="text-text-3"
          >
            <path
              d="M2 3.5l3 3 3-3"
              stroke="currentColor"
              strokeWidth="1.5"
              fill="none"
              strokeLinecap="round"
            />
          </svg>
        </button>

        <span className="ml-auto font-mono text-[10.5px] text-text-3">
          net <span className="text-gold">+${meta.netPnlUsd}</span> · {meta.window} window
        </span>
      </div>

      {/* Table */}
      <div>
        {/* Header row */}
        <div
          className="grid items-center gap-2.5 px-4 py-2 border-b border-border-soft"
          style={{ gridTemplateColumns: "100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px" }}
        >
          {["Time", "Action", "Sym", "Qty", "Entry", "Exit", "P&L", "Runner", "Tx"].map(
            (h, i) => (
              <div
                key={h}
                className="font-mono text-[9px] tracking-[0.2em] font-semibold text-text-3 uppercase"
                style={{ textAlign: i >= 3 && i <= 6 ? "right" : "left" }}
              >
                {h}
              </div>
            ),
          )}
        </div>

        {/* Data rows */}
        {pageSlice.map((t, i) => {
          const ac = ACTION_CLASSES[t.action];
          const pnlPos = (t.pnlUsd ?? 0) > 0;
          const pnlOpen = t.pnlUsd === null;
          const pnlColor = pnlOpen ? "text-info" : pnlPos ? "text-gold" : "text-danger";
          const isAgent = t.runnerKind === "agent";

          return (
            <div
              key={i}
              className={[
                "grid items-center gap-2.5 px-4 py-2.5",
                i < pageSlice.length - 1 ? "border-b border-border-soft" : "",
              ].join(" ")}
              style={{ gridTemplateColumns: "100px 78px 50px 0.7fr 0.9fr 0.9fr 0.95fr 1fr 110px" }}
            >
              <span className="font-mono text-[11px] text-text-3">{t.at}</span>
              <span
                className={[
                  "inline-flex items-center gap-1 px-1.5 py-0.5 rounded-[3px] border font-mono text-[9.5px] tracking-[0.16em] font-semibold uppercase",
                  ac.border,
                  ac.bg,
                  ac.fg,
                ].join(" ")}
              >
                <span className="w-1 h-1 rounded-full bg-current" />
                {t.action}
              </span>
              <span className="font-mono text-[11.5px] text-text-2">{t.symbol}</span>
              <span className="font-mono text-[11.5px] text-text text-right">{t.qty}</span>
              <span className="font-mono text-[11.5px] text-text-2 text-right">
                {t.entry !== null ? `$${t.entry.toLocaleString()}` : "—"}
              </span>
              <span
                className={[
                  "font-mono text-[11.5px] text-right",
                  t.exit === null ? "text-text-4" : "text-text-2",
                ].join(" ")}
              >
                {t.exit !== null ? `$${t.exit.toLocaleString()}` : "—"}
              </span>
              <span className="flex flex-col items-end gap-0.5">
                <span className={`font-mono text-[12px] font-semibold ${pnlColor}`}>
                  {pnlOpen
                    ? "open"
                    : `${(t.pnlUsd ?? 0) > 0 ? "+" : ""}$${t.pnlUsd}`}
                </span>
                {!pnlOpen && t.pnlPct !== null && (
                  <span className="font-mono text-[9.5px] text-text-3">
                    {(t.pnlPct ?? 0) > 0 ? "+" : ""}
                    {t.pnlPct}%
                  </span>
                )}
              </span>
              <span className="inline-flex items-center gap-1.5 min-w-0 overflow-hidden">
                <span
                  className={[
                    "w-4 h-4 flex-shrink-0 flex items-center justify-center border",
                    isAgent
                      ? "rounded-[3px] border-gold-soft bg-gold/[0.10]"
                      : "rounded-full border-border-strong bg-surface-elev",
                  ].join(" ")}
                >
                  {isAgent && <AgentIcon size={8} />}
                </span>
                <span
                  className={[
                    "font-mono text-[11px] truncate",
                    isAgent ? "text-gold" : "text-text-2",
                  ].join(" ")}
                >
                  {t.runner}
                </span>
              </span>
              <TxChip hash={t.tx} network="" />
            </div>
          );
        })}
      </div>

      {/* Footer */}
      <div className="flex items-center gap-2.5 px-4 py-2.5 border-t border-border-soft">
        <span className="font-mono text-[10.5px] text-text-3">
          Showing <span className="text-text-2">{pageSlice.length}</span> of{" "}
          <span className="text-text-2">{filtered.length}</span> · all anchored under{" "}
          <span className="text-info">{meta.anchorTx}</span>
        </span>
        <div className="ml-auto flex items-center gap-1.5">
          <button
            onClick={() => setPage((p) => Math.max(0, p - 1))}
            disabled={page === 0}
            className="px-2 py-1 rounded border border-border-strong text-[11px] text-text-2 disabled:opacity-40"
          >
            ← Prev
          </button>
          <button
            onClick={() => setPage((p) => Math.min(totalPages - 1, p + 1))}
            disabled={page >= totalPages - 1}
            className="px-2 py-1 rounded border border-border-strong text-[11px] text-text-2 disabled:opacity-40"
          >
            Next →
          </button>
        </div>
      </div>
    </div>
  );
}
