import { Outlet, NavLink } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  getBrokers,
  getDaemon,
  getIdentity,
  settingsKeys,
} from "@/api/settings";
import type {
  BrokerEntry,
  CredentialRef,
  DaemonStatus,
  IdentityReport,
} from "@/api/types.gen";

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

export { SettingsProvidersRoute } from "./providers";

export function SettingsBrokersRoute() {
  const q = useQuery({
    queryKey: settingsKeys.brokers(),
    queryFn: getBrokers,
  });
  return (
    <FetchStates query={q} empty={false}>
      {(data) => (
        <div className="space-y-5">
          <BrokerCard entry={data.alpaca} />
          <BrokerCard entry={data.orderly} />
        </div>
      )}
    </FetchStates>
  );
}

export function SettingsDaemonRoute() {
  const q = useQuery({
    queryKey: settingsKeys.daemon(),
    queryFn: getDaemon,
  });
  return (
    <FetchStates query={q} empty={false}>
      {(data) => (
        <Card className="p-6">
          <div className="flex items-center gap-3 mb-4">
            <DaemonPill status={data.status} />
            <span className="text-text-2 text-[12px]">live deploy daemon</span>
          </div>
          <p className="m-0 mb-3 text-text leading-snug">{data.note}</p>
          {data.deferred_to_plan ? (
            <p className="m-0 text-[12px] text-text-3">
              Lands with{" "}
              <code className="text-text-2 font-mono">
                {data.deferred_to_plan}
              </code>
              .
            </p>
          ) : null}
        </Card>
      )}
    </FetchStates>
  );
}

export function SettingsIdentityRoute() {
  const q = useQuery({
    queryKey: settingsKeys.identity(),
    queryFn: getIdentity,
  });
  return (
    <FetchStates query={q} empty={false}>
      {(data) => <IdentityCard data={data} />}
    </FetchStates>
  );
}

export function SettingsDangerRoute() {
  return (
    <PlaceholderTab
      hint="Wipe + reset operations are deferred until eval and strategy stores stabilise."
      doc="(out-of-scope for v1)"
    />
  );
}

// ────────────────────────────────────────────────────────────────────────────
// helpers

function FetchStates<T>({
  query,
  empty,
  children,
}: {
  query: { isPending: boolean; isError: boolean; data: T | undefined; error: unknown; refetch: () => void };
  empty: boolean;
  children: (data: T) => React.ReactNode;
}) {
  if (query.isPending) {
    return (
      <Card className="p-6 animate-pulse">
        <div className="h-4 w-48 bg-surface-elev rounded mb-3" />
        <div className="h-4 w-72 bg-surface-elev rounded" />
      </Card>
    );
  }
  if (query.isError || !query.data) {
    const detail =
      query.error instanceof ApiError
        ? `${query.error.code}: ${query.error.message}`
        : query.error instanceof Error
          ? query.error.message
          : "unknown error";
    return (
      <Card className="p-6">
        <div className="font-serif italic text-[20px] text-danger mb-2">
          couldn't load
        </div>
        <p className="m-0 mb-4 text-text-2 text-[13px]">
          <code className="text-danger font-mono text-[12px]">{detail}</code>
        </p>
        <button
          onClick={() => query.refetch()}
          className="px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
        >
          Retry
        </button>
      </Card>
    );
  }
  if (empty) {
    return <Card className="p-6 text-text-2">No data.</Card>;
  }
  return <>{children(query.data)}</>;
}

function PlaceholderTab({ hint, doc }: { hint: string; doc: string }) {
  return (
    <Card className="px-6 py-12 text-center text-text-2">
      <div className="font-serif italic text-[24px] text-text-3 mb-3">
        coming soon
      </div>
      <p className="m-0 mb-2 max-w-md mx-auto leading-snug">{hint}</p>
      <p className="m-0 text-[12px] text-text-3 font-mono">{doc}</p>
    </Card>
  );
}

