import { useEffect, useRef, type KeyboardEvent } from "react";
import { NavLink, useNavigate } from "react-router-dom";

import { Icon, type IconName } from "@/components/primitives/Icon";
import { useUi } from "@/stores/ui";

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
  const drawerRef = useRef<HTMLElement | null>(null);
  const previousFocusRef = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previousFocusRef.current = document.activeElement as HTMLElement | null;
    const first = focusableElements(drawerRef.current)[0];
    first?.focus();

    return () => {
      previousFocusRef.current?.focus();
      previousFocusRef.current = null;
    };
  }, [open]);

  function onDrawerKeyDown(event: KeyboardEvent<HTMLElement>) {
    if (event.key === "Escape") {
      event.preventDefault();
      setOpen(false);
      return;
    }
    if (event.key !== "Tab") return;

    const focusable = focusableElements(drawerRef.current);
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (event.shiftKey && document.activeElement === first) {
      event.preventDefault();
      last.focus();
    } else if (!event.shiftKey && document.activeElement === last) {
      event.preventDefault();
      first.focus();
    }
  }

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50 md:hidden">
      <button
        type="button"
        className="absolute inset-0 bg-black/55 backdrop-blur-[2px]"
        aria-label="Close navigation"
        onClick={() => setOpen(false)}
      />
      <aside
        ref={drawerRef}
        role="dialog"
        aria-modal="true"
        aria-label="Navigation"
        onKeyDown={onDrawerKeyDown}
        className="absolute top-0 bottom-0 left-0 w-[84vw] max-w-[340px] bg-surface-sidebar border-r border-border-soft flex flex-col py-4 pt-[max(1rem,env(safe-area-inset-top))] pb-[max(0px,env(safe-area-inset-bottom))] pl-[max(0px,env(safe-area-inset-left))]"
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
        <nav className="flex flex-col">
          {NAV.map((item) =>
            item.disabled ? (
              <div
                key={item.to}
                className="flex items-center gap-3 px-5 py-3 text-[15px] text-text-4 border-l-2 border-transparent"
                aria-disabled
              >
                <Icon name={item.icon} size={17} />
                <span>{item.label}</span>
                <span className="ml-auto text-[10px] font-mono text-text-4">
                  Later
                </span>
              </div>
            ) : (
              <NavLink
                key={item.to}
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
            ),
          )}
        </nav>
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
      </aside>
    </div>
  );
}

function focusableElements(root: HTMLElement | null): HTMLElement[] {
  if (!root) return [];
  return Array.from(
    root.querySelectorAll<HTMLElement>(
      'a[href], button:not([disabled]), [tabindex]:not([tabindex="-1"])',
    ),
  ).filter((el) => !el.hasAttribute("aria-disabled"));
}
