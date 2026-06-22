import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { Card } from "@/components/primitives/Card";
import {
  getMemorySettings,
  getMemoryStatus,
  listProviders,
  settingsKeys,
  updateMemorySettings,
} from "@/api/settings";
import type { UpdateMemoryRequest } from "@/api/types.gen";
import { SignalSelectMenu } from "@/components/primitives/SignalMenu";

// Built-in embedder sources. Provider rows are appended after these.
const BUILTIN_EMBEDDER_OPTIONS: { value: string; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "auto", label: "Auto (OpenAI embeddings)" },
];

// Curated OpenAI embedding-model names. The empty value means "provider
// default" (clears back to the resolver default).
const CURATED_EMBEDDING_MODELS: { value: string; label: string }[] = [
  { value: "", label: "Provider default (OpenAI)" },
  { value: "text-embedding-3-small", label: "text-embedding-3-small" },
  { value: "text-embedding-3-large", label: "text-embedding-3-large" },
  { value: "text-embedding-ada-002", label: "text-embedding-ada-002" },
];

/**
 * Settings → General → Memory card.
 *
 * Operator surface for the Cortex memory layer: the embedder source
 * (off/auto/<OpenAI provider>) plus the two workspace memory toggles
 * (chat + optimizer), with a read-only resolved-status line beneath.
 *
 * Backend reads its memory config as a startup snapshot, so changes here
 * persist immediately but only take effect after the next dashboard
 * restart — surfaced honestly in the help copy below.
 */
