// src/features/marketplace/routes/ReceiptsDrawer.tsx
//
// Inline-expand accordion for on-chain receipts.
// NO modal, sheet, or popover — this is the law.
// Parent controls open state (URL-backed via ?receipts=open).
// Body contains: Identity NFT & manifest, attestation verdicts,
// anchor history, and TradeHistoryTable.
import { TxChip } from "@/features/marketplace/components/TxChip";
import { TradeHistoryTable } from "./TradeHistoryTable";
import type { OnChainReceipts, Verdict } from "@/features/marketplace/data/types";

interface Props {
  onChain: OnChainReceipts;
  open: boolean;
  onToggle: () => void;
}

const VERDICT_CLASSES: Record<Verdict, string> = {
  endorse: "border-gold-soft text-gold",
  question: "border-warn/60 text-warn",
  reject: "border-danger/60 text-danger",
};

const ANCHOR_KIND_CLASSES: Record<string, string> = {
  merkle: "text-info",
  mint: "text-gold",
  commit: "text-text-2",
};

export function ReceiptsDrawer({ onChain, open, onToggle }: Props) {
  return (
    <div className={["mt-6 border-t border-border", open ? "bg-[#070707]" : ""].join(" ")}>
      {/* === TOGGLE ROW === */}
      <button
        data-testid="receipts-toggle"
        onClick={onToggle}
        className="w-full flex items-center gap-2.5 px-7 py-3.5 text-left hover:bg-surface-hover transition-colors"
      >
        {/* Chevron — right when closed, down when open */}
        <svg
          width="13"
          height="13"
          viewBox="0 0 13 13"
          className={[
            "text-text-2 transition-transform",
            open ? "rotate-90" : "",
          ].join(" ")}
        >
          <path
            d="M4.5 2.5 l4 4 -4 4"
            stroke="currentColor"
            strokeWidth="1.5"
            fill="none"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
        <span className="text-[13.5px] font-medium text-text">
          {open ? "Hide" : "View"} on-chain receipts
        </span>
        <span className="font-mono text-[11px] text-text-3">
          · NFT, manifest hash, attestations, anchor history, validator activity
        </span>
        <span className="ml-auto flex items-center gap-1.5">
          <span className="font-mono text-[9px] tracking-[0.18em] text-text-3 uppercase">
            Auditor
          </span>
          {/* Shield icon */}
          <svg width="11" height="11" viewBox="0 0 11 11" className="text-text-3">
            <path
              d="M5.5 1 L9.5 2.5 v3.5 c0 2-4 4-4 4s-4-2-4-4 V2.5 Z"
              stroke="currentColor"
              strokeWidth="1.2"
              fill="none"
            />
          </svg>
        </span>
      </button>

      {/* === BODY (only when open) === */}
      {open && (
        <div
          data-testid="receipts-body"
          className="px-7 pb-7 grid gap-4"
          style={{ gridTemplateColumns: "1fr 1fr" }}
        >
          {/* Identity NFT & manifest */}
          <div className="rounded-md border border-border bg-surface-card p-4">
            <div className="text-[12px] font-medium text-text mb-0.5">
              Identity NFT &amp; manifest
            </div>
            <div className="font-mono text-[10.5px] text-text-3 mb-3">
              {onChain.nft.network} · {onChain.nft.contract}
            </div>
            <div className="space-y-0">
              {(
                [
                  ["nft_token_id", onChain.nft.tokenId, "gold"],
                  ["lineage_id", onChain.nft.lineageId, "text"],
                  ["agentURI", onChain.nft.agentURI, "link"],
                  ["manifest_hash", onChain.nft.manifestHash, "text"],
                  ["parent_lineage", onChain.nft.parentLineage ?? "— (seed)", "muted"],
                  ["born_at", onChain.nft.bornAt, "text"],
                  ["operator_sig", onChain.nft.operatorSig, "text"],
                ] as [string, string, string][]
              ).map(([key, val, tone], i, arr) => (
                <div
                  key={key}
                  className={[
                    "grid gap-2.5 py-1.5",
                    i < arr.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                  style={{ gridTemplateColumns: "120px 1fr" }}
                >
                  <span className="font-mono text-[9.5px] tracking-[0.14em] text-text-3 uppercase">
                    {key}
                  </span>
                  <span
                    className={[
                      "font-mono text-[11px] break-all",
                      tone === "gold"
                        ? "text-gold"
                        : tone === "muted"
                          ? "text-text-3"
                          : tone === "link"
                            ? "text-info underline decoration-dotted"
                            : "text-text",
                    ].join(" ")}
                  >
                    {val}
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Attestation verdicts — hidden when there are none. API
              listings always arrive with an empty `attestations` array
              (verdicts are a fixture-only concept for now), and rendering
              "0 verdicts" reads like a trust failure rather than absent
              data. */}
          {onChain.attestations.length > 0 && (
          <div className="rounded-md border border-border bg-surface-card">
            <div className="px-4 py-3 border-b border-border">
              <span className="text-[12px] font-medium text-text">
                Attestation verdicts
              </span>
              <span className="ml-2 font-mono text-[10.5px] text-text-3">
                {onChain.attestations.length} verdicts
              </span>
            </div>
            <div>
              {onChain.attestations.map((att, i) => (
                <div
                  key={i}
                  className={[
                    "flex items-center gap-2.5 px-4 py-2.5",
                    i < onChain.attestations.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                >
                  <span
                    className={[
                      "inline-flex items-center gap-1.5 min-w-[80px] px-1.5 py-0.5 rounded-[3px] border font-mono text-[9.5px] tracking-[0.14em] font-semibold uppercase",
                      VERDICT_CLASSES[att.verdict],
                    ].join(" ")}
                  >
                    <span className="w-1.5 h-1.5 rounded-full bg-current" />
                    {att.verdict}
                  </span>
                  <span className="font-mono text-[11px] text-text-2">{att.attester}</span>
                  <span className="font-mono text-[11px] text-text ml-auto">
                    → {att.targetVersion}
                  </span>
                  <span className="font-mono text-[10.5px] text-text-3">{att.at}</span>
                </div>
              ))}
            </div>
          </div>
          )}

          {/* Anchor history — full width */}
          <div className="rounded-md border border-border bg-surface-card col-span-2">
            <div className="px-4 py-3 border-b border-border">
              <span className="text-[12px] font-medium text-text">Anchor history</span>
              <span className="ml-2 font-mono text-[10.5px] text-text-3">
                {onChain.anchors.length} events
              </span>
            </div>
            <div>
              {onChain.anchors.map((anc, i) => (
                <div
                  key={i}
                  className={[
                    "grid items-center gap-3.5 px-4 py-2.5",
                    i < onChain.anchors.length - 1 ? "border-b border-border-soft" : "",
                  ].join(" ")}
                  style={{ gridTemplateColumns: "110px 1fr auto auto" }}
                >
                  <span
                    className={[
                      "font-mono text-[9.5px] tracking-[0.16em] uppercase",
                      ANCHOR_KIND_CLASSES[anc.kind] ?? "text-text-2",
                    ].join(" ")}
                  >
                    {anc.kind}
                  </span>
                  <span className="font-mono text-[11.5px] text-text-2">{anc.label}</span>
                  <TxChip hash={anc.tx} network={onChain.nft.network} />
                  <span className="font-mono text-[11px] text-text-3 text-right min-w-[90px]">
                    {anc.at} · {anc.gasEth} ETH
                  </span>
                </div>
              ))}
            </div>
          </div>

          {/* Trade history — full width */}
          <div className="col-span-2">
            <TradeHistoryTable trades={onChain.trades} meta={onChain.tradesMeta} />
          </div>
        </div>
      )}
    </div>
  );
}
