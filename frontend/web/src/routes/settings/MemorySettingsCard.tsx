import { useEffect, useState } from "react";
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

// Built-in embedder sources. Provider rows are appended after these.
const BUILTIN_EMBEDDER_OPTIONS: { value: string; label: string }[] = [
  { value: "off", label: "Off" },
  { value: "local", label: "Local (offline, lexical)" },
  { value: "auto", label: "Auto (best available)" },
  { value: "custom", label: "Custom endpoint (OpenAI-compatible)" },
];

// Sentinel option that reveals the free-text custom-model input.
const CUSTOM_MODEL_SENTINEL = "__custom__";

// Curated embedding-model names. These are model ids your provider serves
// (e.g. `ollama pull nomic-embed-text`); Local/Off ignore the model. The
// empty value means "provider default" (clears back to the resolver
// default).
const CURATED_EMBEDDING_MODELS: { value: string; label: string }[] = [
  { value: "", label: "Provider default" },
  { value: "nomic-embed-text", label: "nomic-embed-text (Ollama)" },
  { value: "mxbai-embed-large", label: "mxbai-embed-large (Ollama)" },
  { value: "bge-m3", label: "bge-m3 (Ollama)" },
  { value: "snowflake-arctic-embed2", label: "snowflake-arctic-embed2 (Ollama)" },
  { value: "qwen3-embedding", label: "qwen3-embedding (Ollama)" },
  { value: "all-minilm", label: "all-minilm (Ollama)" },
];

