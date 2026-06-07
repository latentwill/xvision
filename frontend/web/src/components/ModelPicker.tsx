// Shared model-select primitive. Used by:
//   - ChatRail (model picker bar at top of the rail)
//   - Settings → Providers Default-LLM card (filtered to one provider)
//   - Strategy inspector slot editors (all providers, grouped)
//
// Two variants:
//   ModelPicker — legacy native <select> for tight inline use
//   ModelPickerDropdown — Signal-styled floating dropdown with filter + context window

import type { ProviderRow } from "@/api/types.gen";
import { isProviderConfigured } from "@/lib/providers";
import { SignalModelPickerMenu } from "@/components/primitives/SignalMenu";
import type { ModelOption } from "@/components/primitives/SignalMenu";

export type ProviderModelOption = { provider: string; model: string };

export function ModelPicker({
  rows,
  loading,
  provider,
  model,
  onChange,
  filterProvider,
  className,
  ariaLabel,
  emptyHint,
  placeholder,
}: {
  rows: ProviderRow[];
  loading: boolean;
  /** Selected provider name. Null when nothing is picked yet. */
  provider: string | null;
  /** Selected model id. Empty string when nothing is picked. */
  model: string;
  /** Fires with (provider, model). Both null/empty when the user clears
   *  the selection. */
  onChange: (provider: string | null, model: string) => void;
  /** Limit options to one provider; renders a flat list without optgroup
   *  headers. Useful when the caller already picks the provider via a
   *  separate control (e.g. the DefaultLlmCard's provider dropdown). */
  filterProvider?: string;
  className?: string;
  ariaLabel?: string;
  /** Override the "no models" message. */
  emptyHint?: string;
  /** Override the "— pick a model —" placeholder. */
  placeholder?: string;
}) {
  const configuredProviders = rows.filter(isProviderConfigured);
  const all = configuredProviders
    .flatMap((r) =>
      r.enabled_models.map((m) => ({ provider: r.name, model: m })),
    );
  const options = filterProvider
    ? all.filter((o) => o.provider === filterProvider)
    : all;
  const value =
    provider &&
    model &&
    options.find((o) => o.provider === provider && o.model === model)
      ? `${provider}::${model}`
      : "";
  const handleChange = (raw: string) => {
    if (!raw) {
      onChange(null, "");
      return;
    }
    const [p, ...rest] = raw.split("::");
    onChange(p, rest.join("::"));
  };
  return (
    <select
      value={value}
      onChange={(e) => handleChange(e.target.value)}
      disabled={loading}
      aria-label={ariaLabel ?? "Model"}
      className={className}
    >
      {options.length === 0 ? (
        <option value="">
          {emptyHint ??
            (configuredProviders.length === 0
              ? "No provider configured"
              : "no models — visit Settings → Providers")}
        </option>
      ) : (
        <option value="">{placeholder ?? "— pick a model —"}</option>
      )}
      {filterProvider
        ? options.map((o) => (
            <option
              key={`${o.provider}::${o.model}`}
              value={`${o.provider}::${o.model}`}
            >
              {o.model}
            </option>
          ))
        : groupByProvider(options).map((g) => (
            <optgroup key={g.provider} label={g.provider}>
              {g.items.map((o) => (
                <option
                  key={`${o.provider}::${o.model}`}
                  value={`${o.provider}::${o.model}`}
                >
                  {o.model}
                </option>
              ))}
            </optgroup>
          ))}
    </select>
  );
}

// ─── ModelPickerDropdown ──────────────────────────────────────────────────────
// Signal-styled rich model picker. Replaces native <select> where space allows.

const CONTEXT_WINDOWS: Record<string, string> = {
  "claude-opus-4-8": "200K",
  "claude-opus-4-5": "200K",
  "claude-opus-4-1": "200K",
  "claude-sonnet-4-6": "200K",
  "claude-sonnet-4-5": "200K",
  "claude-haiku-4-5": "200K",
  "claude-haiku-4-5-20251001": "200K",
  "claude-3-5-sonnet-20241022": "200K",
  "claude-3-5-haiku-20241022": "200K",
  "claude-3-opus-20240229": "200K",
  "gpt-4o": "128K",
  "gpt-4o-mini": "128K",
  "gpt-4-turbo": "128K",
  "gpt-5": "256K",
  "gpt-5-mini": "128K",
  "gpt-4.1": "1M",
  "gpt-4.1-mini": "1M",
  "gpt-4.1-nano": "1M",
};

function contextWindowLabel(row: ProviderRow, model: string): string | undefined {
  if (CONTEXT_WINDOWS[model]) return CONTEXT_WINDOWS[model];
  if (["ollama", "llama-cpp", "vllm", "local-candle"].includes(row.kind)) return row.kind;
  return undefined;
}

export function ModelPickerDropdown({
  rows,
  loading,
  provider,
  model,
  onChange,
  filterProvider,
  align,
  placeholder,
}: {
  rows: ProviderRow[];
  loading: boolean;
  provider: string | null;
  model: string;
  onChange: (provider: string | null, model: string) => void;
  filterProvider?: string;
  align?: "left" | "right";
  placeholder?: string;
}) {
  const configuredProviders = rows.filter(isProviderConfigured);
  const options: ModelOption[] = configuredProviders
    .filter((r) => !filterProvider || r.name === filterProvider)
    .flatMap((r) =>
      r.enabled_models.map((m) => ({
        provider: r.name,
        model: m,
        contextWindow: contextWindowLabel(r, m),
      })),
    );

  return (
    <SignalModelPickerMenu
      options={options}
      provider={provider}
      model={model}
      onChange={onChange}
      loading={loading}
      align={align}
      placeholder={placeholder}
    />
  );
}

function groupByProvider(
  options: ProviderModelOption[],
): { provider: string; items: ProviderModelOption[] }[] {
  const map = new Map<string, ProviderModelOption[]>();
  for (const o of options) {
    const arr = map.get(o.provider) ?? [];
    arr.push(o);
    map.set(o.provider, arr);
  }
  return Array.from(map.entries()).map(([provider, items]) => ({
    provider,
    items,
  }));
}
