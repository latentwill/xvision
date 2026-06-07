import { useState } from "react";
import { Icon } from "@/components/primitives/Icon";
import { useWallet } from "@/features/marketplace/lib/wallet";

function shortAddr(addr: string): string {
  return `${addr.slice(0, 6)}…${addr.slice(-4)}`;
}

export function WalletConnectFooter() {
  const { address, connecting, connect, disconnect } = useWallet();
  const [error, setError] = useState<string | null>(null);

  async function handleConnect() {
    setError(null);
    try {
      await connect();
    } catch (e) {
      setError(e instanceof Error ? e.message : "Connection failed");
    }
  }

  if (address) {
    return (
      <div className="flex items-center gap-2.5 px-4 py-3.5 border-t border-border-soft">
        <div className="w-8 h-8 rounded-full bg-gold/[0.10] border border-gold/30 flex items-center justify-center shrink-0">
          <Icon name="diamond" size={12} className="text-gold" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text leading-tight font-mono truncate">
            {shortAddr(address)}
          </div>
          <div className="text-[11px] text-text-3 leading-tight">Ethereum</div>
        </div>
        <button
          type="button"
          onClick={disconnect}
          title="Disconnect wallet"
          className="text-[15px] leading-none text-text-4 hover:text-text transition-colors"
        >
          ×
        </button>
      </div>
    );
  }

  return (
    <div className="border-t border-border-soft">
      <button
        type="button"
        onClick={handleConnect}
        disabled={connecting}
        className="flex w-full items-center gap-2.5 px-4 py-3.5 text-left disabled:opacity-60"
      >
        <div className="w-8 h-8 rounded-full bg-surface-panel border border-dashed border-border-soft flex items-center justify-center shrink-0">
          <Icon name="diamond" size={12} className="text-text-4" />
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text-2 leading-tight">
            {connecting ? "Connecting…" : "Connect Wallet"}
          </div>
          {error ? (
            <div className="text-[10px] text-red-400 leading-tight truncate">
              {error}
            </div>
          ) : (
            <div className="text-[11px] text-text-4 leading-tight">
              MetaMask / EVM
            </div>
          )}
        </div>
        {!connecting && (
          <Icon name="chevR" size={14} className="text-text-4" />
        )}
      </button>
    </div>
  );
}
