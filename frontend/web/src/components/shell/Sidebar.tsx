import { NavLink } from "react-router-dom";
import { Icon, type IconName } from "@/components/primitives/Icon";
import { BrandMark } from "@/components/primitives/BrandMark";
import { useTheme } from "@/theme/useTheme";
import { WalletConnectFooter } from "@/components/shell/WalletConnectFooter";
import { useMarketplaceOptIn } from "@/features/marketplace/lib/optin";

type Item = { to: string; label: string; icon: IconName };

// Charts section (chart-rework Track B) is now unconditional after
// B-rollout. The `xvn.chartv2` cookie gate was removed once B0–B4
// shipped — see docs/superpowers/plans/2026-05-23-charts-section-b5-hero-default-review.md
// for the rollout notes.
const PRIMARY: Item[] = [
  { to: "/", label: "Dashboard", icon: "home" },
  { to: "/strategies", label: "Strategies", icon: "chart" },
  { to: "/agents", label: "Agents", icon: "user" },
  { to: "/scenarios", label: "Scenarios", icon: "list" },
  { to: "/charts", label: "Charts", icon: "chartPie" },
  { to: "/eval-runs", label: "Eval", icon: "bars" },
  { to: "/live", label: "Live Trading", icon: "play" },
  { to: "/optimizer", label: "Optimizer", icon: "pulse" },
  { to: "/docs", label: "Docs", icon: "book" },
  { to: "/settings", label: "Settings", icon: "sliders" },
];

// Marketplace is opt-in (C8): hidden by default, surfaced in the sidebar only
// once enabled in Settings → Marketplace. Inserted after Eval, before Optimizer.
const MARKETPLACE_ITEM: Item = {
  to: "/marketplace",
  label: "Marketplace",
  icon: "bag",
};

export function Sidebar({ className = "" }: { className?: string }) {
  const { resolvedTheme, setDarkTheme, setLightTheme } = useTheme();
  const isLight = resolvedTheme === "light";
  const { enabled: marketplaceEnabled } = useMarketplaceOptIn();

  const items: Item[] = marketplaceEnabled
    ? PRIMARY.flatMap((it) =>
        it.to === "/optimizer" ? [MARKETPLACE_ITEM, it] : [it],
      )
    : PRIMARY;

  return (
    <aside
      className={[
        "bg-surface-sidebar border-r border-border-soft flex flex-col w-[220px] pt-6 pb-4",
        // Pin to the viewport so the theme toggle + account row stay anchored
        // to the bottom of the screen instead of scrolling away with a tall
        // main column (the shell grid is min-h-screen, which would otherwise
        // stretch this aside to full page height).
        "sticky top-0 h-screen",
        className,
      ].join(" ")}
    >
      <div className="px-6 pb-8">
        <BrandMark height={24} />
      </div>

      <nav className="flex-1 flex flex-col min-h-0 overflow-y-auto">
        {items.map((it) => (
          <div key={it.to}>
            <NavLink
              to={it.to}
              end={it.to === "/"}
              className={({ isActive }) =>
                [
                  "flex items-center gap-3 px-6 py-2.5 text-[13.5px] border-l-2 transition-colors",
                  isActive
                    ? "text-text border-gold bg-gold/[0.06]"
                    : "text-text-2 border-transparent hover:text-text",
                ].join(" ")
              }
            >
              {({ isActive }) => (
                <>
                  <span className={isActive ? "text-gold" : ""}>
                    <Icon name={it.icon} size={17} />
                  </span>
                  <span>{it.label}</span>
                </>
              )}
            </NavLink>
          </div>
        ))}
      </nav>

      {/*
        QA31: wrap theme toggle + user chip in a single `mt-auto` block
        so BOTH are anchored to the bottom of the sidebar, not just the
        chip. Previously only the chip had `mt-auto` while the theme
        toggle floated up directly beneath the nav — on short nav lists
        the theme toggle sat in the middle of the column with the chip
        glued to the bottom. Operators reported both should sit at the
        viewport bottom regardless of nav-list length.
      */}
      <div className="mt-auto">
        <div className="mx-4 mb-3 flex items-center gap-1 rounded border border-border-soft bg-surface-elev p-1">
          <button
            type="button"
            onClick={setLightTheme}
            aria-label="Switch to light theme"
            className={[
              "flex h-7 flex-1 items-center justify-center rounded text-text-3 transition-colors hover:text-text",
              isLight ? "bg-gold/[0.12] text-gold" : "",
            ].join(" ")}
          >
            <Icon name="sun" size={15} />
          </button>
          <button
            type="button"
            onClick={setDarkTheme}
            aria-label="Switch to dark theme"
            className={[
              "flex h-7 flex-1 items-center justify-center rounded text-text-3 transition-colors hover:text-text",
              !isLight ? "bg-gold/[0.12] text-gold" : "",
            ].join(" ")}
          >
            <Icon name="moon" size={15} />
          </button>
        </div>

        <WalletConnectFooter />
      </div>
    </aside>
  );
}
