// Live Trading wallet banner (spec §2.5).
//
// Shown ONLY when the user's wallet is not connected
// (`useWallet().address === null`). Full-width strip below the strategy
// strip and above the viewport. It does NOT hide strategy data — only
// trading actions are disabled (the strip's transport placeholders render
// disabled with a "Connect wallet to act" tooltip; that gating lives in
// the live page/TransportControls, not here).
//
// This is a SEPARATE component from `SafetyPauseBanner` — do not reuse it.
// No popup: connecting routes to the wallet settings page.

import { Link } from "react-router-dom";

export function WalletBanner() {
  return (
    <div
      data-testid="wallet-banner"
      role="status"
      className="mb-4 flex flex-wrap items-center justify-between gap-3 rounded-lg border border-warn/30 bg-warn/10 px-4 py-2.5 text-[13px]"
    >
      <span className="text-text-2">
        Wallet not connected — trading actions disabled.
      </span>
      <Link
        to="/settings/wallet"
        className="shrink-0 rounded-sm border border-warn/40 px-2.5 py-1 text-[12.5px] font-medium text-warn transition-colors hover:bg-warn/15"
      >
        Connect wallet
      </Link>
    </div>
  );
}
