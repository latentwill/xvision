import { NavLink } from "react-router-dom";
import { Icon } from "@/components/primitives/Icon";
import { BrandMark } from "@/components/primitives/BrandMark";
import { useTheme } from "@/theme/useTheme";
import { WalletConnectFooter } from "@/components/shell/WalletConnectFooter";
import { PRIMARY_NAV } from "@/components/shell/nav";

/**
 * The primary left nav.
 *
 * `compact` renders an icon-only rail (~60px) used by the tablet shell
 * (768–1279px), where the full 220px sidebar was previously absent — leaving
 * the nav unreachable at that breakpoint (QA: "side menu disappears when screen
 * res is too small"). Labels collapse to `title` tooltips; the theme toggle
 * stacks; the wallet footer (text-heavy) is hidden — the wallet lives on its
 * own settings tab.
 */
export function Sidebar({
  className = "",
  compact = false,
}: {
  className?: string;
  compact?: boolean;
}) {
  const { resolvedTheme, setDarkTheme, setLightTheme } = useTheme();
  const isLight = resolvedTheme === "light";

  return (
    <aside
      className={[
        "bg-surface-sidebar border-r border-border-soft flex flex-col pt-6 pb-4",
        compact ? "w-[60px]" : "w-[220px]",
        // Pin to the viewport so the theme toggle + account row stay anchored
        // to the bottom of the screen instead of scrolling away with a tall
        // main column (the shell grid is min-h-screen, which would otherwise
        // stretch this aside to full page height).
        "sticky top-0 h-screen",
        className,
      ].join(" ")}
    >
      <div className={compact ? "flex justify-center pb-8" : "px-6 pb-8"}>
        <BrandMark height={compact ? 14 : 24} />
      </div>

      <nav className="flex-1 flex flex-col min-h-0 overflow-y-auto">
        {PRIMARY_NAV.map((it) => (
          <div key={it.to}>
            <NavLink
              to={it.to}
              end={it.to === "/"}
              title={compact ? it.label : undefined}
              className={({ isActive }) =>
                [
                  "flex items-center border-l-2 text-[13.5px] transition-colors",
                  compact
                    ? "justify-center px-0 py-3"
                    : "gap-3 px-6 py-2.5",
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
                  {!compact && <span>{it.label}</span>}
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
        <div
          className={[
            "mb-3 flex items-center gap-1 rounded border border-border-soft bg-surface-elev p-1",
            compact ? "mx-2 flex-col" : "mx-4",
          ].join(" ")}
        >
          <button
            type="button"
            onClick={setLightTheme}
            aria-label="Switch to light theme"
            className={[
              "flex h-7 flex-1 items-center justify-center rounded text-text-3 transition-colors hover:text-text",
              compact ? "w-full" : "",
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
              compact ? "w-full" : "",
              !isLight ? "bg-gold/[0.12] text-gold" : "",
            ].join(" ")}
          >
            <Icon name="moon" size={15} />
          </button>
        </div>

        {!compact && <WalletConnectFooter />}
      </div>
    </aside>
  );
}
