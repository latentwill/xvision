import { useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import {
  ResponsiveListCard,
  useListColumns,
  useListState,
  useListUrlState,
  type FilterDef,
  type SortOption,
} from "@/components/lists";
import { MListRow } from "@/components/lists/MListRow";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";
import { ApiError } from "@/api/client";
import {
  addProvider,
  listProviderModels,
  listProviders,
  refreshProviderCatalog,
  removeProvider,
  setEnabledModels,
  settingsKeys,
  testProviderConnection,
  updateProvider,
} from "@/api/settings";
import type {
  AddProviderRequest,
  Catalog,
  ProviderModelEntry,
  ProviderRow,
  TestConnectionReport,
  UpdateProviderRequest,
} from "@/api/types.gen";
import { logInfo, safeUrlHost } from "@/lib/logger";

// Provider presets the form recognises. Each preset fills in a sensible
// (wire kind, name, base URL) tuple; the user only has to paste an API key.
// Pick "Custom" for anything else — DeepSeek/Groq/Together-style endpoints
// that don't have a preset, or self-hosted Ollama / vLLM / llama.cpp.
type KindOption = {
  value: string;
  label: string;
  wireKind: "anthropic" | "openai-compat" | "ollama" | "llama-cpp" | "vllm";
  defaultName: string;
  defaultBaseUrl: string;
  isCustom: boolean;
  keyHelp: string;
};

const KIND_OPTIONS: ReadonlyArray<KindOption> = [
  {
    value: "anthropic",
    label: "Anthropic (Claude)",
    wireKind: "anthropic",
    defaultName: "anthropic",
    defaultBaseUrl: "https://api.anthropic.com",
    isCustom: false,
    keyHelp: "Starts with sk-ant-…",
  },
  {
    value: "openai",
    label: "OpenAI",
    wireKind: "openai-compat",
    defaultName: "openai",
    defaultBaseUrl: "https://api.openai.com/v1",
    isCustom: false,
    keyHelp: "Starts with sk-…",
  },
  {
    value: "deepseek",
    label: "DeepSeek",
    wireKind: "openai-compat",
    defaultName: "deepseek",
    // DeepSeek's OpenAI-compat root, per https://api-docs.deepseek.com.
    // We POST to `{base}/chat/completions` — no /v1 segment.
    defaultBaseUrl: "https://api.deepseek.com",
    isCustom: false,
    keyHelp: "Starts with sk-…",
  },
  {
    value: "groq",
    label: "Groq",
    wireKind: "openai-compat",
    defaultName: "groq",
    defaultBaseUrl: "https://api.groq.com/openai/v1",
    isCustom: false,
    keyHelp: "Starts with gsk_…",
  },
  {
    value: "openrouter",
    label: "OpenRouter",
    wireKind: "openai-compat",
    defaultName: "openrouter",
    defaultBaseUrl: "https://openrouter.ai/api/v1",
    isCustom: false,
    keyHelp: "Starts with sk-or-…",
  },
  {
    value: "gemini",
    label: "Google Gemini",
    wireKind: "openai-compat",
    defaultName: "gemini",
    // Gemini's OpenAI-compatible endpoint. Chat posts to
    // `{base}/chat/completions`; the catalog is at `{base}/models`. This is the
    // OpenAI-compat root, NOT the native generativelanguage API root.
    defaultBaseUrl: "https://generativelanguage.googleapis.com/v1beta/openai",
    isCustom: false,
    keyHelp: "Google AI Studio key (AIza…).",
  },
  {
    value: "nous-research",
    label: "Nous Research",
    wireKind: "openai-compat",
    defaultName: "nous-research",
    defaultBaseUrl: "https://inference-api.nousresearch.com/v1",
    isCustom: false,
    keyHelp: "Nous Portal API key.",
  },
  {
    value: "ollama",
    label: "Ollama (local)",
    wireKind: "ollama",
    defaultName: "ollama",
    defaultBaseUrl: "http://localhost:11434",
    isCustom: false,
    keyHelp: "Optional — leave blank for local Ollama.",
  },
  {
    value: "vllm",
    label: "vLLM (local)",
    wireKind: "vllm",
    defaultName: "vllm",
    defaultBaseUrl: "http://localhost:8000/v1",
    isCustom: false,
    keyHelp: "Optional — leave blank for local vLLM.",
  },
  // llama.cpp provider preset temporarily disabled — Ollama is the supported
  // local backend for now. Backend `ProviderKind::LlamaCpp` support remains, so
  // re-enabling is just restoring this block.
  // {
  //   value: "llama-cpp",
  //   label: "llama.cpp server",
  //   wireKind: "llama-cpp",
  //   defaultName: "llama-cpp",
  //   defaultBaseUrl: "http://localhost:8080",
  //   isCustom: false,
  //   keyHelp: "Optional — leave blank for local llama-server.",
  // },
  {
    value: "custom",
    label: "Custom (Together, vLLM, self-hosted, …)",
    wireKind: "openai-compat",
    defaultName: "",
    defaultBaseUrl: "",
    isCustom: true,
    keyHelp: "Leave blank for no-auth endpoints.",
  },
];

function keyRequired(meta: KindOption, baseUrl: string): boolean {
  if (meta.value === "ollama" || meta.value === "llama-cpp" || meta.value === "vllm") return false;
  if (meta.value === "custom" && /localhost|127\.0\.0\.1/.test(baseUrl)) {
    return false;
  }
  return true;
}

function isHttpUrl(s: string): boolean {
  return /^https?:\/\/.+/i.test(s);
}

const PROVIDER_SORT_OPTIONS: SortOption[] = [
  { value: "name", label: "Name A → Z" },
  { value: "kind", label: "Kind (Anthropic → OpenAI-compat)" },
];

const PROVIDER_KIND_FILTER: FilterDef = {
  id: "kind",
  label: "Kind",
  options: [
    { value: "all", label: "All kinds" },
    { value: "anthropic", label: "Anthropic" },
    { value: "openai-compat", label: "OpenAI-compat" },
    { value: "ollama", label: "Ollama" },
    { value: "vllm", label: "vLLM" },
    // llama.cpp filter disabled (see KIND_OPTIONS note above).
    // { value: "llama-cpp", label: "llama.cpp" },
  ],
};

const PROVIDER_COLUMNS = [
  { key: "name",     label: "Name",     essential: true, estWidth: 180 },
  { key: "kind",     label: "Kind",     priority: 3,     estWidth: 90  },
  { key: "base_url", label: "Base URL", priority: 2,     estWidth: 200, align: "left" as const },
  { key: "key",      label: "Key",      priority: 1,     estWidth: 100, align: "right" as const },
  { key: "actions",  label: "",         essential: true, estWidth: 60,  align: "right" as const },
];

export function SettingsProvidersRoute() {
  const qc = useQueryClient();
  const [adding, setAdding] = useState(false);
  const providersQuery = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });

  const remove = useMutation({
    mutationFn: (name: string) => removeProvider(name),
    onSuccess: () => qc.invalidateQueries({ queryKey: settingsKeys.providers() }),
  });

  const rows: ProviderRow[] = providersQuery.data?.providers ?? [];
  // Rows in the config that failed validation and were skipped by the lenient
  // loader. Surfaced so a hand-edited bad row doesn't silently vanish — the
  // operator can see why and remove it inline.
  const invalidRows = providersQuery.data?.invalid ?? [];

  const list = useListState<ProviderRow>({
    rows,
    filters: [PROVIDER_KIND_FILTER],
    sortOptions: PROVIDER_SORT_OPTIONS,
    filterFn: (row, query, values) => {
      const kind = values.kind ?? "all";
      if (kind !== "all" && row.kind !== kind) return false;
      const needle = query.trim().toLowerCase();
      if (needle.length === 0) return true;
      return (
        row.name.toLowerCase().includes(needle) ||
        row.kind.toLowerCase().includes(needle)
      );
    },
    sortFn: (rs, key) => {
      switch (key) {
        case "kind":
          return [...rs].sort((a, b) => {
            const cmp = a.kind.localeCompare(b.kind);
            return cmp !== 0 ? cmp : a.name.localeCompare(b.name);
          });
        case "name":
        default:
          return [...rs].sort((a, b) => a.name.localeCompare(b.name));
      }
    },
  });
  useListUrlState("settings-providers", list);
  const columnState = useListColumns("settings-providers", PROVIDER_COLUMNS);

  return (
    <div className="space-y-5">
      {/* Page chrome — title + add affordance + intro + add form stay above
          the standardised list card. The list itself handles loading /
          error / empty / populated states. */}
      <div className="flex items-center justify-between mb-2">
        <div>
          <h3 className="m-0 font-sans font-medium text-[20px] tracking-tight">
            LLM providers
          </h3>
          <p className="m-0 mt-1 text-text-3 text-[12px]">
            Models xvn can call for the agent rail and authoring wizard.
          </p>
        </div>
        {!adding && rows.length > 0 ? (
          <button
            onClick={() => setAdding(true)}
            className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] border border-border text-text hover:border-text-3"
          >
            + Add provider
          </button>
        ) : null}
      </div>
      <p className="m-0 text-text-3 text-[12px] leading-snug">
        Paste a provider's API key to enable it. Keys are stored on disk
        under <code className="font-mono">~/.xvn/secrets/providers.toml</code>{" "}
        (owner-only) — they never round-trip through this UI again.
      </p>

      {invalidRows.length > 0 ? (
        <div className="rounded-md border border-warn/40 bg-warn/[0.06] p-3 space-y-2">
          <p className="m-0 text-[12px] text-warn font-medium">
            {invalidRows.length} provider{" "}
            {invalidRows.length === 1 ? "row" : "rows"} in the config could not
            be loaded and {invalidRows.length === 1 ? "was" : "were"} skipped.
            Fix the name in the config or remove the{" "}
            {invalidRows.length === 1 ? "row" : "rows"} here:
          </p>
          <ul className="m-0 list-none space-y-1.5">
            {invalidRows.map((r) => (
              <li
                key={r.name}
                className="flex items-center justify-between gap-3 text-[12px]"
              >
                <span className="min-w-0 text-text-2">
                  <code className="font-mono text-text">
                    {r.name || "(empty name)"}
                  </code>
                  <span className="text-text-3 ml-2">{r.reason}</span>
                </span>
                <button
                  onClick={() => remove.mutate(r.name)}
                  disabled={remove.isPending && remove.variables === r.name}
                  className="shrink-0 px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-danger hover:border-danger disabled:opacity-40 disabled:cursor-not-allowed"
                >
                  {remove.isPending && remove.variables === r.name
                    ? "Removing…"
                    : "Remove"}
                </button>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      {adding || rows.length === 0 ? (
        <AddProviderForm
          allowCancel={rows.length > 0}
          onCancel={() => setAdding(false)}
          onAdded={() => {
            setAdding(false);
            qc.invalidateQueries({ queryKey: settingsKeys.providers() });
          }}
        />
      ) : null}

      <ResponsiveListCard<ProviderRow>
        listId="settings-providers"
        title="Configured providers"
        count={list.totalRows}
        toolbar={{
          search: { ...list.search, placeholder: "Search name or kind…" },
          filters: list.filters,
          sort: list.sort,
          clearAll: list.clearAll,
        }}
        columns={PROVIDER_COLUMNS}
        columnState={columnState}
        rows={list.rows}
        loading={providersQuery.isPending}
        error={
          providersQuery.isError
            ? {
                message: errorMessage(providersQuery.error),
                retry: () => providersQuery.refetch(),
              }
            : null
        }
        empty={
          rows.length === 0
            ? "No providers yet — use the form above to add one."
            : "No providers match these filters."
        }
        renderRow={(p) => (
          <ProviderRowView
            key={p.name}
            row={p}
            onRemove={() => remove.mutate(p.name)}
            removeError={
              remove.variables === p.name && remove.isError
                ? errorMessage(remove.error)
                : null
            }
            removeBusy={remove.variables === p.name && remove.isPending}
          />
        )}
        renderMobileRow={(p) => (
          <MListRow
            key={p.name}
            title={p.name}
            subtitle={safeUrlHost(p.base_url) || p.base_url}
            badge={p.kind}
            badgeColor={p.kind === "anthropic" ? "gold" : "muted"}
            meta={`env: ${p.api_key_env || "—"}`}
          />
        )}
      />
    </div>
  );
}

function ProviderRowView({
  row,
  onRemove,
  removeError,
  removeBusy,
}: {
  row: ProviderRow;
  onRemove: () => void;
  removeError: string | null;
  removeBusy: boolean;
}) {
  const [managing, setManaging] = useState(false);
  const [editing, setEditing] = useState(false);
  const qc = useQueryClient();
  const test = useMutation<TestConnectionReport, unknown, void>({
    mutationFn: () => testProviderConnection(row.name),
  });
  // Refresh fetches the provider's `/v1/models` upstream and writes the
  // full catalog (context window, max output tokens, pricing) to disk.
  // Invalidating both query keys keeps the chat-rail dropdown and the
  // SlotForm placeholder ("Auto: 384,000") in sync without a manual
  // page reload.
  const refresh = useMutation<Catalog, unknown, void>({
    mutationFn: () => refreshProviderCatalog(row.name),
    onSuccess: () => {
      qc.invalidateQueries({
        queryKey: settingsKeys.providerCatalog(row.name),
      });
      qc.invalidateQueries({
        queryKey: settingsKeys.providerModels(row.name),
      });
    },
  });
  // The Test / Refresh / Pick-models actions probe the provider's catalog
  // endpoint. They're available once we can authenticate — either a key is
  // set, or the provider is a no-auth kind (Ollama, llama.cpp, no-auth custom
  // localhost) where `api_key_env` is empty. Gating purely on `api_key_set`
  // hid every action for Ollama, which has no key by design.
  const canProbe = row.api_key_set || !row.api_key_env.trim();
  return (
    <>
      <tr className="border-t border-border-soft align-middle">
        {/*
          QA31: header cells get `paddingLeft: 20` on the first column
          via ListCard.tsx, but the row cells had no matching `pl-` so
          the provider names rendered flush against the card's left
          edge while their header label was indented 20px. Aligning the
          first cell with `pl-5` (20px) so the column reads as a
          proper column.
        */}
        <td className="py-2 pl-5 pr-3">
          <div className="flex items-center gap-2">
            <code className="font-mono text-[13px] text-text">{row.name}</code>
          </div>
        </td>
        <td className="py-2 pr-3 text-text-2 text-[12px] font-mono">
          {row.kind}
        </td>
        <td className="py-2 pr-3">
          <code className="font-mono text-[12px] text-text-2">
            {row.base_url}
          </code>
        </td>
        <td className="py-2 pr-3 text-right">
          {!row.api_key_env ? (
            <span className="text-text-3 text-[12px]">no auth</span>
          ) : row.api_key_set ? (
            <span className="text-gold text-[12px]">● set</span>
          ) : (
            <span className="text-warn text-[12px]">○ missing</span>
          )}
        </td>
        <td className="py-2 pr-0 text-right">
          <div className="inline-flex items-center gap-2">
            {canProbe ? (
              <button
                onClick={() => test.mutate()}
                disabled={test.isPending}
                title="Hit the provider's catalog endpoint to verify the key + base URL"
                className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {test.isPending ? "Testing…" : "Test"}
              </button>
            ) : null}
            {canProbe ? (
              <button
                onClick={() => refresh.mutate()}
                disabled={refresh.isPending}
                title="Fetch the provider's /v1/models catalog and persist context window + max output tokens + pricing for every model"
                className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-text hover:border-text-3 disabled:opacity-50 disabled:cursor-not-allowed"
              >
                {refresh.isPending
                  ? "Refreshing…"
                  : refresh.isSuccess
                    ? `Refreshed · ${refresh.data?.models.length}`
                    : "Refresh"}
              </button>
            ) : null}
            {canProbe ? (
              <button
                onClick={() => setManaging((m) => !m)}
                title="Pick which models from this provider show up in the chat-rail dropdown"
                className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-text hover:border-text-3"
              >
                {`Models · ${row.enabled_models.length}`}
              </button>
            ) : null}
            <button
              onClick={() => setEditing((v) => !v)}
              className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-text hover:border-text-3"
            >
              {editing ? "Cancel" : "Edit"}
            </button>
            <button
              onClick={onRemove}
              disabled={removeBusy}
              className="px-2 py-1 rounded text-[12px] border border-border text-text-2 hover:text-danger hover:border-danger disabled:opacity-30 disabled:cursor-not-allowed disabled:hover:text-text-2 disabled:hover:border-border"
            >
              {removeBusy ? "Removing…" : "Remove"}
            </button>
          </div>
        </td>
      </tr>
      {test.data || test.isError ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={5} className="py-1.5 pl-5 pr-5 text-[12px]">
            <ConnectionResult
              data={test.data ?? null}
              error={test.isError ? test.error : null}
            />
          </td>
        </tr>
      ) : null}
      {editing ? (
        <tr className="border-t border-border-soft/40 bg-surface-elev/20">
          <td colSpan={5} className="py-3 pl-5 pr-5">
            <EditProviderForm row={row} onClose={() => setEditing(false)} />
          </td>
        </tr>
      ) : null}
      {managing ? (
        <tr className="border-t border-border-soft/40 bg-surface-elev/20">
          <td colSpan={5} className="py-3 pl-5 pr-5">
            <ModelManager row={row} onClose={() => setManaging(false)} />
          </td>
        </tr>
      ) : null}
      {removeError ? (
        <tr className="border-t border-border-soft/40">
          <td colSpan={5} className="py-2 pl-5 pr-5 text-[12px] text-danger">
            {removeError}
          </td>
        </tr>
      ) : null}
    </>
  );
}

function ConnectionResult({
  data,
  error,
}: {
  data: TestConnectionReport | null;
  error: unknown;
}) {
  if (error) {
    return (
      <span className="inline-flex items-center gap-1.5 text-danger">
        <span aria-hidden>✗</span>
        <span className="font-mono text-text-2">{errorMessage(error)}</span>
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
          {data.model_count > 0 ? ` · ${data.model_count} models` : ""}
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

function EditProviderForm({
  row,
  onClose,
}: {
  row: ProviderRow;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [kind, setKind] = useState(row.kind);
  const [baseUrl, setBaseUrl] = useState(row.base_url);
  const [apiKey, setApiKey] = useState("");

  const save = useMutation({
    mutationFn: (req: UpdateProviderRequest) => updateProvider(row.name, req),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKeys.providers() });
      onClose();
    },
  });

  const trimmedBaseUrl = baseUrl.trim();
  const trimmedEnv = row.api_key_env;
  const errors: string[] = [];
  if (trimmedBaseUrl === "") {
    errors.push("base URL is required");
  } else if (!isHttpUrl(trimmedBaseUrl)) {
    errors.push("base URL must start with http:// or https://");
  }
  const dirty =
    kind !== row.kind ||
    trimmedBaseUrl !== row.base_url ||
    apiKey.trim() !== "";
  const submittable = dirty && errors.length === 0;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!submittable) return;
        save.mutate({
          kind,
          base_url: trimmedBaseUrl,
          api_key_env: trimmedEnv,
          api_key: apiKey.trim() === "" ? null : apiKey,
          // `null` = "no per-provider enabled_models override; keep the
          // built-in model catalog". The settings UI doesn't manage
          // this field today; ensure it's always sent so the server's
          // `deny_unknown_fields` accepts the request and existing
          // operator config isn't accidentally overwritten with an
          // empty allowlist.
          enabled_models: null,
        });
      }}
      className="px-4 space-y-3"
    >
      <div className="grid grid-cols-1 md:grid-cols-[150px_1fr] gap-3">
        <div>
          <div className="text-[12px] text-text-2 mb-1 block">Kind</div>
          <SignalSelectMenu
            ariaLabel="Kind"
            value={kind}
            options={[
              { value: "anthropic", label: "anthropic" },
              { value: "openai-compat", label: "openai-compat" },
              { value: "local-candle", label: "local-candle" },
              { value: "ollama", label: "ollama" },
              { value: "vllm", label: "vllm" },
            ]}
            onChange={setKind}
            className="w-full justify-between"
          />
        </div>
        <Field label="Base URL">
          <input
            value={baseUrl}
            onChange={(e) => setBaseUrl(e.target.value)}
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
          />
        </Field>
      </div>
      <Field label="New API key" hint="Optional. Leave blank to keep the stored key.">
        <input
          type="password"
          autoComplete="off"
          spellCheck={false}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>
      {errors.length > 0 ? (
        <ul className="m-0 pl-4 text-[12px] text-danger list-disc">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      ) : null}
      <div className="flex items-center gap-2">
        <button
          type="submit"
          disabled={!submittable || save.isPending}
          className="px-3 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 motion-safe:active:scale-[0.96]"
        >
          {save.isPending ? "Saving…" : "Save changes"}
        </button>
        <button
          type="button"
          onClick={onClose}
          className="text-[12px] text-text-3 hover:text-text"
        >
          Cancel
        </button>
        {save.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(save.error)}
          </span>
        ) : null}
      </div>
    </form>
  );
}

