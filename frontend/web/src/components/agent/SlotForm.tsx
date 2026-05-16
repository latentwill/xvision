// SlotForm — editor for a single AgentSlot. Renders an expandable card
// with provider/model picker, system prompt textarea, max_tokens, skills
// placeholder (no skill registry yet — hidden when empty per v1 plan).

import { useQuery } from "@tanstack/react-query";
import type { AgentSlot } from "@/api/agents";
import {
  getProviderCatalog,
  listProviders,
  settingsKeys,
} from "@/api/settings";
import type { ModelEntry } from "@/api/types.gen";
import { ModelPicker } from "@/components/ModelPicker";
import { Icon } from "@/components/primitives/Icon";
import {
  autoMaxTokens,
  hasModelMetadata,
  isReasoning,
  lookupModel,
} from "./modelMetadata";

export function SlotForm({
  slot,
  onChange,
  onRemove,
  onDuplicate,
  canRemove,
  index,
}: {
  slot: AgentSlot;
  onChange: (next: AgentSlot) => void;
  onRemove: () => void;
  onDuplicate: () => void;
  canRemove: boolean;
  index: number;
}) {
  const providersQ = useQuery({
    queryKey: settingsKeys.providers(),
    queryFn: listProviders,
  });
  const providerRows = providersQ.data?.providers ?? [];
  const providerNames =
    providerRows.map((p) => p.name) ?? [];
  const slotProviderRow = providerRows.find((p) => p.name === slot.provider);
  const slotProviderKind = slotProviderRow?.kind ?? null;

  function patch<K extends keyof AgentSlot>(key: K, value: AgentSlot[K]) {
    onChange({ ...slot, [key]: value });
  }

  return (
    <div className="bg-surface-panel border border-border rounded-card p-5 mb-3">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3 flex-1">
          <span className="text-text-3 font-mono text-[11px]">
            agent slot {index + 1}
          </span>
          <input
            type="text"
            value={slot.name}
            onChange={(e) => patch("name", e.target.value)}
            placeholder="agent slot name (e.g. main, trader, risk_check)"
            className="flex-1 bg-transparent border-0 border-b border-border-soft text-text font-mono text-[14px] focus:outline-none focus:border-gold/40 px-0 py-1"
          />
        </div>
        <div className="flex items-center gap-1.5">
          <button
            type="button"
            onClick={onDuplicate}
            title="Duplicate slot"
            className="p-1.5 text-text-3 hover:text-text rounded transition-colors"
          >
            <Icon name="plus" size={14} />
          </button>
          {canRemove ? (
            <button
              type="button"
              onClick={onRemove}
              title="Remove slot"
              className="p-1.5 text-text-3 hover:text-danger rounded transition-colors"
            >
              <Icon name="check" size={14} />
            </button>
          ) : null}
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
        <Field label="Provider">
          {providerNames.length > 0 ? (
            <select
              value={slot.provider}
              onChange={(e) => patch("provider", e.target.value)}
              className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
            >
              <option value="">— select provider —</option>
              {providerNames.map((p) => (
                <option key={p} value={p}>
                  {p}
                </option>
              ))}
            </select>
          ) : (
            <input
              type="text"
              value={slot.provider}
              onChange={(e) => patch("provider", e.target.value)}
              placeholder="provider name"
              className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
          )}
        </Field>

        <Field label="Model">
          {providerRows.length > 0 ? (
            <ModelPicker
              rows={providerRows}
              loading={providersQ.isPending}
              provider={slot.provider || null}
              model={slot.model}
              filterProvider={slot.provider || undefined}
              placeholder="— select model —"
              emptyHint="No enabled models for this provider"
              onChange={(provider, model) => {
                onChange({
                  ...slot,
                  provider: provider ?? slot.provider,
                  model,
                });
              }}
              className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
          ) : (
            <input
              type="text"
              value={slot.model}
              onChange={(e) => patch("model", e.target.value)}
              placeholder="e.g. claude-sonnet-4-6"
              className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
          )}
        </Field>
      </div>

      <Field label="System prompt">
        <textarea
          value={slot.system_prompt}
          onChange={(e) => patch("system_prompt", e.target.value)}
          placeholder="You are a trader. Make a decision based on the briefing..."
          rows={6}
          className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono leading-relaxed focus:outline-none focus:border-gold/40 resize-y"
        />
      </Field>

      <div className="grid grid-cols-2 gap-4 mt-4">
        <Field label="Max tokens">
          <MaxTokensInput
            slot={slot}
            onChange={onChange}
            providerKind={slotProviderKind}
          />
        </Field>
        {slot.skill_ids.length > 0 ? (
          <Field label="Skills">
            <div className="text-text-3 text-[12px] px-3 py-2">
              {slot.skill_ids.length} skill
              {slot.skill_ids.length === 1 ? "" : "s"} (manage at /agents/skills)
            </div>
          </Field>
        ) : null}
      </div>
    </div>
  );
}

function Field({
  label,
  children,
}: {
  label: string;
  children: React.ReactNode;
}) {
  return (
    <label className="block">
      <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
        {label}
      </span>
      {children}
    </label>
  );
}

