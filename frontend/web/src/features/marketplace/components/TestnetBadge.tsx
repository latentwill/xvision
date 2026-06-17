// src/features/marketplace/components/TestnetBadge.tsx
//
// One shared "Testnet" affordance for the marketplace. Every chain-bound
// surface (buy / mint / receipt / install actions, listing prices, tx chips)
// labels the network so the testnet (Mantle Sepolia) nature is honest to the
// user. Theme-token styling only — warn tones with dark-safe opacity, never
// hard white/gray borders.
//
// On MAINNET these render nothing / an accurate real-funds notice: a "Testnet ·
// purchases are simulated" claim on mainnet would be false and unsafe. The
// network is resolved at RUNTIME from the backend (useMarketplaceNetwork) so a
// prebuilt bundle reflects whatever chain the backend is on — not the build-time
// VITE_MARKETPLACE_NETWORK.

import { useMarketplaceNetwork } from "../lib/useMarketplaceNetwork";

interface TestnetBadgeProps {
  /** "xs" for inline pills next to CTAs/prices, "sm" for standalone rows. */
  size?: "xs" | "sm";
  className?: string;
}

export function TestnetBadge({ size = "xs", className = "" }: TestnetBadgeProps) {
  const { isMainnet } = useMarketplaceNetwork();
  // No testnet to flag on mainnet — never mislabel a real-funds surface.
  if (isMainnet) return null;
  const sizing =
    size === "sm"
      ? "px-1.5 py-0.5 text-[10px]"
      : "px-1 py-px text-[9px]";
  return (
    <span
      className={[
        "inline-flex items-center rounded-[3px] border border-warn/40 text-warn",
        "uppercase font-mono tracking-[0.06em] whitespace-nowrap",
        sizing,
        className,
      ].join(" ")}
    >
      Testnet
    </span>
  );
}

// Page-level banner for the marketplace shell. Quiet, full-width, single row —
// no right-side box (chat rail owns the right), no popup.
export function TestnetBanner({ className = "" }: { className?: string }) {
  const { isMainnet } = useMarketplaceNetwork();
  // Mainnet is the normal state — render nothing (no "Mainnet" banner). The
  // banner exists only to warn that testnet purchases are simulated; on mainnet
  // there is nothing to warn about, so we suppress it entirely (operator
  // decision 2026-06-16: testnet banner stays, no mainnet banner).
  if (isMainnet) {
    return null;
  }
  return (
    <div
      className={[
        "flex items-center gap-2 rounded-md border border-warn/30 bg-warn/[0.06]",
        "px-3 py-2 text-[12px] text-text-2",
        className,
      ].join(" ")}
    >
      <TestnetBadge size="sm" />
      <span>
        The marketplace runs on the{" "}
        <span className="text-text">Mantle Sepolia testnet</span>. Purchases are
        simulated — no real funds move.
      </span>
    </div>
  );
}