function ModelManager({
  row,
  onClose,
}: {
  row: ProviderRow;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const models = useQuery({
    queryKey: settingsKeys.providerModels(row.name),
    queryFn: () => listProviderModels(row.name),
    staleTime: 5 * 60 * 1000,
  });

  const [filter, setFilter] = useState("");
  // Working set — toggling checkboxes mutates this, "Save" flushes to the
  // server. Initialized from the persisted enabled_models so reopening
  // the manager shows the prior state.
  const [selected, setSelected] = useState<Set<string>>(
    () => new Set(row.enabled_models),
  );
  // Reset working set when the row's persisted state changes underneath us
  // (eg. another tab saved a different selection).
  const persistedKey = row.enabled_models.join(",");
  useMemo(() => {
    setSelected(new Set(row.enabled_models));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [persistedKey]);

  const save = useMutation({
    mutationFn: (ids: string[]) => setEnabledModels(row.name, ids),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: settingsKeys.providers() });
      onClose();
    },
  });

  const list = models.data?.models ?? [];
  const filtered = filter.trim()
    ? list.filter((m) =>
        modelSearchHaystack(m).includes(filter.trim().toLowerCase()),
      )
    : list;
  // Partition by the persisted enabled set (not the local checkbox working
  // set) so toggling a checkbox doesn't make rows jump under the cursor.
  const persistedEnabled = useMemo(
    () => new Set(row.enabled_models),
    [persistedKey],
  );
  const enabledRows = filtered.filter((m) => persistedEnabled.has(m.id));
  const otherRows = filtered.filter((m) => !persistedEnabled.has(m.id));
  const selectedCount = selected.size;
  const dirty = setsDiffer(selected, new Set(row.enabled_models));

  return (
    <div className="px-4 space-y-3">
      <div className="flex items-center justify-between gap-3">
        <div className="text-[13px] text-text-2">
          Manage <code className="font-mono text-text">{row.name}</code> models
          <span className="text-text-3 ml-2">
            ({selectedCount} selected
            {list.length ? ` of ${list.length}` : ""})
          </span>
        </div>
        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={() => models.refetch()}
            disabled={models.isFetching}
            className="text-[12px] px-2 py-1 rounded border border-border text-text-2 hover:text-text disabled:opacity-50"
          >
            {models.isFetching ? "Refreshing…" : "Refresh"}
          </button>
          <button
            type="button"
            onClick={onClose}
            className="text-[12px] px-2 py-1 rounded border border-border text-text-2 hover:text-text"
          >
            Close
          </button>
        </div>
      </div>

      {models.isError ? (
        <div className="text-[12px] text-danger">
          {errorMessage(models.error)}
        </div>
      ) : null}

      <input
        type="text"
        value={filter}
        onChange={(e) => setFilter(e.target.value)}
        placeholder={
          list.length > 20
            ? `Filter ${list.length} models — try "claude", "70b", "free"`
            : "Filter…"
        }
        className="w-full bg-surface-elev border border-border rounded px-3 py-1.5 text-[13px] text-text font-mono"
      />

      {models.isPending ? (
        <div className="text-[12px] text-text-3 py-4 text-center">
          Loading model catalog…
        </div>
      ) : list.length === 0 ? (
        <div className="text-[12px] text-text-3 py-4 text-center">
          Upstream returned no models.
        </div>
      ) : (
        <div className="scrollbar-stable max-h-[300px] border border-border-soft rounded">
          <table className="w-full">
            <tbody>
              {enabledRows.length > 0 ? (
                <>
                  <SectionHeading
                    label={`Selected (${enabledRows.length})`}
                  />
                  {enabledRows.map((m) => (
                    <ModelRow
                      key={m.id}
                      model={m}
                      checked={selected.has(m.id)}
                      onToggle={(on) => {
                        setSelected((prev) => {
                          const next = new Set(prev);
                          if (on) next.add(m.id);
                          else next.delete(m.id);
                          return next;
                        });
                      }}
                    />
                  ))}
                  {otherRows.length > 0 ? (
                    <SectionHeading label="All models" />
                  ) : null}
                </>
              ) : null}
              {otherRows.map((m) => (
                <ModelRow
                  key={m.id}
                  model={m}
                  checked={selected.has(m.id)}
                  onToggle={(on) => {
                    setSelected((prev) => {
                      const next = new Set(prev);
                      if (on) next.add(m.id);
                      else next.delete(m.id);
                      return next;
                    });
                  }}
                />
              ))}
              {filtered.length === 0 && filter.trim() ? (
                <tr>
                  <td className="py-2 px-2 text-[12px] text-text-3">
                    No match for "{filter.trim()}"
                  </td>
                </tr>
              ) : null}
            </tbody>
          </table>
        </div>
      )}

      <div className="flex items-center gap-2 pt-1">
        <button
          type="button"
          disabled={!dirty || save.isPending}
          onClick={() => save.mutate(Array.from(selected))}
          className="px-3 py-1.5 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 motion-safe:active:scale-[0.96]"
        >
          {save.isPending ? "Saving…" : "Save selection"}
        </button>
        <button
          type="button"
          onClick={() => setSelected(new Set(row.enabled_models))}
          disabled={!dirty || save.isPending}
          className="text-[12px] text-text-3 hover:text-text disabled:opacity-30"
        >
          Reset
        </button>
        {save.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(save.error)}
          </span>
        ) : null}
      </div>
    </div>
  );
}

