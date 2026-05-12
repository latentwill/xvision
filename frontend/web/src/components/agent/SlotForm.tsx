// SlotForm — editor for a single AgentSlot. Renders an expandable card
// with provider/model picker, system prompt textarea, max_tokens, and
// skill multi-select (loads from /settings/skills registry).

import { Link } from "react-router-dom";
import { useQuery } from "@tanstack/react-query";
import type { AgentSlot } from "@/api/agents";
import { listProviders, settingsKeys } from "@/api/settings";
import { listSkills, skillKeys } from "@/api/skills";
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
  const providerNames =
    providersQ.data?.providers.map((p) => p.name) ?? [];

  function patch<K extends keyof AgentSlot>(key: K, value: AgentSlot[K]) {
    onChange({ ...slot, [key]: value });
  }

  return (
    <div className="bg-surface-panel border border-border rounded-card p-5 mb-3">
      <div className="flex items-center justify-between mb-4">
        <div className="flex items-center gap-3 flex-1">
          <span className="text-text-3 font-mono text-[11px]">
            slot {index + 1}
          </span>
          <input
            type="text"
            value={slot.name}
            onChange={(e) => patch("name", e.target.value)}
            placeholder="slot name (e.g. main, trader, risk_check)"
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
          <input
            type="text"
            value={slot.model}
            onChange={(e) => patch("model", e.target.value)}
            placeholder="e.g. claude-sonnet-4-6"
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
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

      <Field label="Max tokens">
        <input
          type="number"
          value={slot.max_tokens}
          min={1}
          onChange={(e) =>
            patch("max_tokens", Math.max(1, parseInt(e.target.value, 10) || 0))
          }
          className="w-full md:w-1/2 px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
        />
      </Field>

      <div className="mt-4">
        <SkillPicker
          selectedIds={slot.skill_ids}
          onChange={(ids) => patch("skill_ids", ids)}
        />
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

function SkillPicker({
  selectedIds,
  onChange,
}: {
  selectedIds: string[];
  onChange: (ids: string[]) => void;
}) {
  const q = useQuery({
    queryKey: skillKeys.list(false),
    queryFn: () => listSkills(false),
  });

  function toggle(id: string) {
    if (selectedIds.includes(id)) {
      onChange(selectedIds.filter((x) => x !== id));
    } else {
      onChange([...selectedIds, id]);
    }
  }

  const skills = q.data ?? [];

  if (q.isPending) {
    return (
      <Field label="Skills">
        <div className="text-text-3 text-[12px] px-3 py-2">Loading…</div>
      </Field>
    );
  }

  if (skills.length === 0) {
    return (
      <Field label="Skills">
        <div className="text-text-3 text-[12.5px] px-3 py-2 leading-snug">
          No skills configured yet.{" "}
          <Link
            to="/settings/skills"
            className="text-gold hover:underline"
          >
            Add some at Settings → Skills
          </Link>
          .
        </div>
      </Field>
    );
  }

  return (
    <Field label="Skills">
      <div className="flex flex-wrap gap-1.5">
        {skills.map((s) => {
          const selected = selectedIds.includes(s.skill_id);
          return (
            <button
              key={s.skill_id}
              type="button"
              onClick={() => toggle(s.skill_id)}
              title={s.description || s.kind}
              className={[
                "inline-flex items-center gap-1.5 px-2.5 py-1 rounded-sm text-[12px] border transition-colors",
                selected
                  ? "bg-gold/15 border-gold/40 text-gold"
                  : "bg-surface-card border-border text-text-2 hover:text-text hover:border-border-strong",
              ].join(" ")}
            >
              <span className="font-mono">{s.name}</span>
              <span className="text-[10px] opacity-70">
                {s.kind.replace("_", " ")}
              </span>
              {selected ? <Icon name="check" size={11} /> : null}
            </button>
          );
        })}
      </div>
    </Field>
  );
}
