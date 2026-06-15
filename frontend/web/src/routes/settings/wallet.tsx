import { useState } from "react";
import { Card } from "@/components/primitives/Card";
import { useWallet } from "@/features/marketplace/lib/wallet";
import { isMainnetNetwork } from "@/features/marketplace/lib/chain";

function truncateAddress(addr: string): string {
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

export function SettingsWalletRoute() {
  const { address, connecting, connect, disconnect } = useWallet();
  const [error, setError] = useState<string | null>(null);

  async function handleConnect() {
    setError(null);
    try {
      await connect();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Failed to connect");
    }
  }

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Wallet
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Required for buying strategies. Browse and evaluate strategies
            without a wallet.
          </p>
        </div>

        {address ? (
          <div className="flex items-center gap-3 flex-wrap">
            <code className="font-mono text-[13px] text-text">
              {truncateAddress(address)}
            </code>
            <span className="px-2 py-0.5 rounded border border-border-strong font-mono text-[11px] text-text-3">
              {isMainnetNetwork() ? "Mantle mainnet" : "Testnet (Mantle Sepolia)"}
            </span>
            <button
              type="button"
              onClick={disconnect}
              className="px-3 py-1.5 rounded text-[12px] border border-border text-text-2 hover:text-danger hover:border-danger/50 transition-colors"
            >
              Disconnect
            </button>
          </div>
        ) : (
          <div>
            <button
              type="button"
              onClick={handleConnect}
              disabled={connecting}
              className="px-3 py-2 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {connecting ? "Connecting…" : "Connect wallet"}
            </button>
            {error ? (
              <p className="m-0 mt-2 text-[12px] text-danger font-mono">
                {error}
              </p>
            ) : null}
          </div>
        )}
      </Card>
    </div>
  );
}
