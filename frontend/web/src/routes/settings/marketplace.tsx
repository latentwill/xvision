import { Card } from "@/components/primitives/Card";
import { useMarketplaceOptIn } from "@/features/marketplace/lib/optin";

export function SettingsMarketplaceRoute() {
  const { enabled, setEnabled } = useMarketplaceOptIn();

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Marketplace
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            The marketplace lets you browse, buy, and list strategies. It is
            hidden by default; enable it to add the Marketplace tab to the
            sidebar.
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 rounded border border-border-soft bg-surface-elev px-4 py-3">
          <div>
            <div className="text-[13px] text-text font-medium">
              Enable Marketplace
            </div>
            <div className="text-[12px] text-text-3 mt-0.5">
              {enabled
                ? "Marketplace is on. The sidebar entry and /marketplace routes are available."
                : "Marketplace is off. Sidebar entry hidden; /marketplace routes redirect here."}
            </div>
          </div>
          <button
            type="button"
            role="switch"
            aria-checked={enabled}
            aria-label="Enable Marketplace"
            onClick={() => setEnabled(!enabled)}
            className={[
              "relative inline-flex h-6 w-11 shrink-0 items-center rounded-full border transition-colors",
              enabled
                ? "bg-gold/[0.2] border-gold/50"
                : "bg-surface border-border-strong",
            ].join(" ")}
          >
            <span
              className={[
                "inline-block h-4 w-4 transform rounded-full transition-transform",
                enabled ? "translate-x-6 bg-gold" : "translate-x-1 bg-text-3",
              ].join(" ")}
            />
          </button>
        </div>
      </Card>

      <Card className="p-5">
        <div className="mb-4">
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Marketplace Profile
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            Seller display details, listing previews, and generated art are
            managed from the Marketplace sell flow so the profile stays tied to
            the strategy being published.
          </p>
        </div>

        <div className="grid gap-3 sm:grid-cols-3">
          <div className="rounded border border-border-soft bg-surface-elev px-4 py-3">
            <div className="text-[11px] uppercase tracking-[0.08em] text-text-3">
              Display source
            </div>
            <div className="mt-1 text-[13px] font-medium text-text">
              Selected strategy
            </div>
          </div>
          <div className="rounded border border-border-soft bg-surface-elev px-4 py-3">
            <div className="text-[11px] uppercase tracking-[0.08em] text-text-3">
              Preview art
            </div>
            <div className="mt-1 text-[13px] font-medium text-text">
              Listing card
            </div>
          </div>
          <div className="rounded border border-border-soft bg-surface-elev px-4 py-3">
            <div className="text-[11px] uppercase tracking-[0.08em] text-text-3">
              Edit path
            </div>
            <div className="mt-1 text-[13px] font-medium text-text">
              Marketplace sell
            </div>
          </div>
        </div>
      </Card>
    </div>
  );
}
