import { NavLink } from "react-router-dom";
import { Icon, type IconName } from "@/components/primitives/Icon";
import { useTheme } from "@/theme/useTheme";

type Item = { to: string; label: string; icon: IconName };

const PRIMARY: Item[] = [
  { to: "/", label: "Dashboard", icon: "home" },
  { to: "/strategies", label: "Strategies", icon: "chart" },
  { to: "/strategies-folder", label: "Folder", icon: "book" },
  { to: "/agents", label: "Agents", icon: "user" },
  { to: "/scenarios", label: "Scenarios", icon: "list" },
  { to: "/eval-runs", label: "Eval", icon: "bars" },
  { to: "/docs", label: "Docs", icon: "book" },
  { to: "/settings", label: "Settings", icon: "sliders" },
];

export function Sidebar({ className = "" }: { className?: string }) {
  const { resolvedTheme, setDarkTheme, setLightTheme } = useTheme();
  const isLight = resolvedTheme === "light";

  return (
    <aside
      className={[
        "bg-surface-sidebar border-r border-border-soft flex flex-col w-[220px] pt-6 pb-4",
        className,
      ].join(" ")}
    >
      <div className="px-6 pb-8">
        <span className="font-serif italic font-medium text-[38px] tracking-tight text-text leading-none">
          xvn
        </span>
      </div>

      <nav className="flex-1 flex flex-col">
        {PRIMARY.map((it) => (
          <NavLink
            key={it.to}
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
        ))}
      </nav>

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

      <div className="flex items-center gap-2.5 px-4 py-3.5 border-t border-border-soft mt-auto">
        <div className="w-8 h-8 rounded-full bg-surface-panel border border-border flex items-center justify-center text-[11px] font-semibold text-text">
          AK
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text leading-tight">Alex Kim</div>
          <div className="text-[11px] text-text-3 leading-tight">alex@xvn.dev</div>
        </div>
        <Icon name="chevR" size={14} className="text-text-3" />
      </div>
    </aside>
  );
}
