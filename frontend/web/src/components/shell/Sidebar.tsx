import { NavLink, useNavigate } from "react-router-dom";
import { Icon, type IconName } from "@/components/primitives/Icon";

type Item = { to: string; label: string; icon: IconName };

const PRIMARY: Item[] = [
  { to: "/", label: "Home", icon: "home" },
  { to: "/strategies", label: "Strategies", icon: "chart" },
  { to: "/eval-runs", label: "Eval", icon: "bars" },
  { to: "/settings", label: "Settings", icon: "sliders" },
];

export function Sidebar() {
  const navigate = useNavigate();
  return (
    <aside className="bg-surface-sidebar border-r border-border-soft flex flex-col w-[200px] pt-6 pb-4">
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

      <div className="mx-4 mb-4 px-3.5 py-3.5 bg-gold/5 border border-gold/20 rounded-sm">
        <h4 className="m-0 mb-1.5 text-[13px] font-semibold text-text">Setup agent</h4>
        <p className="m-0 mb-3 text-text-2 text-[12px] leading-snug">
          Add an LLM key to begin building strategies with xvn.
        </p>
        <button
          type="button"
          onClick={() => navigate("/settings/providers")}
          className="w-full flex items-center justify-center px-3 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors"
        >
          Add LLM key
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
