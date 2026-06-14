// src/features/marketplace/routes/InstallSteps.tsx
// Inline install stepper — no modals; all states render inline.
// "Add to strategies" runs the license-gated import against the local engine.
// The bundle step links the pinned IPFS manifest for OPEN-tier listings; for
// SEALED-tier listings (detected by the bundle route returning encrypted:true)
// the bundle is undecryptable IPFS ciphertext, so we revive a "Decrypt & import
// sealed bundle" step that drives the Lit-gated decrypt + import-sealed flow.
import { useEffect, useState } from "react";
import { Link } from "react-router-dom";
import { ApiError, apiFetch } from "@/api/client";
import {
  currentAddress,
  getPublicGateway,
  DEFAULT_PUBLIC_GATEWAY,
} from "../lib/chain";
import {
  fetchBundle,
  importSealedListing,
} from "@/features/marketplace/data/ApiMarketplaceData";
import {
  SealedGateError,
  SealedNotConfiguredError,
} from "../lib/sealed";
import { WalletRequiredError } from "../lib/purchaseErrors";
import {
  requirementsFromManifest,
  useBundleManifest,
} from "../data/bundle";
import { RequirementChip } from "../components/RequirementChip";
import type { Receipt, Ingredient } from "@/features/marketplace/data/types";

// ── visual step states ───────────────────────────────────────────────────────
type StepState = "done" | "active" | "pending";

function StepCircle({ n, state }: { n: number; state: StepState }) {
  const isDone   = state === "done";
  const isActive = state === "active";
  return (
    <div
      className={[
        "w-[26px] h-[26px] rounded-full border-[1.5px] flex items-center justify-center shrink-0",
        isDone   ? "border-gold bg-gold" : "",
        isActive ? "border-gold bg-gold/10" : "",
        state === "pending" ? "border-border-strong bg-transparent" : "",
      ].filter(Boolean).join(" ")}
    >
      {isDone ? (
        <svg
          width="13" height="13" viewBox="0 0 13 13" fill="none"
          stroke="#001A0A" strokeWidth="2.4" strokeLinecap="round" strokeLinejoin="round"
          aria-hidden="true"
        >
          <path d="M2 7l3 3 6-7" />
        </svg>
      ) : (
        <span
          className={[
            "font-mono text-[12px] font-semibold",
            isActive ? "text-gold" : "text-text-3",
          ].join(" ")}
        >
          {n}
        </span>
      )}
    </div>
  );
}

function Step({
  n,
  state,
  title,
  description,
  action,
  last = false,
}: {
  n: number;
  state: StepState;
  title: string;
  description: React.ReactNode;
  action?: React.ReactNode;
  last?: boolean;
}) {
  return (
    <div
      className={[
        "grid gap-3 px-4 py-3.5",
        !last ? "border-b border-border-soft" : "",
      ].join(" ")}
      style={{ gridTemplateColumns: "38px 1fr auto" }}
    >
      <StepCircle n={n} state={state} />
      <div className="min-w-0">
        <div
          className={[
            "text-[13.5px] font-semibold leading-tight",
            state === "done" ? "text-text-3 line-through" : "text-text",
          ].join(" ")}
        >
          {title}
        </div>
        <div className="mt-1.5 text-[12px] text-text-2 leading-snug">{description}</div>
      </div>
      {action ? (
        <div className="flex items-start pt-0.5">{action}</div>
      ) : (
        <div />
      )}
    </div>
  );
}

function chipClass(variant: "primary" | "chip" | "ghost"): string {
  const base = "font-mono text-[11.5px] px-2.5 py-1 rounded cursor-pointer flex items-center gap-1";
  const styles: Record<string, string> = {
    primary: `${base} bg-gold text-black font-semibold motion-safe:active:scale-[0.96]`,
    chip:    `${base} border border-border-strong text-text-2 hover:text-text`,
    ghost:   `${base} text-text-3 hover:text-text`,
  };
  return styles[variant];
}

function ChipBtn({
  children,
  variant = "chip",
  onClick,
  disabled,
}: {
  children: React.ReactNode;
  variant?: "primary" | "chip" | "ghost";
  onClick?: () => void;
  disabled?: boolean;
}) {
  return (
    <button
      className={[chipClass(variant), disabled ? "opacity-60 cursor-default" : ""].filter(Boolean).join(" ")}
      onClick={onClick}
      disabled={disabled}
    >
      {children}
    </button>
  );
}