function SectionHeading({ label }: { label: string }) {
  return (
    <tr className="bg-surface-elev/40 first:border-t-0">
      <td
        colSpan={3}
        className="py-1 px-3 text-[10px] uppercase tracking-wider text-text-3 font-medium"
      >
        {label}
      </td>
    </tr>
  );
}

function ModelRow({
  model,
  checked,
  onToggle,
}: {
  model: ProviderModelEntry;
  checked: boolean;
  onToggle: (on: boolean) => void;
}) {
  return (
    <tr
      className="border-t border-border-soft/40 first:border-t-0 hover:bg-surface-elev/40 cursor-pointer"
      onClick={() => onToggle(!checked)}
    >
      <td className="py-1.5 pl-3 pr-2 w-6">
        <input
          type="checkbox"
          checked={checked}
          onChange={(e) => onToggle(e.target.checked)}
          onClick={(e) => e.stopPropagation()}
        />
      </td>
      <td className="py-1.5 pr-2">
        <code className="font-mono text-[12px] text-text">{model.id}</code>
        {model.display_name && model.display_name !== model.id ? (
          <span className="ml-2 text-[11px] text-text-3">
            {model.display_name}
          </span>
        ) : null}
      </td>
      <td className="py-1.5 pr-3 text-right text-[11px] text-text-3 whitespace-nowrap">
        {model.owned_by ?? ""}
        {model.context_length
          ? `${model.owned_by ? " · " : ""}${formatContext(model.context_length)}`
          : ""}
      </td>
    </tr>
  );
}

