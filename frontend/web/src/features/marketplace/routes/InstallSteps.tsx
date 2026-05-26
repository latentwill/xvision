// src/features/marketplace/routes/InstallSteps.tsx
// Inline install stepper — no modals, no external navigation.
// All step actions are anchors/buttons that remain within the page.
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

function ChipBtn({ children, variant = "chip" }: { children: React.ReactNode; variant?: "primary" | "chip" | "ghost" }) {
  const base = "font-mono text-[11.5px] px-2.5 py-1 rounded cursor-pointer flex items-center gap-1";
  const styles: Record<string, string> = {
    primary: `${base} bg-gold text-black font-semibold`,
    chip:    `${base} border border-border-strong text-text-2 hover:text-text`,
    ghost:   `${base} text-text-3 hover:text-text`,
  };
  return <button className={styles[variant]}>{children}</button>;
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

// ── component ────────────────────────────────────────────────────────────────
export function InstallSteps({ receipt }: { receipt: Receipt }) {
  const { install } = receipt;
  const missingCount = install.ingredients.filter((i) => !i.installed).length;

  return (
    <div className="py-1">
      {/* Step 1 — XVN detected */}
      <Step
        n={1}
        state={install.xvnDetected ? "done" : "active"}
        title="XVN install detected"
        description={
          install.xvnDetected ? (
            <>
              Connected to your XVN at{" "}
              <span className="font-mono text-gold">{install.xvnEndpoint}</span>
            </>
          ) : (
            <span className="text-warn">
              XVN not detected — install XVN locally and reopen this receipt.
            </span>
          )
        }
      />

      {/* Step 2 — Decrypt sealed bundle */}
      <Step
        n={2}
        state="active"
        title="Decrypt sealed bundle"
        description={
          <>
            Sealed bundle from IPFS — your license token authorises decryption.{" "}
            About to fetch{" "}
            <span className="font-mono text-text-2">{receipt.license.bundleCid}</span>.
          </>
        }
        action={
          <ChipBtn variant="primary">
            Decrypt now
          </ChipBtn>
        }
      />

      {/* Step 3 — Install missing ingredients */}
      <Step
        n={3}
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

      {/* Step 4 — Add to Strategies */}
      <Step
        n={4}
        state="pending"
        title="Add to your Strategies and run backtest first"
        description={
          <>
            Lands in{" "}
            <span className="font-mono text-text-2">
              Strategies / Marketplace · {receipt.listing.id}
            </span>
            . Recommended: 7-day backtest with 2% risk cap before going live.
          </>
        }
        action={
          <div className="flex gap-1.5">
            <ChipBtn variant="chip">Add to strategies</ChipBtn>
            <ChipBtn variant="ghost">
              Open in XVN
              <svg width="9" height="9" viewBox="0 0 12 12" fill="none" stroke="currentColor" strokeWidth="1.4" aria-hidden="true">
                <path d="M4 2h6v6M10 2L4.5 7.5" strokeLinecap="round" strokeLinejoin="round" />
              </svg>
            </ChipBtn>
          </div>
        }
        last
      />
    </div>
  );
}