export function MemorySettingsCard() {
  const queryClient = useQueryClient();

  const settingsQuery = useQuery({
    queryKey: settingsKeys.memory(),
    queryFn: getMemorySettings,
  });
  const statusQuery = useQuery({
    queryKey: settingsKeys.memoryStatus(),
    queryFn: getMemoryStatus,
  });
  const providersQuery = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });

  const mutation = useMutation({
    mutationFn: (req: UpdateMemoryRequest) => updateMemorySettings(req),
    onSuccess: (report) => {
      queryClient.setQueryData(settingsKeys.memory(), report);
      void queryClient.invalidateQueries({ queryKey: settingsKeys.memory() });
      void queryClient.invalidateQueries({
        queryKey: settingsKeys.memoryStatus(),
      });
    },
  });

  const settings = settingsQuery.data;
  const status = statusQuery.data;
  const providers = providersQuery.data?.providers ?? [];

  const embedderOptions = [
    ...BUILTIN_EMBEDDER_OPTIONS,
    ...providers.filter(isOpenAiMemoryProvider).map((p) => ({
      value: p.name,
      label: `${p.name} (OpenAI)`,
    })),
  ];

  // Total live observations across namespaces. `live_observations` is a
  // ts-rs `bigint` (u64) in the generated types, but JSON.parse delivers a
  // plain number at runtime — mixing it with a 0n accumulator throws
  // "Cannot mix BigInt and other types" and crashes the whole settings page.
  const totalLiveObservations =
    status?.namespaces.reduce((acc, n) => acc + Number(n.live_observations), 0) ?? 0;

  const busy = mutation.isPending || settingsQuery.isLoading;

  // The persisted model id (or empty string for "provider default").
  const persistedModel = settings?.embedder_model ?? "";
  // Whether the persisted model is a curated OpenAI value; unknown values fall
  // back to provider default in the select.
  const isCuratedModel = CURATED_EMBEDDING_MODELS.some(
    (m) => m.value === persistedModel,
  );

  // The model picker is hidden for the "off" source (no embeddings at all).
  const showModelPicker = (settings?.embedder ?? "off") !== "off";

  // The menu value: the curated value or provider default.
  const modelSelectValue = isCuratedModel ? persistedModel : "";

  function onEmbedderChange(value: string) {
    mutation.mutate({
      embedder: value,
      chat_enabled: null,
      optimizer_enabled: null,
      embedder_model: null,
      embedder_base_url: null,
    });
  }

  function submitModel(value: string) {
    mutation.mutate({
      embedder: null,
      chat_enabled: null,
      optimizer_enabled: null,
      embedder_model: value,
      embedder_base_url: null,
    });
  }

  function onModelSelectChange(value: string) {
    // Curated OpenAI value (or "" for provider default) persists immediately.
    submitModel(value);
  }

  function onChatToggle(enabled: boolean) {
    mutation.mutate({
      embedder: null,
      chat_enabled: enabled,
      optimizer_enabled: null,
      embedder_model: null,
      embedder_base_url: null,
    });
  }

  function onOptimizerToggle(enabled: boolean) {
    mutation.mutate({
      embedder: null,
      chat_enabled: null,
      optimizer_enabled: enabled,
      embedder_model: null,
      embedder_base_url: null,
    });
  }

  return (
    <Card className="p-5">
      <div className="mb-4 flex flex-wrap items-start justify-between gap-3">
        <div>
          <h3 className="m-0 font-sans font-semibold text-[18px] tracking-tight">
            Memory
          </h3>
          <p className="m-0 mt-1 max-w-2xl text-[12px] leading-snug text-text-3">
            Cortex memory lets the chat rail and optimizer recall prior
            observations. Memory currently uses OpenAI embedding providers only;
            pick the OpenAI source and which surfaces use memory.
          </p>
        </div>
        {settings ? (
          <span
            className={[
              "rounded px-2 py-[2px] text-[11px] font-medium",
              settings.persisted
                ? "bg-gold/[0.12] text-gold dark:bg-gold/[0.16]"
                : "bg-surface-elev text-text-3 border border-border",
            ].join(" ")}
          >
            {settings.persisted ? "Custom" : "Default"}
          </span>
        ) : null}
      </div>

      {/* Embedder source */}
      <div className="space-y-1.5">
        <div className="block text-[13px] font-medium text-text-2">
          Embedder source
        </div>
        <SignalSelectMenu
          ariaLabel="Embedder source"
          value={settings?.embedder ?? "off"}
          options={embedderOptions}
          disabled={busy}
          onChange={onEmbedderChange}
          className="w-full max-w-sm justify-between"
          minWidth={240}
        />
        <small className="block max-w-2xl text-[11px] leading-snug text-text-3">
          Add or edit the OpenAI provider in Settings {">"} Providers.
          Non-OpenAI providers are not shown here because memory embeddings are
          not supported for them.
        </small>
      </div>

      {/* Embedding model — only relevant when a real embedder is in use. */}
      {showModelPicker ? (
        <div className="mt-4 space-y-1.5">
          <div className="block text-[13px] font-medium text-text-2">
            Embedding model
          </div>
          <SignalSelectMenu
            ariaLabel="Embedding model"
            value={modelSelectValue}
            options={CURATED_EMBEDDING_MODELS}
            disabled={busy}
            onChange={onModelSelectChange}
            className="w-full max-w-sm justify-between"
            minWidth={240}
          />
          <small className="block text-[11px] leading-snug text-text-3">
            Use the OpenAI embedding model configured for your provider. Off
            ignores this. Don&apos;t switch models mid-corpus — the embeddings
            live in separate spaces.
          </small>
        </div>
      ) : null}

      {/* Toggles */}
      <div className="mt-4 grid gap-2 sm:grid-cols-2">
        <label className="flex items-center gap-2 rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text-2">
          <input
            type="checkbox"
            aria-label="Chat memory"
            checked={settings?.chat_enabled ?? false}
            disabled={busy}
            onChange={(e) => onChatToggle(e.target.checked)}
          />
          <span>Chat memory</span>
        </label>
        <label className="flex items-center gap-2 rounded border border-border bg-surface-elev px-3 py-2 text-[13px] text-text-2">
          <input
            type="checkbox"
            aria-label="Optimizer memory"
            checked={settings?.optimizer_enabled ?? false}
            disabled={busy}
            onChange={(e) => onOptimizerToggle(e.target.checked)}
          />
          <span>Optimizer memory</span>
        </label>
      </div>

      {/* Read-only resolved status */}
      <div className="mt-4 rounded border border-border bg-surface-elev px-3 py-2 text-[12px] leading-relaxed text-text-3">
        {status ? (
          <ul className="m-0 list-none space-y-0.5 p-0">
            <li>
              {status.embedder_present && status.embedder_id ? (
                <>
                  Embedder:{" "}
                  <span className="text-text-2">{status.embedder_id}</span>
                  {status.embedder_source ? (
                    <span> ({status.embedder_source})</span>
                  ) : null}
                </>
              ) : (
                <span>No embedder — recall disabled</span>
              )}
            </li>
            <li>
              Store: {status.writable ? "writable" : "read-only"} ·{" "}
              {status.grace_days}-day forget grace
            </li>
            <li>
              Live observations:{" "}
              <span className="text-text-2">
                {Number(totalLiveObservations)}
              </span>{" "}
              across {status.namespaces.length} namespace
              {status.namespaces.length === 1 ? "" : "s"}
            </li>
          </ul>
        ) : statusQuery.isError ? (
          <span>Status unavailable.</span>
        ) : (
          <span>Loading status…</span>
        )}
      </div>

      {mutation.isError ? (
        <p
          role="alert"
          className="m-0 mt-2 text-[12px] text-rose-600 dark:text-rose-400"
        >
          Could not save memory settings. Try again.
        </p>
      ) : null}

      <small className="mt-3 block text-[11px] leading-snug text-text-3">
        Memory is best-effort. Off uses no memory; Auto uses the configured
        OpenAI embedding provider when available. Strategy-agent (per-slot)
        memory is set on each agent.{" "}
        <span className="text-text-2">
          Settings apply after the next dashboard restart.
        </span>
      </small>
    </Card>
  );
}

function isOpenAiMemoryProvider(provider: {
  name: string;
  base_url?: string | null;
}) {
  const name = provider.name.toLowerCase();
  const baseUrl = (provider.base_url ?? "").toLowerCase();
  return name === "openai" || baseUrl.includes("api.openai.com");
}