function BrokerCard({ entry }: { entry: BrokerEntry }) {
  return (
    <Card className="p-5">
      <div className="flex items-center justify-between mb-3">
        <div>
          <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
            {entry.name}
          </h3>
          {entry.note ? (
            <p className="m-0 mt-1 text-text-3 text-[12px]">{entry.note}</p>
          ) : null}
        </div>
        {entry.configured ? (
          <Pill tone="gold">
            <span className="w-1.5 h-1.5 rounded-full bg-gold" /> configured
          </Pill>
        ) : (
          <Pill tone="warn">
            <span className="w-1.5 h-1.5 rounded-full bg-warn" /> not configured
          </Pill>
        )}
      </div>

      <table className="w-full mt-2">
        <tbody>
          {entry.credentials.map((c) => (
            <CredentialRow key={c.env_var} cred={c} />
          ))}
          {entry.base_url ? (
            <tr className="border-t border-border-soft">
              <td className="py-2 text-text-2 text-[12px]">base url</td>
              <td className="py-2 text-right">
                <code className="font-mono text-[12px] text-text">
                  {entry.base_url}
                </code>
              </td>
            </tr>
          ) : null}
        </tbody>
      </table>
    </Card>
  );
}

function CredentialRow({ cred }: { cred: CredentialRef }) {
  return (
    <tr className="border-t border-border-soft first:border-t-0">
      <td className="py-2">
        <code className="font-mono text-[12px] text-text-2">{cred.env_var}</code>
      </td>
      <td className="py-2 text-right">
        {cred.is_set ? (
          <span className="text-gold text-[12px]">● set</span>
        ) : (
          <span className="text-text-3 text-[12px]">○ unset</span>
        )}
      </td>
    </tr>
  );
}

function DaemonPill({ status }: { status: DaemonStatus }) {
  if (status === "running") {
    return (
      <Pill tone="gold">
        <span className="w-1.5 h-1.5 rounded-full bg-gold" /> running
      </Pill>
    );
  }
  if (status === "stopped") {
    return (
      <Pill tone="warn">
        <span className="w-1.5 h-1.5 rounded-full bg-warn" /> stopped
      </Pill>
    );
  }
  return (
    <Pill>
      <span className="w-1.5 h-1.5 rounded-full bg-text-3" /> not in v1
    </Pill>
  );
}

function IdentityCard({ data }: { data: IdentityReport }) {
  return (
    <Card className="p-5">
      <div className="flex items-center justify-between mb-3">
        <h3 className="m-0 font-serif font-medium text-[20px] tracking-tight">
          On-chain identity
        </h3>
        {data.feature_compiled_in ? (
          <Pill tone="gold">
            <span className="w-1.5 h-1.5 rounded-full bg-gold" /> compiled in
          </Pill>
        ) : (
          <Pill>
            <span className="w-1.5 h-1.5 rounded-full bg-text-3" /> not compiled
          </Pill>
        )}
      </div>
      <p className="m-0 mb-4 text-text-2 text-[13px]">{data.note}</p>

      <table className="w-full">
        <tbody>
          <tr className="border-t border-border-soft">
            <td className="py-2 text-text-2 text-[12px]">MANTLE_RPC_URL</td>
            <td className="py-2 text-right">
              {data.wallet.rpc_url_set ? (
                <span className="text-gold text-[12px]">● set</span>
              ) : (
                <span className="text-text-3 text-[12px]">○ unset</span>
              )}
            </td>
          </tr>
          <tr className="border-t border-border-soft">
            <td className="py-2 text-text-2 text-[12px]">XVN_WALLET_KEY</td>
            <td className="py-2 text-right">
              {data.wallet.wallet_key_set ? (
                <span className="text-gold text-[12px]">● set</span>
              ) : (
                <span className="text-text-3 text-[12px]">○ unset</span>
              )}
            </td>
          </tr>
        </tbody>
      </table>
    </Card>
  );
}
