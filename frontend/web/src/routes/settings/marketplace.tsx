import { Card } from "@/components/primitives/Card";
import { useMarketplaceOptIn } from "@/features/marketplace/lib/optin";
import { TestnetBadge } from "@/features/marketplace/components/TestnetBadge";

export function SettingsMarketplaceRoute() {
  const { enabled, setEnabled } = useMarketplaceOptIn();

  return (
    <div className="space-y-5">
      <Card className="p-5">
        <div className="mb-4">
          <div className="flex items-center gap-2">
            <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
              Marketplace
            </h3>
            <TestnetBadge size="sm" />
          </div>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-2xl">
            The marketplace lets you browse, buy, and list strategies. It is a
            Mantle <span className="text-text-2">testnet</span> feature —
            purchases are simulated and no real funds move. It is hidden by
            default; enable it to add the Marketplace tab to the sidebar.
          </p>
        </div>

        <div className="flex items-center justify-between gap-4 rounded border border-border-soft bg-surface-elev px-4 py-3">
          <div>
            <div className="text-[13px] text-text font-medium">
              Enable Marketplace (Testnet)
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
            aria-label="Enable Marketplace (Testnet)"
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
    </div>
  );
}
