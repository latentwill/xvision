import { Outlet, NavLink } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";

const TABS = [
  { to: "providers", label: "Providers" },
  { to: "brokers", label: "Brokers" },
  { to: "daemon", label: "Daemon" },
  { to: "identity", label: "Identity" },
  { to: "danger", label: "Danger zone" },
];

export function SettingsLayout() {
  return (
    <>
      <Topbar title="Settings" sub="LLM keys · brokers · daemon · identity" />
      <nav className="flex gap-1 mb-5 border-b border-border-soft">
        {TABS.map((t) => (
          <NavLink
            key={t.to}
            to={t.to}
            className={({ isActive }) =>
              [
                "px-3 py-2 text-[13px] -mb-px border-b-2",
                isActive
                  ? "text-gold border-gold"
                  : "text-text-2 border-transparent hover:text-text",
              ].join(" ")
            }
          >
            {t.label}
          </NavLink>
        ))}
      </nav>
      <Outlet />
    </>
  );
}

export function SettingsProvidersRoute() {
  return <p className="text-text-2">LLM provider keys configuration (placeholder).</p>;
}
export function SettingsBrokersRoute() {
  return <p className="text-text-2">Alpaca / Orderly credentials (placeholder).</p>;
}
export function SettingsDaemonRoute() {
  return <p className="text-text-2">Live daemon controls (placeholder).</p>;
}
export function SettingsIdentityRoute() {
  return <p className="text-text-2">On-chain identity (placeholder).</p>;
}
export function SettingsDangerRoute() {
  return <p className="text-text-2">Reset, wipe, factory restore (placeholder).</p>;
}
