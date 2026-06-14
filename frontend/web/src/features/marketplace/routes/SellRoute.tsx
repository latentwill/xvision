// src/features/marketplace/routes/SellRoute.tsx
import { useState, useCallback } from "react";
import { Link, useNavigate } from "react-router-dom";
import { useMarketplaceData } from "@/features/marketplace/data/provider";
import { ApiError } from "@/api/client";
import { patchStrategyMetadata } from "@/api/strategies";
import type { ListableStrategy, PublishDraft } from "@/features/marketplace/data/types";
import { Step1PickStrategy } from "./sell/Step1PickStrategy";
import { Step2Configure } from "./sell/Step2Configure";
import { Step3Preview, type PublicDescription } from "./sell/Step3Preview";

type Step = 1 | 2 | 3;

export function SellRoute() {
  const mp = useMarketplaceData();
  const navigate = useNavigate();

  const [step, setStep] = useState<Step>(1);
  const [draft, setDraft] = useState<PublishDraft | null>(null);
  const [loadingDraft, setLoadingDraft] = useState(false);
  const [draftError, setDraftError] = useState<string | null>(null);
  const [minting, setMinting] = useState(false);
  const [mintError, setMintError] = useState<string | null>(null);

  const handleStrategySelect = useCallback(
    async (strategy: ListableStrategy) => {
      setLoadingDraft(true);
      setDraftError(null);
      try {
        const d = await mp.createPublishDraft(strategy.id);
        setDraft(d);
        setStep(2);
      } catch (err) {
        setDraftError(
          err instanceof ApiError && err.status === 404
            ? `"${strategy.name}" is no longer in your XVN — refresh and try again.`
            : "Couldn't load that strategy — try again.",
        );
      } finally {
        setLoadingDraft(false);
      }
    },
    [mp],
  );

  const handleDraftUpdate = useCallback(
    (patch: Partial<Pick<PublishDraft, "name" | "tier" | "priceUsdc" | "acceptedPayers">>) => {
      setDraft((prev) => (prev ? { ...prev, ...patch } : prev));
    },
    [],
  );

  const handleMint = useCallback(async (description: PublicDescription) => {
    if (!draft) return;
    setMinting(true);
    setMintError(null);
    // The public description must land on the stored strategy BEFORE the
    // listing is submitted — the manifest hash is computed server-side from
    // the stored strategy. A failed save aborts the publish.
    if (description.dirty) {
      try {
        await patchStrategyMetadata(draft.strategyId, {
          plain_summary: description.value,
        });
      } catch (err) {
        setMintError(
          err instanceof ApiError && err.status === 404
            ? "Couldn't save the description — this strategy isn't in your XVN anymore. Refresh your strategy list and try again."
            : err instanceof Error
              ? `Saving the public description failed — ${err.message}`
              : "Saving the public description failed — try again.",
        );
        setMinting(false);
        return;
      }
    }
    try {
      const tx = await mp.submitListing(draft);
      // Phase-2 wart: txHash carries listing_id (not a real tx hash) until the
      // confirmation path is wired (see publish.ts — TxRef.txHash = out.listing_id).
      navigate(`/marketplace/lineage/${tx.txHash}`);
    } catch (err) {
      const msg =
        err instanceof ApiError
          ? err.message
          : "Publish failed — try again.";
      setMintError(msg);
    } finally {
      setMinting(false);
    }
  }, [draft, mp, navigate]);

  return (
    <div className="px-7 py-8 max-w-2xl" data-page="sell">
      <div className="mb-4 text-[13px]">
        <Link to="/marketplace" className="text-text-3 hover:text-text hover:underline underline-offset-2">
          ← Back to Marketplace
        </Link>
      </div>
      <h1 className="text-[20px] font-sans font-semibold tracking-tight mb-1">
        List your strategy
      </h1>
      <p className="text-[13px] text-text-2 mb-8">
        List a strategy from your XVN to the marketplace. Three steps.
      </p>

      <section
        data-sell-step="1"
        className={`mb-4 rounded-md border border-border ${step === 1 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={1} active={step === 1} done={step > 1} />
          <span className="text-[13px] font-medium">Pick a strategy</span>
          {step > 1 && draft && (
            <button
              className="ml-auto text-[11px] text-text-3 hover:text-text-2"
              onClick={() => {
                setStep(1);
                setDraft(null);
              }}
            >
              Change
            </button>
          )}
        </div>
        {step === 1 && (
          <div className="px-5 pb-5">
            {loadingDraft ? (
              <p className="text-[13px] text-text-3">Loading draft…</p>
            ) : (
              <Step1PickStrategy onSelect={handleStrategySelect} />
            )}
            {draftError && (
              <p className="mt-3 text-[12px] text-danger">{draftError}</p>
            )}
          </div>
        )}
      </section>

      <section
        data-sell-step="2"
        className={`mb-4 rounded-md border border-border ${step === 2 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={2} active={step === 2} done={step > 2} />
          <span className={`text-[13px] font-medium ${step < 2 ? "text-text-3" : ""}`}>
            Configure listing
          </span>
          {step > 2 && draft && (
            <button
              className="ml-auto text-[11px] text-text-3 hover:text-text-2"
              onClick={() => setStep(2)}
            >
              Change
            </button>
          )}
        </div>
        {step === 2 && draft && (
          <div className="px-5 pb-5">
            <Step2Configure
              draft={draft}
              onUpdate={handleDraftUpdate}
              onNext={() => setStep(3)}
            />
          </div>
        )}
      </section>

      <section
        data-sell-step="3"
        className={`rounded-md border border-border ${step === 3 ? "border-gold/40" : ""}`}
      >
        <div className="px-5 py-3 flex items-center gap-3">
          <StepIndicator n={3} active={step === 3} done={false} />
          <span className={`text-[13px] font-medium ${step < 3 ? "text-text-3" : ""}`}>
            Preview &amp; mint
          </span>
        </div>
        {step === 3 && draft && (
          <div className="px-5 pb-5">
            {mintError && (
              <div
                data-testid="mint-error"
                className="mb-4 rounded-[5px] border border-danger/40 bg-danger/10 px-4 py-3 font-mono text-[12px] text-danger"
              >
                {mintError}
              </div>
            )}
            <Step3Preview draft={draft} onMint={handleMint} minting={minting} />
          </div>
        )}
      </section>
    </div>
  );
}

function StepIndicator({
  n,
  active,
  done,
}: {
  n: number;
  active: boolean;
  done: boolean;
}) {
  if (done) {
    return (
      <span className="w-5 h-5 rounded-full bg-gold/20 border border-gold/40 flex items-center justify-center text-gold text-[10px]">
        ✓
      </span>
    );
  }
  return (
    <span
      className={`w-5 h-5 rounded-full border flex items-center justify-center text-[10px] font-mono ${
        active ? "border-gold/60 text-gold" : "border-border text-text-3"
      }`}
    >
      {n}
    </span>
  );
}
