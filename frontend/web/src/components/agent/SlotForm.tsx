// SlotForm — editor for a single AgentSlot. Renders an expandable card
// with provider/model picker, system prompt textarea, max_tokens, skills
// placeholder (no skill registry yet — hidden when empty per v1 plan).

import { useQuery } from "@tanstack/react-query";
import type { AgentSlot } from "@/api/agents";
import { listProviders, settingsKeys } from "@/api/settings";
import { ModelPicker } from "@/components/ModelPicker";
import { Icon } from "@/components/primitives/Icon";

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
          <input
            type="number"
            value={slot.max_tokens}
            min={1}
            onChange={(e) =>
              patch("max_tokens", Math.max(1, parseInt(e.target.value, 10) || 0))
            }
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
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