function IngredientChip({ ingredient }: { ingredient: Ingredient }) {
  const { name, kind, installed } = ingredient;
  return (
    <span
      data-testid="ingredient-chip"
      data-installed={String(installed)}
      className={[
        "inline-flex items-center gap-1 px-2 py-0.5 rounded-sm border font-mono text-[10.5px]",
        installed
          ? "border-gold-soft bg-gold/10 text-gold"
          : "border-warn/60 bg-warn/[0.08] text-warn",
      ].join(" ")}
    >
      {/* check / plus icon */}
      {installed ? (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M2 7l3 3 5-6" />
        </svg>
      ) : (
        <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
          <path d="M6 2v8M2 6h8" />
        </svg>
      )}
      {name}
      <span className="text-[9px] tracking-[0.14em] text-text-4 uppercase">{kind}</span>
    </span>
  );
}

// ── import flow state (Add to strategies) ────────────────────────────────────
type ImportState =
  | { phase: "idle" }
  | { phase: "pending" }
  | { phase: "done"; agentId: string }
  | { phase: "error"; message: string };

// ── component ────────────────────────────────────────────────────────────────
/** Map a sealed import/decrypt failure to inline operator copy. */
function sealedErrorMessage(e: unknown): string {
  if (e instanceof ApiError && e.status === 403) return "No license held for this wallet.";
  if (e instanceof ApiError && e.status === 409)
    return "Bundle integrity check failed (content hash mismatch).";
  if (e instanceof WalletRequiredError) return "Connect a wallet to decrypt this sealed bundle.";
  if (e instanceof SealedNotConfiguredError) return e.message;
  if (e instanceof SealedGateError) return `Decryption failed: ${e.message}`;
  return e instanceof Error ? e.message : String(e);
}

