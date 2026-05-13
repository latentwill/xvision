// Shared model-select primitive. Used by:
//   - ChatRail (model picker bar at top of the rail)
//   - Settings → Providers Default-LLM card (filtered to one provider)
//   - Strategy inspector slot editors (all providers, grouped)
//
// The component is just the <select> element — wrappers, labels, and
// surrounding layout live at call sites so each context gets its own
// styling.

import type { ProviderRow } from "@/api/types.gen";

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
  const configuredProviders = rows.filter((r) => r.api_key_set && !r.synthetic);
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
