// src/features/marketplace/components/TxChip.tsx
import { TestnetBadge } from "./TestnetBadge";

const TESTNETS = ["mantle-sepolia", "sepolia", "testnet"];

/**
 * Builds the canonical block-explorer URL for a transaction hash.
 * Uses the canonical Mantle explorers (NOT mantlescan.xyz):
 *   - mantle-sepolia / sepolia / testnet  → https://explorer.sepolia.mantle.xyz
 *   - mainnet mantle                      → https://explorer.mantle.xyz
 */
function explorerTxUrl(hash: string, network?: string): string {
  if (!network) return `https://explorer.sepolia.mantle.xyz/tx/${hash}`;
  if (network.includes("sepolia") || network.includes("testnet")) {
    return `https://explorer.sepolia.mantle.xyz/tx/${hash}`;
  }
  if (network.includes("mantle")) return `https://explorer.mantle.xyz/tx/${hash}`;
  return "#";
}

export function TxChip({ hash, label, network }: { hash: string; label?: string; network?: string }) {
  const isTestnet = !!network && TESTNETS.some((t) => network.includes(t));
  return (
    <span className="inline-flex items-center gap-1 font-mono text-[11px] text-text-2">
      {label ? <span className="text-text-3 uppercase tracking-wide">{label}</span> : null}
      {isTestnet ? <TestnetBadge /> : null}
      <a
        href={explorerTxUrl(hash, network)}
        target="_blank"
        rel="noreferrer"
        className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded-sm border border-border-strong hover:text-text"
      >
        {hash}
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
          <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </a>
    </span>
  );
}
