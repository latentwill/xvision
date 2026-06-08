// Shared model-select primitive. Used by:
//   - ChatRail (model picker bar at top of the rail)
//   - Settings → Providers Default-LLM card (filtered to one provider)
//   - Strategy inspector slot editors (all providers, grouped)
//
// Both the `ModelPicker` and `ModelPickerDropdown` exports render the
// Signal-styled floating dropdown (filter + provider groups + context window).
// `ModelPicker` is kept as the call-site name; it forwards to
// `ModelPickerDropdown`. The legacy native <select> was retired so every model
// surface gets the same control.

import type { ProviderRow } from "@/api/types.gen";
import { isProviderConfigured } from "@/lib/providers";
import { SignalModelPickerMenu } from "@/components/primitives/SignalMenu";
import type { ModelOption } from "@/components/primitives/SignalMenu";

export type ProviderModelOption = { provider: string; model: string };

/** Call-site name for the model picker. Delegates to {@link ModelPickerDropdown}
 *  (the Signal-styled floating dropdown). Kept so the ~7 existing call sites
 *  need no import change. */
export function ModelPicker(props: {
  rows: ProviderRow[];
  loading: boolean;
  /** Selected provider name. Null when nothing is picked yet. */
  provider: string | null;
  /** Selected model id. Empty string when nothing is picked. */
  model: string;
  /** Fires with (provider, model). Both null/empty when the user clears
   *  the selection. */
  onChange: (provider: string | null, model: string) => void;
  /** Limit options to one provider. Useful when the caller already picks the
   *  provider via a separate control (e.g. the DefaultLlmCard). */
  filterProvider?: string;
  /** Layout/width classes for the trigger button (e.g. `w-full`,
   *  `flex-1 min-w-0`). */
  className?: string;
  ariaLabel?: string;
  /** Override the "no models available" message. */
  emptyHint?: string;
  /** Override the "— pick a model —" placeholder. */
  placeholder?: string;
  align?: "left" | "right";
}) {
  return <ModelPickerDropdown {...props} />;
}

// ─── ModelPickerDropdown ──────────────────────────────────────────────────────
// Signal-styled rich model picker (filter + provider groups + context window).

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
  className,
  ariaLabel,
  emptyHint,
}: {
  rows: ProviderRow[];
  loading: boolean;
  provider: string | null;
  model: string;
  onChange: (provider: string | null, model: string) => void;
  filterProvider?: string;
  align?: "left" | "right";
  placeholder?: string;
  className?: string;
  ariaLabel?: string;
  emptyHint?: string;
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
      className={className}
      ariaLabel={ariaLabel}
      emptyHint={emptyHint}
    />
  );
}
