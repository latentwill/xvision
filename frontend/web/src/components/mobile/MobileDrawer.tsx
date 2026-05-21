import { NavLink, useNavigate } from "react-router-dom";

import { Icon, type IconName } from "@/components/primitives/Icon";
import { useUi } from "@/stores/ui";

// MobileDrawer is the phone-breakpoint navigation surface. It is
// *not* a popup — when open it occupies the full viewport with an
// opaque background, fully replacing (not overlaying) the underlying
// content per the workspace no-popups rule. No modal semantics, no
// focus trap, no backdrop, no click-outside-to-close.

type NavItem = {
  to: string;
  label: string;
  icon: IconName;
  disabled?: boolean;
  count?: string;
};

const NAV: NavItem[] = [
  { to: "/", label: "Dashboard", icon: "home" },
  { to: "/strategies", label: "Strategies", icon: "chart" },
  { to: "/agents", label: "Agents", icon: "user" },
  { to: "/eval-runs", label: "Eval", icon: "bars" },
  { to: "/live", label: "Live", icon: "play", disabled: true },
  { to: "/journal", label: "Journal", icon: "book", disabled: true },
  { to: "/settings", label: "Settings", icon: "sliders" },
];

export function MobileDrawer() {
  const open = useUi((s) => s.mobileDrawerOpen);
  const setOpen = useUi((s) => s.setMobileDrawerOpen);
  const navigate = useNavigate();

  if (!open) return null;

  return (
    <nav
      aria-label="Navigation"
      className="fixed inset-0 z-50 md:hidden bg-surface-sidebar flex flex-col py-4 pt-[max(1rem,env(safe-area-inset-top))] pb-[max(0px,env(safe-area-inset-bottom))]"
    >
      <div className="px-5 pb-5 flex items-center justify-between">
        <span className="font-serif italic font-medium text-[34px] tracking-tight text-text">
          xvn
        </span>
        <button
          type="button"
          onClick={() => setOpen(false)}
          className="w-9 h-9 rounded-full flex items-center justify-center text-text-2 hover:text-text hover:bg-surface-hover"
          aria-label="Close navigation"
        >
          <Icon name="arrow" size={16} />
        </button>
      </div>
      <ul className="flex flex-col list-none m-0 p-0">
        {NAV.map((item) =>
          item.disabled ? (
            <li
              key={item.to}
              className="flex items-center gap-3 px-5 py-3 text-[15px] text-text-4 border-l-2 border-transparent"
              aria-disabled
            >
              <Icon name={item.icon} size={17} />
              <span>{item.label}</span>
              <span className="ml-auto text-[10px] font-mono text-text-4">
                Later
              </span>
            </li>
          ) : (
            <li key={item.to} className="m-0">
              <NavLink
                to={item.to}
                end={item.to === "/"}
                onClick={() => setOpen(false)}
                className={({ isActive }) =>
                  [
                    "flex items-center gap-3 px-5 py-3 text-[15px] border-l-2 transition-colors",
                    isActive
                      ? "text-text border-gold bg-gold/[0.06]"
                      : "text-text-2 border-transparent hover:text-text",
                  ].join(" ")
                }
              >
                <Icon name={item.icon} size={17} />
                <span>{item.label}</span>
                {item.count && (
                  <span className="ml-auto text-[11px] font-mono text-text-3">
                    {item.count}
                  </span>
                )}
              </NavLink>
            </li>
          ),
        )}
      </ul>
      <div className="m-4 p-3.5 bg-gold/5 border border-gold/20 rounded-sm">
        <h4 className="m-0 mb-1 text-[13px] font-semibold text-text">
          Conversations
        </h4>
        <p className="m-0 mb-3 text-[12px] leading-snug text-text-2">
          Resume a past thread or start fresh.
        </p>
        <button
          type="button"
          onClick={() => {
            setOpen(false);
            navigate("/");
          }}
          className="w-full px-3 py-2 rounded-sm border border-border text-[12px] text-text-2 hover:text-text"
        >
          View history
        </button>
      </div>
      <div className="mt-auto flex items-center gap-2.5 px-5 py-3.5 border-t border-border-soft">
        <div className="w-8 h-8 rounded-full bg-surface-panel border border-border flex items-center justify-center text-[11px] font-semibold text-text">
          AK
        </div>
        <div className="flex-1 min-w-0">
          <div className="text-[13px] text-text leading-tight">Alex Kim</div>
          <div className="text-[11px] text-text-3 leading-tight truncate">
            alex@xvn.dev
          </div>
        </div>
        <Icon name="settings" size={14} className="text-text-3" />
      </div>
    </nav>
  );
}
