import type { RunChartPayload } from "@/api/types.gen";

type ActiveMarker = { kind: "trade" | "veto" | "hold"; decision_index: number };

type Props = {
  payload: RunChartPayload;
  active: ActiveMarker | null;
  onClose: () => void;
};

function fmt(n: number, decimals = 2) {
  return n.toLocaleString("en-US", {
    minimumFractionDigits: decimals,
    maximumFractionDigits: decimals,
  });
}

export function MarkerSidePanel({ payload, active, onClose }: Props) {
  if (!active) return null;

  const idx = active.decision_index;
  const trade = payload.markers.trades.find((t) => t.decision_index === idx);
  const veto = payload.markers.vetoes.find((v) => v.decision_index === idx);
  const hold = payload.markers.holds.find((h) => h.decision_index === idx);

  const ts = trade ?? veto ?? hold;
  if (!ts) return null;

  const date = new Date((ts.time as number) * 1000).toLocaleString();

  return (
    <aside className="absolute right-0 top-0 h-full w-80 border-l border-border bg-surface p-4 text-[13px] overflow-auto z-20">
      <div className="flex justify-between items-start mb-3">
        <strong className="text-text">Decision #{idx}</strong>
        <button onClick={onClose} className="text-text-3 hover:text-text leading-none text-lg">
          ×
        </button>
      </div>
      <p className="text-text-3 text-[11px] mb-3">{date}</p>

      {trade && (
        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-text-3 mb-2">
            Trade · {trade.side} · {payload.asset}
          </div>
          <Row label="Side" value={trade.side} />
          <Row label="Price" value={`$${fmt(trade.price)}`} />
          <Row label="Size" value={fmt(trade.size, 6)} />
          <Row label="Fee" value={`$${fmt(trade.fee, 4)}`} />
          {trade.pnl_realized !== null && (
            <Row
              label="PnL"
              value={`$${fmt(trade.pnl_realized)}`}
              highlight={trade.pnl_realized >= 0 ? "green" : "red"}
            />
          )}
          {trade.justification && (
            <div className="mt-2">
              <div className="text-[11px] text-text-3 mb-1">Justification</div>
              <p className="text-[12px] text-text-2 whitespace-pre-wrap">{trade.justification}</p>
            </div>
          )}
        </div>
      )}

      {veto && (
        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-text-3 mb-2">
            Veto
          </div>
          <Row label="Price" value={`$${fmt(veto.price)}`} />
          <div className="mt-2">
            <div className="text-[11px] text-text-3 mb-1">Reason</div>
            <p className="text-[12px] text-text-2 whitespace-pre-wrap">{veto.reason}</p>
          </div>
        </div>
      )}

      {hold && (
        <div className="space-y-1.5">
          <div className="text-[11px] font-medium uppercase tracking-wide text-text-3 mb-2">
            Hold
          </div>
          <Row label="Price" value={`$${fmt(hold.price)}`} />
          {hold.conviction !== null && (
            <Row label="Conviction" value={fmt(hold.conviction, 3)} />
          )}
        </div>
      )}
    </aside>
  );
}

function Row({
  label,
  value,
  highlight,
}: {
  label: string;
  value: string;
  highlight?: "green" | "red";
}) {
  const color =
    highlight === "green"
      ? "text-green-400"
      : highlight === "red"
        ? "text-red-400"
        : "text-text";
  return (
    <div className="flex justify-between gap-2">
      <span className="text-text-3">{label}</span>
      <span className={`font-mono ${color}`}>{value}</span>
    </div>
  );
}
