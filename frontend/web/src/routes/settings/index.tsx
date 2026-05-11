import { useState } from "react";
import { Outlet, NavLink } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  clearAlpacaCredentials,
  getBrokers,
  getDaemon,
  getIdentity,
  setAlpacaCredentials,
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
          <AlpacaBrokerCard entry={data.alpaca} />
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

export { SettingsDangerRoute } from "./danger";

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


function AlpacaBrokerCard({ entry }: { entry: BrokerEntry }) {
  const qc = useQueryClient();
  const [editing, setEditing] = useState(!entry.stored);
  const [apiKeyId, setApiKeyId] = useState("");
  const [apiSecretKey, setApiSecretKey] = useState("");
  const [baseUrl, setBaseUrl] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () =>
      setAlpacaCredentials({
        api_key_id: apiKeyId.trim(),
        api_secret_key: apiSecretKey.trim(),
        base_url: baseUrl.trim() ? baseUrl.trim() : null,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKeys.brokers() });
      setApiKeyId("");
      setApiSecretKey("");
      setBaseUrl("");
      setErrorMsg(null);
      setEditing(false);
    },
    onError: (err) => {
      const detail =
        err instanceof ApiError
          ? `${err.code}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err);
      setErrorMsg(detail);
    },
  });

  const clear = useMutation({
    mutationFn: clearAlpacaCredentials,
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKeys.brokers() });
      setErrorMsg(null);
      setEditing(true);
    },
    onError: (err) => {
      const detail =
        err instanceof ApiError
          ? `${err.code}: ${err.message}`
          : err instanceof Error
            ? err.message
            : String(err);
      setErrorMsg(detail);
    },
  });

  const onSubmit = (e: React.FormEvent) => {
    e.preventDefault();
    if (!apiKeyId.trim() || !apiSecretKey.trim()) {
      setErrorMsg("Both API key id and secret are required");
      return;
    }
    save.mutate();
  };

  const showForm = editing || !entry.stored;
  const showStored = entry.stored && !editing;

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

      {showStored ? (
        <div className="mt-2 space-y-3">
          <div className="flex items-center justify-between gap-3 px-3 py-2 bg-surface-elev border border-border-soft rounded">
            <div className="text-[13px] text-text-2">
              Stored key id ending in{" "}
              <code className="font-mono text-text">
                ••••{entry.stored_key_id_suffix ?? "····"}
              </code>
              {entry.base_url ? (
                <>
                  {" "}
                  · base url{" "}
                  <code className="font-mono text-text-3">{entry.base_url}</code>
                </>
              ) : null}
            </div>
            <div className="flex items-center gap-2">
              <button
                type="button"
                onClick={() => {
                  setEditing(true);
                  setErrorMsg(null);
                }}
                className="text-[12px] text-text-2 hover:text-text underline-offset-2 hover:underline"
              >
                replace
              </button>
              <button
                type="button"
                onClick={() => clear.mutate()}
                disabled={clear.isPending}
                className="text-[12px] text-danger hover:underline disabled:opacity-50"
              >
                {clear.isPending ? "clearing…" : "clear"}
              </button>
            </div>
          </div>
          <details className="text-[12px] text-text-3">
            <summary className="cursor-pointer hover:text-text-2">
              Env-var fallback status
            </summary>
            <table className="w-full mt-2">
              <tbody>
                {entry.credentials.map((c) => (
                  <CredentialRow key={c.env_var} cred={c} />
                ))}
              </tbody>
            </table>
          </details>
        </div>
      ) : null}

      {showForm ? (
        <form onSubmit={onSubmit} className="space-y-3">
          {!entry.stored ? (
            <p className="m-0 text-[12px] text-text-3 leading-snug">
              Paste your Alpaca paper-trading credentials. They're saved to{" "}
              <code className="font-mono text-text-2">
                ~/.xvn/secrets/brokers.toml
              </code>{" "}
              (owner-only) and used by{" "}
              <code className="font-mono text-text-2">--mode paper</code>{" "}
              eval runs. Env vars{" "}
              <code className="font-mono text-text-2">APCA_API_KEY_ID</code>{" "}
              and{" "}
              <code className="font-mono text-text-2">APCA_API_SECRET_KEY</code>{" "}
              still work as a fallback.
            </p>
          ) : null}
          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              API key id
            </label>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={apiKeyId}
              onChange={(e) => setApiKeyId(e.target.value)}
              placeholder="PK…"
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono placeholder:text-text-3 focus:outline-none focus:border-text-3"
            />
          </div>
          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              API secret key
            </label>
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={apiSecretKey}
              onChange={(e) => setApiSecretKey(e.target.value)}
              placeholder="••••••••••••••••"
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono placeholder:text-text-3 focus:outline-none focus:border-text-3"
            />
          </div>
          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              Base URL{" "}
              <span className="text-text-3">
                (optional — defaults to paper-api.alpaca.markets)
              </span>
            </label>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={baseUrl}
              onChange={(e) => setBaseUrl(e.target.value)}
              placeholder="https://paper-api.alpaca.markets"
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono placeholder:text-text-3 focus:outline-none focus:border-text-3"
            />
          </div>
          {errorMsg ? (
            <p className="m-0 text-[12px] text-danger font-mono">{errorMsg}</p>
          ) : null}
          <div className="flex items-center gap-2 pt-1">
            <button
              type="submit"
              disabled={save.isPending}
              className="px-3 py-1.5 rounded text-[13px] font-medium border border-gold text-gold hover:bg-gold/10 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {save.isPending ? "Saving…" : entry.stored ? "Replace" : "Save"}
            </button>
            {entry.stored ? (
              <button
                type="button"
                onClick={() => {
                  setEditing(false);
                  setErrorMsg(null);
                }}
                className="px-3 py-1.5 rounded text-[13px] text-text-2 hover:text-text"
              >
                Cancel
              </button>
            ) : null}
          </div>
        </form>
      ) : null}
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