function formatContext(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M ctx`;
  if (n >= 1_000) return `${Math.round(n / 1_000)}K ctx`;
  return `${n} ctx`;
}

function modelSearchHaystack(m: ProviderModelEntry): string {
  return [m.id, m.display_name ?? "", m.owned_by ?? ""]
    .join(" ")
    .toLowerCase();
}

function setsDiffer<T>(a: Set<T>, b: Set<T>): boolean {
  if (a.size !== b.size) return true;
  for (const x of a) if (!b.has(x)) return true;
  return false;
}

function AddProviderForm({
  allowCancel = true,
  onCancel,
  onAdded,
}: {
  allowCancel?: boolean;
  onCancel: () => void;
  onAdded: () => void;
}) {
  const [kindOption, setKindOption] = useState<string>(KIND_OPTIONS[0].value);
  const [name, setName] = useState<string>(KIND_OPTIONS[0].defaultName);
  const [baseUrl, setBaseUrl] = useState<string>(KIND_OPTIONS[0].defaultBaseUrl);
  const [apiKey, setApiKey] = useState("");

  const meta = KIND_OPTIONS.find((k) => k.value === kindOption) ?? KIND_OPTIONS[0];

  const add = useMutation({
    mutationFn: (req: AddProviderRequest) => addProvider(req),
    onSuccess: () => onAdded(),
  });

  const trimmedName = name.trim();
  const trimmedBaseUrl = baseUrl.trim();
  const trimmedKey = apiKey.trim();
  const needsKey = keyRequired(meta, trimmedBaseUrl);
  const errors: string[] = [];
  if (trimmedName === "") {
    errors.push("name is required");
  } else if (!/^[a-z0-9-]+$/.test(trimmedName)) {
    // Mirror the server rule (xvision-core validate_provider_name_str) so an
    // invalid name is caught inline instead of failing on save. Only the
    // "Custom" preset lets the user type a name; presets supply valid slugs.
    errors.push("name must use only lowercase letters, digits, and hyphens");
  } else if (trimmedName.length > 32) {
    errors.push("name must be 32 characters or fewer");
  }
  if (trimmedBaseUrl === "") {
    errors.push("base URL is required");
  } else if (!isHttpUrl(trimmedBaseUrl)) {
    errors.push("base URL must start with http:// or https://");
  }
  if (needsKey && trimmedKey === "") errors.push("API key is required");
  const submittable = errors.length === 0;

  return (
    <form
      onSubmit={(e) => {
        e.preventDefault();
        if (!submittable) return;
        logInfo("settings", "settings.provider.form.submit", {
          provider: trimmedName,
          kind: meta.wireKind,
          base_url_host: safeUrlHost(trimmedBaseUrl),
          api_key_set: trimmedKey !== "",
        });
        add.mutate({
          name: trimmedName,
          kind: meta.wireKind,
          base_url: trimmedBaseUrl,
          api_key_env: "",
          api_key: trimmedKey === "" ? null : apiKey,
        });
      }}
      className="border border-border-soft rounded-md p-4 mb-4 bg-surface-elev/30 space-y-3"
    >
      <div className="flex items-center justify-between">
        <span className="text-[13px] text-text">New provider</span>
        {allowCancel ? (
          <button
            type="button"
            onClick={onCancel}
            className="px-2 py-1 rounded text-[12px] border border-border text-text-3 hover:text-text hover:border-text-3 transition-colors"
          >
            Cancel
          </button>
        ) : null}
      </div>

      <div>
        <div className="text-[12px] text-text-2 mb-1 block">Provider</div>
        <SignalSelectMenu
          ariaLabel="Provider"
          value={kindOption}
          options={KIND_OPTIONS}
          onChange={(value) => {
            const next = KIND_OPTIONS.find((k) => k.value === value);
            if (!next) return;
            setKindOption(next.value);
            setName(next.defaultName);
            setBaseUrl(next.defaultBaseUrl);
          }}
          className="w-full justify-between"
          minWidth={240}
        />
      </div>

      <Field
        label={needsKey ? "API key (required)" : "API key"}
        hint={meta.keyHelp}
      >
        <input
          type="password"
          autoFocus
          autoComplete="off"
          spellCheck={false}
          value={apiKey}
          onChange={(e) => setApiKey(e.target.value)}
          placeholder="paste key here"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
          required={needsKey}
        />
      </Field>

      {meta.isCustom ? (
        <Field
          label="Name"
          hint="lowercase, digits, hyphens. e.g. ollama, together."
        >
          <input
            value={name}
            onChange={(e) => setName(e.target.value)}
            placeholder="e.g. ollama"
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
          />
        </Field>
      ) : null}

      <Field
        label="Base URL"
        hint={
          meta.isCustom
            ? "The /v1 endpoint root."
            : "Pre-filled for this provider — override only for proxies or self-hosted gateways."
        }
      >
        <input
          value={baseUrl}
          onChange={(e) => setBaseUrl(e.target.value)}
          placeholder="https://api.example.com/v1"
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
        />
      </Field>

      {errors.length > 0 ? (
        <ul className="m-0 pl-4 text-[12px] text-danger list-disc">
          {errors.map((e) => (
            <li key={e}>{e}</li>
          ))}
        </ul>
      ) : null}
      <div className="flex items-center gap-3 pt-1">
        <button
          type="submit"
          disabled={!submittable || add.isPending}
          title={submittable ? "" : errors.join("; ")}
          className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft disabled:opacity-40 disabled:hover:bg-gold transition-colors motion-safe:active:scale-[0.96]"
        >
          {add.isPending ? "Saving…" : "Save provider"}
        </button>
        {add.isError ? (
          <span className="text-[12px] text-danger">
            {errorMessage(add.error)}
          </span>
        ) : null}
      </div>
    </form>
  );
}

function Field({
  label,
  hint,
  children,
}: {
  label: string;
  hint?: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="text-[12px] text-text-2 mb-1 block">{label}</span>
      {children}
      {hint ? (
        <span className="text-[11px] text-text-3 mt-1 block">{hint}</span>
      ) : null}
    </label>
  );
}

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return `${err.code}: ${err.message}`;
  if (err instanceof Error) return err.message;
  return String(err);
}
