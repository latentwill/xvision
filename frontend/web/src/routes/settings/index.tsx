import { useState } from "react";
import { Outlet, NavLink } from "react-router-dom";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { ApiError } from "@/api/client";
import {
  clearAlpacaCredentials,
  clearByrealCredentials,
  getBrokers,
  setAlpacaCredentials,
  setByrealCredentials,
  settingsKeys,
  testAlpacaConnection,
} from "@/api/settings";
import type {
  AlpacaTestReport,
  BrokerEntry,
  CredentialRef,
} from "@/api/types.gen";

// ── Market refresh types ──────────────────────────────────────────────────

interface RefreshResult {
  symbols_found: number;
  whitelist_path: string;
  registry_reloaded: boolean;
  message: string;
}

type RefreshState =
  | { status: "idle" }
  | { status: "loading" }
  | { status: "done"; result: RefreshResult }
  | { status: "error"; message: string };

const TABS = [
  { to: "general", label: "General" },
  { to: "providers", label: "Providers" },
  { to: "brokers", label: "Brokers" },
  { to: "wallet", label: "Wallet" },
  { to: "marketplace", label: "Marketplace" },
  { to: "danger", label: "Danger zone" },
];

export function SettingsLayout() {
  return (
    <>
      <Topbar
        title="Settings"
        sub="Connect LLM providers and broker accounts xvn uses to run agents"
      />
      <nav className="flex gap-1 mb-5 border-b border-border-soft overflow-x-auto">
        {TABS.map((t) => (
          <NavLink
            key={t.to}
            to={t.to}
            className={({ isActive }) =>
              [
                "shrink-0 px-3 py-2 text-[13px] -mb-px border-b-2",
                isActive
                  ? t.to === "danger"
                    ? "text-danger border-danger"
                    : "text-gold border-gold"
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

export { SettingsGeneralRoute } from "./general";
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
          <ByrealBrokerCard entry={data.byreal} />
          <MarketsRefreshCard />
        </div>
      )}
    </FetchStates>
  );
}

export { SettingsDangerRoute } from "./danger";
export { SettingsSkillsRoute } from "./skills";
export { SettingsWalletRoute } from "./wallet";
export { SettingsMarketplaceRoute } from "./marketplace";

// ────────────────────────────────────────────────────────────────────────────
// Markets refresh card

function MarketsRefreshCard() {
  const [state, setState] = useState<RefreshState>({ status: "idle" });

  const handleRefresh = async () => {
    setState({ status: "loading" });
    try {
      const res = await fetch("/api/assets/refresh", { method: "POST" });
      if (!res.ok) {
        const body = await res.text().catch(() => res.statusText);
        throw new Error(`${res.status} ${body}`);
      }
      const result: RefreshResult = await res.json();
      setState({ status: "done", result });
    } catch (e) {
      setState({ status: "error", message: String(e) });
    }
  };

  const isLoading = state.status === "loading";

  return (
    <Card className="p-5">
      <div className="flex items-start justify-between gap-4 mb-3">
        <div>
          <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
            Markets
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px] leading-snug max-w-lg">
            Fetch current perpetual markets from Orderly Network and update the
            asset registry file on disk.
          </p>
        </div>
        <button
          type="button"
          onClick={handleRefresh}
          disabled={isLoading}
          className="shrink-0 px-3 py-1.5 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 disabled:opacity-50 disabled:cursor-not-allowed"
        >
          {isLoading ? "Refreshing…" : "Refresh markets"}
        </button>
      </div>

      {state.status === "done" && (
        <div className="px-3 py-2 bg-surface-elev border border-border rounded text-[12px] space-y-1">
          <div className="text-info font-medium">
            {state.result.symbols_found} markets fetched
          </div>
          <div className="text-text-2">
            Saved to{" "}
            <code className="font-mono text-[11px] text-text">
              {state.result.whitelist_path}
            </code>
          </div>
          {!state.result.registry_reloaded && (
            <div className="text-text-3 text-[11px]">
              Restart xvn to apply the new market list.
            </div>
          )}
        </div>
      )}

      {state.status === "error" && (
        <div className="px-3 py-2 bg-surface-elev border border-border rounded text-[12px]">
          <span className="text-danger font-mono">{state.message}</span>
        </div>
      )}
    </Card>
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
        <div className="font-sans font-semibold text-[20px] text-danger mb-2">
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

  const test = useMutation<AlpacaTestReport, unknown, void>({
    mutationFn: testAlpacaConnection,
  });

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
          <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
            {entry.name}
          </h3>
          {entry.note ? (
            <p className="m-0 mt-1 text-text-3 text-[12px]">{entry.note}</p>
          ) : null}
        </div>
        <div className="flex items-center gap-2">
          {entry.configured ? (
            <button
              type="button"
              onClick={() => test.mutate()}
              disabled={test.isPending}
              title="Call /v2/account to verify the stored credentials"
              className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-50 disabled:cursor-not-allowed"
            >
              {test.isPending ? "Testing…" : "Test"}
            </button>
          ) : null}
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
      </div>

      {test.data || test.isError ? (
        <div className="mb-3 px-3 py-2 bg-surface-elev/50 border border-border-soft rounded text-[12px]">
          <AlpacaConnectionResult
            data={test.data ?? null}
            error={test.isError ? test.error : null}
          />
        </div>
      ) : null}

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
                Edit replacement
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
              eval runs.
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
              {save.isPending ? "Saving…" : entry.stored ? "Save replacement" : "Save"}
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

function AlpacaConnectionResult({
  data,
  error,
}: {
  data: AlpacaTestReport | null;
  error: unknown;
}) {
  if (error) {
    const detail =
      error instanceof ApiError
        ? `${error.code}: ${error.message}`
        : error instanceof Error
          ? error.message
          : String(error);
    return (
      <span className="inline-flex items-center gap-1.5 text-danger">
        <span aria-hidden>✗</span>
        <span className="font-mono text-text-2">{detail}</span>
      </span>
    );
  }
  if (!data) return null;
  if (data.ok) {
    return (
      <span className="inline-flex items-center gap-1.5 text-info">
        <span aria-hidden>✓</span>
        <span>
          connected · {data.latency_ms}ms
          {data.account_status ? ` · ${data.account_status}` : ""}
          {data.equity ? ` · equity $${data.equity}` : ""}
        </span>
      </span>
    );
  }
  return (
    <span className="inline-flex items-center gap-1.5 text-danger">
      <span aria-hidden>✗</span>
      <span className="font-mono text-text-2">
        {data.error ?? "connection failed"}
      </span>
    </span>
  );
}

function ByrealBrokerCard({ entry }: { entry: BrokerEntry }) {
  const qc = useQueryClient();
  const [editing, setEditing] = useState(!entry.stored);
  const [privateKey, setPrivateKey] = useState("");
  const [network, setNetwork] = useState("testnet");
  const [account, setAccount] = useState("");
  const [errorMsg, setErrorMsg] = useState<string | null>(null);

  const save = useMutation({
    mutationFn: () =>
      setByrealCredentials({
        private_key: privateKey.trim(),
        network: network.trim() ? network.trim() : null,
        account: account.trim() ? account.trim() : null,
      }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKeys.brokers() });
      setPrivateKey("");
      setAccount("");
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
    mutationFn: clearByrealCredentials,
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
    if (!privateKey.trim()) {
      setErrorMsg("Trading-only agent key is required");
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
          <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
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
              Stored agent key ending in{" "}
              <code className="font-mono text-text">
                ••••{entry.stored_key_id_suffix ?? "····"}
              </code>
            </div>
            <div className="flex items-center gap-3">
              <button
                type="button"
                onClick={() => {
                  setEditing(true);
                  setErrorMsg(null);
                }}
                className="text-[12px] text-text-2 hover:text-text underline-offset-2 hover:underline"
              >
                Edit replacement
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
        </div>
      ) : null}

      {showForm ? (
        <form onSubmit={onSubmit} className="space-y-3">
          <p className="m-0 text-[12px] text-text-3 leading-snug">
            Use a Hyperliquid{" "}
            <strong className="text-text-2">agent / API wallet key</strong> —
            trading-only, cannot withdraw.
          </p>
          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              Trading-only agent key
            </label>
            <input
              type="password"
              autoComplete="off"
              spellCheck={false}
              value={privateKey}
              onChange={(e) => setPrivateKey(e.target.value)}
              placeholder="0x…"
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] font-mono placeholder:text-text-3 focus:outline-none focus:border-text-3"
            />
          </div>
          <div>
            <label className="block text-[12px] text-text-2 mb-1">Network</label>
            <select
              value={network}
              onChange={(e) => setNetwork(e.target.value)}
              className="w-full px-3 py-2 bg-surface-elev border border-border rounded text-text text-[13px] focus:outline-none focus:border-text-3"
            >
              <option value="testnet">testnet</option>
              <option value="mainnet">mainnet</option>
            </select>
          </div>
          <div>
            <label className="block text-[12px] text-text-2 mb-1">
              Account id <span className="text-text-3">(optional)</span>
            </label>
            <input
              type="text"
              autoComplete="off"
              spellCheck={false}
              value={account}
              onChange={(e) => setAccount(e.target.value)}
              placeholder=""
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
              {save.isPending ? "Saving…" : entry.stored ? "Save replacement" : "Save"}
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
          <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
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

const FRIENDLY_NAMES: Record<string, string> = {
  ORDERLY_KEY: "API Key",
  ORDERLY_SECRET: "API Secret",
  ALPACA_API_KEY: "API Key",
  ALPACA_SECRET_KEY: "API Secret",
  ALPACA_BASE_URL: "Base URL",
  BYREAL_API_KEY: "API Key",
  BYREAL_API_SECRET: "API Secret",
};

function CredentialRow({ cred }: { cred: CredentialRef }) {
  const friendly = FRIENDLY_NAMES[cred.env_var];
  return (
    <tr className="border-t border-border-soft first:border-t-0">
      <td className="py-2">
        {friendly ? (
          <span className="flex flex-col gap-0.5">
            <span className="text-[12px] text-text">{friendly}</span>
            <code className="font-mono text-[10px] text-text-4">{cred.env_var}</code>
          </span>
        ) : (
          <code className="font-mono text-[12px] text-text-2">{cred.env_var}</code>
        )}
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