export function InstallSteps({ receipt }: { receipt: Receipt }) {
  const { install } = receipt;
  const missingCount = install.ingredients.filter((i) => !i.installed).length;
  const [importState, setImportState] = useState<ImportState>({ phase: "idle" });
  // Sealed-tier detection: the bundle route reports encrypted:true for sealed
  // listings. null = still loading / unknown; false = open tier.
  const [sealed, setSealed] = useState<boolean | null>(null);
  // Config-driven public IPFS read gateway base (no trailing slash), from the
  // status route; the neutral default until it loads.
  const [publicGateway, setPublicGateway] = useState<string>(
    DEFAULT_PUBLIC_GATEWAY,
  );

  useEffect(() => {
    let live = true;
    getPublicGateway()
      .then((g) => {
        if (live) setPublicGateway(g);
      })
      .catch(() => {
        // Keep the neutral default.
      });
    return () => {
      live = false;
    };
  }, []);

  useEffect(() => {
    let live = true;
    fetchBundle(receipt.listing.id)
      .then((b) => {
        if (live) setSealed(b.encrypted === true);
      })
      .catch(() => {
        // Bundle route unreachable / unindexed — fall back to the open-tier
        // stepper (the plain import route still resolves server-side).
        if (live) setSealed(false);
      });
    return () => {
      live = false;
    };
  }, [receipt.listing.id]);

  // Real receipts carry no local ingredient detection (install.ingredients
  // is an honest empty) — derive the requirement list from the listing's
  // verified manifest instead. Installed state is unknown, so these render
  // as neutral "required" chips, never installed/missing badges.
  const manifest = useBundleManifest(receipt.listing.id);
  const requirements =
    install.ingredients.length === 0 ? requirementsFromManifest(manifest) : [];

  // Only IPFS-pinned (open tier) bundles get a step; receipts without a CID
  // (local xvn:// listings, unindexed listings) skip it — the import route
  // still resolves the manifest server-side.
  const bundleCid = receipt.license.bundleCid;
  let n = 0;
  const next = () => ++n;

  async function runSealedImport() {
    setImportState({ phase: "pending" });
    try {
      const out = await importSealedListing(receipt.listing.id);
      setImportState({ phase: "done", agentId: out.agent_id });
    } catch (e) {
      setImportState({ phase: "error", message: sealedErrorMessage(e) });
    }
  }

  async function runImport() {
    setImportState({ phase: "pending" });
    const address = await currentAddress();
    if (!address) {
      setImportState({ phase: "error", message: "Connect wallet first." });
      return;
    }
    try {
      const out = await apiFetch<{ agent_id: string }>(
        `/api/marketplace/listings/${receipt.listing.id}/import`,
        { method: "POST", body: JSON.stringify({ address }) },
      );
      setImportState({ phase: "done", agentId: out.agent_id });
    } catch (e) {
      const message =
        e instanceof ApiError && e.status === 403
          ? "No license held for this wallet."
          : e instanceof Error
            ? e.message
            : String(e);
      setImportState({ phase: "error", message });
    }
  }

  return (
    <div className="py-1">
      {/* Bundle step — only for OPEN-tier listings with an IPFS-pinned
          manifest. Sealed listings carry encrypted ciphertext (not a
          human-openable manifest), so the decrypt step below replaces it. */}
      {sealed !== true && bundleCid !== "" && (
        <Step
          n={next()}
          state="active"
          title="Fetch strategy bundle"
          description={
            <>
              Manifest pinned to IPFS as{" "}
              <span className="font-mono text-text-2">{bundleCid}</span>.
            </>
          }
          action={
            <a
              className={chipClass("chip")}
              href={`${publicGateway}/ipfs/${bundleCid}`}
              target="_blank"
              rel="noreferrer"
            >
              Open bundle
              <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
                <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </a>
          }
        />
      )}

      {/* Install ingredients — local detection when the receipt carries it
          (fixtures); otherwise the manifest's requirement list with no
          installed/missing claim (real receipts). */}
      {requirements.length > 0 ? (
        <Step
          n={next()}
          state="pending"
          title="Check the strategy's requirements"
          description={
            <div data-testid="receipt-requirements">
              <span className="text-text-2">
                Required to run this strategy — installed state unknown.
              </span>
              <div className="flex flex-wrap gap-1.5 mt-2">
                {requirements.map((r) => (
                  <RequirementChip key={`${r.kind}:${r.name}`} requirement={r} />
                ))}
              </div>
            </div>
          }
        />
      ) : (
        <Step
          n={next()}
          state="pending"
          title="Install missing ingredients"
          description={
            <div>
              <span className="text-text-2">
                {install.ingredients.filter((i) => i.installed).length} of{" "}
                {install.ingredients.length} already installed in your XVN.
              </span>
              <div className="flex flex-wrap gap-1.5 mt-2">
                {install.ingredients.map((ing) => (
                  <IngredientChip key={ing.name} ingredient={ing} />
                ))}
              </div>
            </div>
          }
          action={
            missingCount > 0 ? (
              <ChipBtn variant="chip">
                {/* plus icon */}
                <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
                  <path d="M6 2v8M2 6h8" />
                </svg>
                Install missing ({missingCount})
              </ChipBtn>
            ) : undefined
          }
        />
      )}

      {/* Final step — license-gated import into the local engine. Sealed
          listings decrypt the bundle through the Lit gate first; open
          listings import the server-resolved manifest directly. */}
      <Step
        n={next()}
        state={importState.phase === "done" ? "done" : "pending"}
        title={
          sealed === true
            ? "Decrypt & import sealed bundle"
            : "Add to your Strategies and run backtest first"
        }
        description={
          <>
            {sealed === true ? (
              <>
                Decrypts the sealed manifest with your wallet (license-gated),
                then lands in{" "}
                <span className="font-mono text-text-2">
                  Strategies / Marketplace · {receipt.listing.id}
                </span>
                .
              </>
            ) : (
              <>
                Lands in{" "}
                <span className="font-mono text-text-2">
                  Strategies / Marketplace · {receipt.listing.id}
                </span>
                . Recommended: 7-day backtest with 2% risk cap before going live.
              </>
            )}
            {importState.phase === "error" && (
              <div data-testid="import-error" className="mt-1.5 text-warn">
                {importState.message}
              </div>
            )}
          </>
        }
        action={
          importState.phase === "done" ? (
            <Link
              className={chipClass("primary")}
              to={`/authoring/${importState.agentId}`}
            >
              Open in strategies
            </Link>
          ) : importState.phase === "pending" ? (
            <ChipBtn variant="chip" disabled>
              {sealed === true ? "Decrypting…" : "Importing…"}
            </ChipBtn>
          ) : sealed === true ? (
            <ChipBtn variant="chip" onClick={() => void runSealedImport()}>
              Decrypt &amp; import
            </ChipBtn>
          ) : (
            <ChipBtn variant="chip" onClick={() => void runImport()}>
              Add to strategies
            </ChipBtn>
          )
        }
        last
      />
    </div>
  );
}
