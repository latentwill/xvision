// NetworkMismatchBanner — ops guard for the build/runtime network split.
//
// The bundle is built for one network (VITE_MARKETPLACE_NETWORK) while the
// backend is configured for a chain at runtime (XVN_CHAIN_ID). The signing path
// resolves the backend's chain at runtime so buys still work even on a mismatch,
// but a mismatch is almost always a misconfigured deploy — so surface it loudly
// instead of letting it pass silently. Inline, full-width, single row (no popup,
// per the shell layout rules); renders only on a real mismatch.
import { useEffect, useState } from "react";
import { activeChain, getBackendChainId } from "../lib/chain";

export function NetworkMismatchBanner() {
  const [mismatch, setMismatch] = useState<{
    build: number;
    backend: number;
  } | null>(null);

  useEffect(() => {
    let cancelled = false;
    const build = activeChain.id;
    // Compare against the RAW backend chain id so a mismatch surfaces even when
    // the backend chain is one the SPA can't resolve (still a misconfig).
    void getBackendChainId().then((backend) => {
      if (cancelled) return;
      if (backend != null && backend !== build) {
        // Loud console signal for operators tailing logs.
        console.warn(
          `[marketplace] build/runtime network mismatch: bundle built for ` +
            `chain ${build} but the backend reports chain ${backend}. ` +
            `The backend chain wins for signing; rebuild with the matching ` +
            `VITE_MARKETPLACE_NETWORK to clear this.`,
        );
        setMismatch({ build, backend });
      }
    });
    return () => {
      cancelled = true;
    };
  }, []);

  if (!mismatch) return null;
  return (
    <div
      data-testid="network-mismatch-banner"
      className="flex items-center gap-2 rounded-md border border-danger/40 bg-danger/[0.08] px-3 py-2 text-[12px] text-text"
    >
      <span className="inline-flex items-center rounded-[3px] border border-danger/50 text-danger uppercase font-mono tracking-[0.06em] whitespace-nowrap px-1.5 py-0.5 text-[10px]">
        Network mismatch
      </span>
      <span>
        This build targets chain{" "}
        <span className="font-mono">{mismatch.build}</span> but the server is on
        chain <span className="font-mono">{mismatch.backend}</span>. Buying uses
        the server&apos;s chain — rebuild with the matching{" "}
        <span className="font-mono">VITE_MARKETPLACE_NETWORK</span> (or fix{" "}
        <span className="font-mono">XVN_CHAIN_ID</span>) to clear this.
      </span>
    </div>
  );
}