// MaxTokensInput — renders the per-slot max_tokens override with an
// "Auto" pill when unset.
//
// Catalog-first resolution (PR #199):
//
//   1. Look up the model in the provider's catalog (the persisted
//      `/v1/models` response). If the entry carries an explicit
//      `max_output_tokens`, that's what we surface — the provider just
//      told us the ceiling, end of homework.
//
//   2. Catalog miss falls back to the editorial `modelMetadata.ts`
//      table — fine for the canonical models that still live there.
//
//   3. When BOTH catalog and editorial miss AND the provider's kind is
//      OpenAI-compat, the placeholder used to lie ("Auto: 4096"). It now
//      reads "Provider default" because the dispatcher omits `max_tokens`
//      on that path and lets the provider apply its own large default.
//      For Anthropic-kind providers (where the API requires the field),
//      we still show the editorial fallback number. A known OpenAI-compat
//      model that's merely missing from the catalog response keeps its
//      editorial number — only true editorial misses fall to "Provider
//      default".
//
// Operator values pass through verbatim — no client-side ceiling
// clamp; the dispatcher does the right thing per #195.
function MaxTokensInput({
  slot,
  onChange,
  providerKind,
}: {
  slot: AgentSlot;
  onChange: (next: AgentSlot) => void;
  providerKind: string | null;
}) {
  const catalogQ = useQuery({
    queryKey: settingsKeys.providerCatalog(slot.provider),
    queryFn: () => getProviderCatalog(slot.provider),
    // Only fire when the slot actually has a provider name to query;
    // empty during the brief moment after "+ New slot" before the
    // provider dropdown is touched. Treat 404 as a soft state.
    enabled: slot.provider.trim().length > 0,
    retry: false,
    staleTime: 5 * 60 * 1000,
  });
  const catalogEntry: ModelEntry | undefined = catalogQ.data?.models.find(
    (m) => m.id === slot.model,
  );

  const isUnset = slot.max_tokens == null;
  const editorialMeta = lookupModel(slot.model);
  const editorialKnown = hasModelMetadata(slot.model);
  const editorialAuto = autoMaxTokens(editorialMeta);
  const isOpenAiCompat = providerKind === "openai-compat";

  const catalogMax = catalogEntry?.max_output_tokens ?? null;
  const catalogCtx = catalogEntry?.context_window ?? null;
  const catalogReasoning = catalogEntry?.supports_reasoning ?? null;
  const editorialReasoning = isReasoning(editorialMeta);

  // The "Provider default" fallback only applies when BOTH the live
  // catalog and the editorial table miss the model. A known OpenAI-compat
  // model that's merely absent from the catalog response (e.g. provider
  // didn't list it on /v1/models, or hasn't been refreshed) still gets
  // its editorial auto number rather than the vaguer copy.
  const useProviderDefault =
    catalogMax === null && !editorialKnown && isOpenAiCompat;

  // Three resolution paths in order of trust:
  // 1. Catalog says exact ceiling: use it.
  // 2. Editorial table has this model: use its auto number.
  // 3. Catalog miss + editorial miss + openai-compat provider: show
  //    "Provider default" copy with no specific number, because the
  //    dispatcher will omit `max_tokens` and let the upstream provider
  //    pick its own default.
  const placeholder =
    catalogMax !== null
      ? `Auto: ${catalogMax.toLocaleString()}`
      : useProviderDefault
        ? "Provider default"
        : `Auto: ${editorialAuto.toLocaleString()}`;

  const pillLabel =
    catalogMax !== null
      ? "Auto from catalog"
      : useProviderDefault
        ? "Provider default"
        : "Auto from model";

  const pillTitle = catalogMax !== null
    ? buildCatalogTooltip({
        modelId: slot.model,
        ctx: catalogCtx,
        max: catalogMax,
        reasoning: catalogReasoning ?? editorialReasoning,
      })
    : useProviderDefault
      ? "Provider applies its own default — no client-side limit. " +
        "Click Refresh in Settings → Providers to fetch the model's actual ceiling."
      : editorialReasoning
        ? `Reasoning model — auto includes ${editorialMeta.reasoning_token_default} reasoning + ${editorialMeta.recommended_visible_output} visible (ceiling ${editorialMeta.output_token_ceiling}).`
        : `Standard model — auto is ${editorialMeta.recommended_visible_output} visible (ceiling ${editorialMeta.output_token_ceiling}).`;

  return (
    <div className="flex items-stretch gap-2">
      <input
        type="number"
        value={slot.max_tokens ?? ""}
        min={1}
        placeholder={placeholder}
        onChange={(e) => {
          const raw = e.target.value;
          if (raw === "") {
            onChange({ ...slot, max_tokens: null });
            return;
          }
          const parsed = parseInt(raw, 10);
          onChange({
            ...slot,
            max_tokens: Number.isFinite(parsed) && parsed > 0 ? parsed : null,
          });
        }}
        className="flex-1 px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
      />
      {isUnset ? (
        <span
          title={pillTitle}
          className="inline-flex items-center px-2 py-1 rounded-sm text-[11px] font-mono uppercase tracking-wide bg-surface-card border border-border-soft text-text-3"
        >
          {pillLabel}
        </span>
      ) : (
        <button
          type="button"
          onClick={() => onChange({ ...slot, max_tokens: null })}
          title="Clear override and resolve from the provider's catalog (or model metadata)."
          className="inline-flex items-center px-2 py-1 rounded-sm text-[11px] font-mono uppercase tracking-wide bg-surface-card border border-border-soft text-text-3 hover:text-text"
        >
          Reset
        </button>
      )}
    </div>
  );
}

function buildCatalogTooltip({
  modelId,
  ctx,
  max,
  reasoning,
}: {
  modelId: string;
  ctx: number | null;
  max: number;
  reasoning: boolean | null;
}): string {
  const parts: string[] = [`${modelId} (from provider catalog)`];
  if (ctx !== null) {
    parts.push(`context ${ctx.toLocaleString()}`);
  }
  parts.push(`max output ${max.toLocaleString()}`);
  if (reasoning) {
    parts.push("reasoning class");
  }
  return parts.join(" — ");
}
