// src/features/marketplace/components/TestnetBadge.tsx
//
// One shared "Testnet" affordance for the marketplace. Every chain-bound
// surface (buy / mint / receipt / install actions, listing prices, tx chips)
// labels the network so the testnet (Mantle Sepolia) nature is honest to the
// user. Theme-token styling only — warn tones with dark-safe opacity, never
// hard white/gray borders.
//
// On a MAINNET build (`VITE_MARKETPLACE_NETWORK=mainnet`) these render nothing /
// an accurate real-funds notice: a "Testnet · purchases are simulated" claim on
// mainnet would be false and unsafe.

import { isMainnetNetwork } from "../lib/chain";

interface TestnetBadgeProps {
  /** "xs" for inline pills next to CTAs/prices, "sm" for standalone rows. */
  size?: "xs" | "sm";
  className?: string;
}

export function TestnetBadge({ size = "xs", className = "" }: TestnetBadgeProps) {
  // No testnet to flag on a mainnet build — never mislabel a real-funds surface.
  if (isMainnetNetwork()) return null;
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
  // Mainnet build: surface an accurate real-funds notice instead of the
  // (false) "simulated testnet" copy. Kept full-width/single-row per the shell
  // layout + no-popup rules.
  if (isMainnetNetwork()) {
    return (
      <div
        className={[
          "flex items-center gap-2 rounded-md border border-border bg-surface-elev/40",
          "px-3 py-2 text-[12px] text-text-2",
          className,
        ].join(" ")}
      >
        <span className="inline-flex items-center rounded-[3px] border border-gold/40 text-gold uppercase font-mono tracking-[0.06em] whitespace-nowrap px-1.5 py-0.5 text-[10px]">
          Mainnet
        </span>
        <span>
          The marketplace runs on{" "}
          <span className="text-text">Mantle mainnet</span>. Purchases move real
          USDC — listing and buying spend live funds.
        </span>
      </div>
    );
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
