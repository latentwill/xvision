// Venue account panel — the REAL execution-venue account behind live runs.
//
// Where `LiveAccountStrip` derives stats from the selected run's eval
// stream, this panel reports the venue's own ledger (Orderly Network:
// equity, USDC holding, unrealized PnL, open positions) plus the connected
// browser wallet address. Full-width inline band (NO right-side box; no
// popups). Disconnected venue renders a quiet one-line state, never an
// error surface.

import { useQuery } from "@tanstack/react-query";

import { getVenueAccount, liveKeys } from "@/api/live";
import { useWallet } from "@/features/marketplace/lib/wallet";

import { DASH, fmtUsdPlain, fmtUsdSigned, pnlTone } from "./live-format";

function shortAddr(addr: string): string {
  return addr.length > 12 ? `${addr.slice(0, 6)}…${addr.slice(-4)}` : addr;
}

export function VenueAccountPanel() {
  const { address } = useWallet();
  const query = useQuery({
    queryKey: liveKeys.venueAccount(),
    queryFn: getVenueAccount,
    refetchInterval: 15_000,
  });
  const acct = query.data;

  return (
    <section
      data-testid="venue-account-panel"
      className="rounded-card border border-border bg-surface-card"
    >
      <header className="flex flex-wrap items-center gap-x-3 gap-y-1 border-b border-border px-4 py-2.5">
        <span className="text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
          Venue account
        </span>
        {acct?.connected ? (
          <span className="inline-flex items-center gap-1.5 rounded-full border border-gold/30 bg-gold/10 px-2 py-0.5 text-[10px] font-mono uppercase tracking-[0.12em] text-gold">
            <span className="h-1.5 w-1.5 rounded-full bg-gold" />
            {acct.venue} · {acct.network ?? "?"}
          </span>
        ) : (
          <span className="inline-flex items-center gap-1.5 rounded-full border border-border bg-surface px-2 py-0.5 text-[10px] font-mono uppercase tracking-[0.12em] text-text-3">
            <span className="h-1.5 w-1.5 rounded-full bg-text-3/50" />
            not connected
          </span>
        )}
        {address != null && (
          <span
            className="text-[11px] font-mono text-text-3"
            title={address}
            data-testid="venue-wallet-addr"
          >
            wallet {shortAddr(address)}
          </span>
        )}
        {!acct?.connected && acct?.reason != null && (
          <span className="text-[11px] text-text-3">{acct.reason}</span>
        )}
      </header>

      {acct?.connected && (
        <>
          <div className="grid grid-cols-2 gap-px bg-border sm:grid-cols-3">
            <Stat label="Venue equity" value={fmtUsdPlain(acct.equity_usd ?? null)} />
            <Stat label="USDC holding" value={fmtUsdPlain(acct.usdc_holding ?? null)} />
            <Stat
              label="Unrealized PnL"
              value={fmtUsdSigned(acct.unrealized_pnl ?? null)}
              tone={pnlTone(acct.unrealized_pnl ?? null)}
            />
          </div>

          {acct.positions.length > 0 ? (
            <table className="w-full text-[12px] tabular-nums">
              <thead>
                <tr className="border-t border-border text-left text-[10px] font-mono uppercase tracking-[0.14em] text-text-3">
                  <th className="px-4 py-2 font-normal">Market</th>
                  <th className="px-4 py-2 font-normal">Qty</th>
                  <th className="px-4 py-2 font-normal">Entry</th>
                  <th className="px-4 py-2 font-normal">Mark</th>
                  <th className="px-4 py-2 text-right font-normal">uPnL</th>
                </tr>
              </thead>
              <tbody>
                {acct.positions.map((p) => (
                  <tr key={p.symbol} className="border-t border-border">
                    <td className="px-4 py-2 font-mono">{p.symbol}</td>
                    <td className={`px-4 py-2 ${p.qty >= 0 ? "text-gold" : "text-danger"}`}>
                      {p.qty}
                    </td>
                    <td className="px-4 py-2">{fmtUsdPlain(p.entry_price)}</td>
                    <td className="px-4 py-2">{fmtUsdPlain(p.mark_price)}</td>
                    <td className={`px-4 py-2 text-right ${pnlTone(p.unrealized_pnl)}`}>
                      {fmtUsdSigned(p.unrealized_pnl)}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          ) : (
            <div className="border-t border-border px-4 py-2.5 text-[12px] text-text-3">
              No open venue positions
            </div>
          )}
        </>
      )}
    </section>
  );
}

function Stat({
  label,
  value,
  tone = "text-text",
}: {
  label: string;
  value: string;
  tone?: string;
}) {
  return (
    <div className="bg-surface-card px-4 py-3">
      <div className="text-[10px] font-mono uppercase tracking-[0.16em] text-text-3">
        {label}
      </div>
      <div className={`mt-1 text-[16px] font-semibold tabular-nums ${tone}`}>
        {value ?? DASH}
      </div>
    </div>
  );
}
