// src/features/marketplace/routes/sell/Step2Configure.tsx
import type { PublishDraft, Tier } from "@/features/marketplace/data/types";

export function Step2Configure({
  draft,
  onUpdate,
  onNext,
  onBack,
}: {
  draft: PublishDraft;
  onUpdate: (patch: Partial<Pick<PublishDraft, "name" | "tier" | "priceUsdc" | "acceptedPayers">>) => void;
  onNext: () => void;
  onBack?: () => void;
}) {
  const allPass = draft.listable.every((c) => c.ok);
  const nameEmpty = draft.name.trim().length === 0;

  function setTier(t: Tier) {
    onUpdate({ tier: t, priceUsdc: t === "open" ? null : (draft.priceUsdc ?? 49) });
  }

  return (
    <div data-testid="sell-step-2-body" className="flex flex-col gap-6">

      {/* Listing name — defaults to the strategy's display name; the seller can
          rename the listing before minting so it never lists as "Strategy #N". */}
      <div>
        <label
          htmlFor="listing-name-input"
          className="block text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2"
        >
          Listing name
        </label>
        <input
          id="listing-name-input"
          data-testid="listing-name-input"
          type="text"
          value={draft.name}
          maxLength={80}
          onChange={(e) => onUpdate({ name: e.target.value })}
          placeholder="Name shown on the marketplace"
          className="w-full px-3 py-2 bg-surface-elev border border-border rounded-md text-[13px] text-text focus:border-gold/60 focus:outline-none"
        />
        <p className="mt-1 text-[11px] text-text-3">
          Defaults to your strategy’s name. Buyers see this on the listing.
        </p>
      </div>

      {/* Listability checks */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Listability checks
        </p>
        <ul className="flex flex-col gap-1">
          {draft.listable.map((check, i) => (
            <li key={i} className="flex items-start gap-2 text-[13px]">
              {check.ok ? (
                <span className="text-gold mt-0.5" aria-label="pass">✓</span>
              ) : (
                <span className="text-danger mt-0.5" aria-label="fail">✗</span>
              )}
              <span className={check.ok ? "text-text-2" : "text-text"}>
                {check.label}
                {!check.ok && check.reason && (
                  <span className="ml-1 text-danger text-[11px]">— {check.reason}</span>
                )}
              </span>
            </li>
          ))}
        </ul>
      </div>

      {/* Tier selector */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">Tier</p>
        <div className="flex gap-2">
          <button
            data-testid="tier-open-btn"
            onClick={() => setTier("open")}
            className={`px-3 py-2 rounded-md border text-[13px] ${
              draft.tier === "open"
                ? "border-gold/60 bg-gold/10 text-gold"
                : "border-border text-text-2 hover:border-border-strong"
            }`}
          >
            <span className="font-medium">Tier A</span>
            <span className="ml-1 text-text-3">· open / free</span>
          </button>
          <button
            data-testid="tier-sealed-btn"
            onClick={() => setTier("sealed")}
            className={`px-3 py-2 rounded-md border text-[13px] ${
              draft.tier === "sealed"
                ? "border-gold/60 bg-gold/10 text-gold"
                : "border-border text-text-2 hover:border-border-strong"
            }`}
          >
            <span className="font-medium">Tier B</span>
            <span className="ml-1 text-text-3">· sealed / paid</span>
          </button>
        </div>
      </div>

      {/* Price (Tier B only) */}
      {draft.tier === "sealed" && (
        <div>
          <label
            htmlFor="price-input"
            className="block text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2"
          >
            Price (USDC)
          </label>
          <input
            id="price-input"
            data-testid="price-input"
            type="number"
            min={1}
            step={1}
            value={draft.priceUsdc ?? 49}
            onChange={(e) => onUpdate({ priceUsdc: Math.max(1, Number(e.target.value)) })}
            className="w-28 px-3 py-2 bg-surface-elev border border-border rounded-md text-[13px] font-mono text-text focus:border-gold/60 focus:outline-none"
          />
        </div>
      )}

      {/* Accepted payers */}
      <div>
        <p className="text-[11px] font-mono uppercase tracking-wide text-text-3 mb-2">
          Accepted payers
        </p>
        <div className="flex gap-4">
          <label className="flex items-center gap-2 text-[13px] cursor-pointer">
            <input
              type="checkbox"
              data-testid="payer-humans"
              checked={draft.acceptedPayers.humans}
              onChange={(e) =>
                onUpdate({ acceptedPayers: { ...draft.acceptedPayers, humans: e.target.checked } })
              }
              className="accent-gold"
            />
            Humans
          </label>
          <label className="flex items-center gap-2 text-[13px] cursor-pointer">
            <input
              type="checkbox"
              data-testid="payer-agents"
              checked={draft.acceptedPayers.agents}
              onChange={(e) =>
                onUpdate({ acceptedPayers: { ...draft.acceptedPayers, agents: e.target.checked } })
              }
              className="accent-gold"
            />
            Agents (x402)
          </label>
        </div>
      </div>

      {/* Continue */}
      <div className="flex flex-wrap items-center gap-4">
        <button
          onClick={onNext}
          disabled={!allPass || nameEmpty}
          className={`px-4 py-2 rounded-md text-[13px] font-medium motion-safe:active:scale-[0.96] ${
            allPass && !nameEmpty
              ? "bg-gold text-black hover:bg-gold/90"
              : "bg-surface-elev border border-border text-text-3 cursor-not-allowed"
          }`}
        >
          Continue
        </button>
        <button
          type="button"
          onClick={onBack}
          className="text-[12px] text-text-3 hover:text-text-2"
        >
          ← Back
        </button>
        {!allPass && (
          <p className="text-[12px] text-danger">
            Resolve listability failures before continuing.
          </p>
        )}
        {allPass && nameEmpty && (
          <p className="text-[12px] text-danger">Give your listing a name before continuing.</p>
        )}
      </div>
    </div>
  );
}
