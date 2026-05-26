// src/features/marketplace/routes/IngredientBanner.tsx
// Full-width warn-tinted banner below the hero.
// Shows when any ingredient is not installed. Each pill is green-check (installed)
// or amber-plus (missing). No popups — just an inline banner.
import type { Ingredient } from "@/features/marketplace/data/types";

interface Props {
  ingredients: Ingredient[];
}

export function IngredientBanner({ ingredients }: Props) {
  const missing = ingredients.filter((i) => !i.installed);
  if (missing.length === 0) return null;

  return (
    <div
      data-testid="ingredient-banner"
      className="flex items-center gap-3 px-7 py-3.5 border-b border-border bg-warn/[0.04]"
    >
      {/* Warn circle icon */}
      <div className="flex-shrink-0 w-7 h-7 rounded-full flex items-center justify-center bg-warn/[0.12] border border-warn">
        <svg width="14" height="14" viewBox="0 0 14 14" fill="none" className="text-warn">
          <circle cx="7" cy="7" r="6" stroke="currentColor" strokeWidth="1.5" />
          <line x1="7" y1="4" x2="7" y2="7.5" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
          <circle cx="7" cy="10" r="0.75" fill="currentColor" />
        </svg>
      </div>

      <div className="flex-1 min-w-0">
        <p className="text-[13.5px] text-foreground">
          <strong>
            Ingredient check · {missing.length} of {ingredients.length} installed in your XVN.
          </strong>{" "}
          Install the missing {missing.length === 1 ? "one" : `${missing.length}`} before purchase.
        </p>
        <div className="flex flex-wrap gap-2 mt-1.5">
          {ingredients.map((ing) => (
            <span
              key={ing.name}
              className={[
                "inline-flex items-center gap-1.5 px-2 py-0.5 rounded-[3px] border font-mono text-[11px]",
                ing.installed
                  ? "border-gold-soft bg-gold/[0.10] text-gold"
                  : "border-warn bg-warn/[0.08] text-warn",
              ].join(" ")}
            >
              {ing.installed ? (
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                  <path
                    d="M2 5l2 2 4-4"
                    stroke="currentColor"
                    strokeWidth="1.5"
                    strokeLinecap="round"
                    strokeLinejoin="round"
                  />
                </svg>
              ) : (
                <svg width="10" height="10" viewBox="0 0 10 10" fill="none">
                  <path d="M5 2v6M2 5h6" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" />
                </svg>
              )}
              {ing.name}
              <span className="font-mono text-[9px] tracking-[0.14em] opacity-60 uppercase">
                {ing.kind.toUpperCase()}
              </span>
            </span>
          ))}
        </div>
      </div>

      {/* TODO(install-flow): no navigation target in F2 */}
      <button className="flex-shrink-0 px-3 py-1.5 rounded border border-border-strong text-[11.5px] font-medium text-text-2 hover:text-foreground hover:border-gold/50 transition-colors">
        Install missing
      </button>
    </div>
  );
}
