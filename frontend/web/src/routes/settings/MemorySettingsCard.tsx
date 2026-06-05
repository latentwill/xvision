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
  // ts-rs `bigint` (u64); sum as BigInt then narrow for display.
  const totalLiveObservations =
    status?.namespaces.reduce((acc, n) => acc + n.live_observations, 0n) ?? 0n;

  const busy = mutation.isPending || settingsQuery.isLoading;

  function onEmbedderChange(value: string) {
    mutation.mutate({
      embedder: value,
      chat_enabled: null,
      optimizer_enabled: null,
    });
  }

  function onChatToggle(enabled: boolean) {
    mutation.mutate({
      embedder: null,
      chat_enabled: enabled,
      optimizer_enabled: null,
    });
  }

  function onOptimizerToggle(enabled: boolean) {
    mutation.mutate({
      embedder: null,
      chat_enabled: null,
      optimizer_enabled: enabled,
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