/**
 * Settings → General → Memory card.
 *
 * Operator surface for the Cortex memory layer: the embedder source
 * (off/local/auto/<provider>) plus the two workspace memory toggles
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
    ...providers.map((p) => ({
      value: p.name,
      label: `${p.name} (provider)`,
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
  // Whether the persisted model is a curated value; if not (and non-empty),
  // the picker starts in Custom mode showing the value in the text input.
  const isCuratedModel = CURATED_EMBEDDING_MODELS.some(
    (m) => m.value === persistedModel,
  );
  // Custom-input mode: revealed by selecting "Custom…", or implied when the
  // persisted model is a non-curated value.
  const [customMode, setCustomMode] = useState(false);
  const [customDraft, setCustomDraft] = useState("");

  // Sync local custom state when the persisted settings load/change.
  useEffect(() => {
    if (persistedModel && !isCuratedModel) {
      setCustomMode(true);
      setCustomDraft(persistedModel);
    }
  }, [persistedModel, isCuratedModel]);

  // The model picker is hidden for the "off" source (no embeddings at all).
  const showModelPicker = (settings?.embedder ?? "off") !== "off";

  // Custom-endpoint base URL: shown only when the source is "custom". Bound to
  // a local draft so the operator can type without a request per keystroke;
  // persisted on blur / Enter.
  const isCustomSource = (settings?.embedder ?? "off") === "custom";
  const persistedBaseUrl = settings?.embedder_base_url ?? "";
  const [baseUrlDraft, setBaseUrlDraft] = useState("");
  // Sync the draft from the persisted value when settings load/change.
  useEffect(() => {
    setBaseUrlDraft(persistedBaseUrl);
  }, [persistedBaseUrl]);
  // Soft, non-blocking hint when the URL doesn't include the `/v1` suffix.
  const trimmedBaseUrl = baseUrlDraft.trim();
  const missingV1 =
    trimmedBaseUrl.length > 0 &&
    !trimmedBaseUrl.replace(/\/+$/, "").endsWith("/v1");

  function submitBaseUrl(value: string) {
    mutation.mutate({
      embedder: "custom",
      embedder_base_url: value.trim(),
      chat_enabled: null,
      optimizer_enabled: null,
      embedder_model: null,
    });
  }

  // The <select> value: the curated value, the custom sentinel, or empty.
  const modelSelectValue = customMode
    ? CUSTOM_MODEL_SENTINEL
    : isCuratedModel
      ? persistedModel
      : "";

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
    if (value === CUSTOM_MODEL_SENTINEL) {
      setCustomMode(true);
      setCustomDraft(persistedModel && !isCuratedModel ? persistedModel : "");
      return;
    }
    setCustomMode(false);
    // Curated value (or "" for provider default) → persist immediately.
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
            observations. Pick where embeddings come from and which surfaces
            use memory.
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
        <label
          htmlFor="memory-embedder"
          className="block text-[13px] font-medium text-text-2"
        >
          Embedder source
        </label>
        <select
          id="memory-embedder"
          aria-label="Embedder source"
          className="w-full max-w-sm rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text disabled:opacity-60"
          value={settings?.embedder ?? "off"}
          disabled={busy}
          onChange={(e) => onEmbedderChange(e.target.value)}
        >
          {embedderOptions.map((opt) => (
            <option key={opt.value} value={opt.value}>
              {opt.label}
            </option>
          ))}
        </select>
      </div>

      {/* Custom endpoint base URL — only for the "custom" source. */}
      {isCustomSource ? (
        <div className="mt-4 space-y-1.5">
          <label
            htmlFor="memory-embedder-base-url"
            className="block text-[13px] font-medium text-text-2"
          >
            Custom endpoint base URL
          </label>
          <input
            id="memory-embedder-base-url"
            aria-label="Custom endpoint base URL"
            type="text"
            inputMode="url"
            placeholder="http://localhost:11434/v1"
            className="w-full max-w-sm rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text disabled:opacity-60"
            value={baseUrlDraft}
            disabled={busy}
            onChange={(e) => setBaseUrlDraft(e.target.value)}
            onBlur={() => submitBaseUrl(baseUrlDraft)}
            onKeyDown={(e) => {
              if (e.key === "Enter") {
                e.preventDefault();
                submitBaseUrl(baseUrlDraft);
              }
            }}
          />
          {missingV1 ? (
            <small className="block text-[11px] leading-snug text-amber-600 dark:text-amber-400">
              Most OpenAI-compatible servers expect the path to end in{" "}
              <code>/v1</code> (e.g. <code>http://localhost:11434/v1</code>).
            </small>
          ) : null}
          <small className="block text-[11px] leading-snug text-text-3">
            Point at any OpenAI-compatible /v1 endpoint — Ollama, llama.cpp, LM
            Studio, vLLM. Include <code>/v1</code> (e.g.{" "}
            <code>http://localhost:11434/v1</code>). No-auth only; for
            authenticated endpoints add a provider in the Providers tab.
          </small>
        </div>
      ) : null}

      {/* Embedding model — only relevant when a real embedder is in use. */}
      {showModelPicker ? (
        <div className="mt-4 space-y-1.5">
          <label
            htmlFor="memory-embedder-model"
            className="block text-[13px] font-medium text-text-2"
          >
            Embedding model
          </label>
          <select
            id="memory-embedder-model"
            aria-label="Embedding model"
            className="w-full max-w-sm rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text disabled:opacity-60"
            value={modelSelectValue}
            disabled={busy}
            onChange={(e) => onModelSelectChange(e.target.value)}
          >
            {CURATED_EMBEDDING_MODELS.map((opt) => (
              <option key={opt.value} value={opt.value}>
                {opt.label}
              </option>
            ))}
            <option value={CUSTOM_MODEL_SENTINEL}>Custom…</option>
          </select>
          {customMode ? (
            <input
              id="memory-embedder-model-custom"
              aria-label="Custom embedding model"
              type="text"
              placeholder="e.g. nomic-embed-text"
              className="mt-1.5 w-full max-w-sm rounded border border-border bg-surface-elev px-2 py-1.5 text-[13px] text-text disabled:opacity-60"
              value={customDraft}
              disabled={busy}
              onChange={(e) => setCustomDraft(e.target.value)}
              onBlur={() => submitModel(customDraft)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  submitModel(customDraft);
                }
              }}
            />
          ) : null}
          <small className="block text-[11px] leading-snug text-text-3">
            The model name your provider serves (e.g.{" "}
            <code>ollama pull nomic-embed-text</code>). Local and Off ignore
            this. Don&apos;t switch models mid-corpus — the embeddings live in
            separate spaces.
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
        Memory is best-effort. Off uses no memory; Local is offline/lexical;
        Auto prefers a configured provider and falls back to local.
        Strategy-agent (per-slot) memory is set on each agent.{" "}
        <span className="text-text-2">
          Settings apply after the next dashboard restart.
        </span>
      </small>
    </Card>
  );
}
