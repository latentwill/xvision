// SlotForm — editor for a single AgentSlot. Renders an expandable card
// with provider/model picker, system prompt textarea, and skills
// placeholder (no skill registry yet — hidden when empty per v1 plan).
//
// NB: the per-slot `max_tokens` operator override was removed (2026-05-17
// via qa-remove-agent-max-tokens). The wire schema field is retained on
// `AgentSlot` for backwards-compat on disk; the engine's `execute_slot`
// always resolves the per-request cap from the model library via
// `agent/llm.rs` (`lookup_model(model).auto_max_tokens()` for Anthropic;
// OpenAI-compat omits the field and lets the provider apply its own
// default). Do not bring this input back as JSX in any downstream
// refactor — it's a footgun (operators set 4096 on a 384k-output model
// and silently capped real production runs).

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

  // `bar_history_limit` UX:
  //   - empty string  → null   (engine default: send full warmup_bars slice)
  //   - positive int  → number (trim to most-recent N bars)
  //   - 0 / negative  → null   (server-side normalization mirrors this; we
  //                             reject at the input layer too to avoid the
  //                             confusing "saved 0 → reloaded as null" round-trip)
  //   - non-integer   → null   (HTML number input + step=1 keeps the UI honest)
  // Bounds: 1..=1000. The engine has no hard cap (the field is `Option<u32>`)
  // but 1000 is well past any reasonable per-decision context window and
  // gives operators a guardrail against typing 100000 by accident.
  const BAR_HISTORY_LIMIT_MIN = 1;
  const BAR_HISTORY_LIMIT_MAX = 1000;
  function patchBarHistoryLimit(raw: string) {
    if (raw.trim() === "") {
      patch("bar_history_limit", null);
      return;
    }
    const parsed = Number(raw);
    if (!Number.isFinite(parsed) || !Number.isInteger(parsed)) {
      patch("bar_history_limit", null);
      return;
    }
    if (parsed < BAR_HISTORY_LIMIT_MIN) {
      patch("bar_history_limit", null);
      return;
    }
    const clamped = Math.min(parsed, BAR_HISTORY_LIMIT_MAX);
    patch("bar_history_limit", clamped);
  }

  function changeProvider(provider: string) {
    const row = providerRows.find((p) => p.name === provider);
    const modelStillValid =
      !slot.model || !row || row.enabled_models.includes(slot.model);
    onChange({
      ...slot,
      provider,
      model: modelStillValid ? slot.model : "",
    });
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
              <Icon name="trash" size={14} />
            </button>
          ) : null}
        </div>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-4 mb-4">
        <Field label="Provider">
          {providerNames.length > 0 ? (
            <select
              value={slot.provider}
              onChange={(e) => changeProvider(e.target.value)}
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

      {/* F-8 — bar-history rolling window. `null` (empty input) keeps
          today's behavior: the full warmup_bars slice goes to the trader
          LLM. A positive integer trims the slice to its most-recent N
          entries so the prompt prefix is stable across many decisions
          and Anthropic prompt-caching can land hits. Shipped runner-side
          via PR #372; this input surfaces the existing cap to operators.
          See team/contracts/bar-history-limit-surface.md */}
      <div className="mt-4">
        <Field label="Bar history limit">
          <input
            type="number"
            inputMode="numeric"
            step={1}
            min={BAR_HISTORY_LIMIT_MIN}
            max={BAR_HISTORY_LIMIT_MAX}
            value={slot.bar_history_limit ?? ""}
            onChange={(e) => patchBarHistoryLimit(e.target.value)}
            placeholder="auto (full warmup window)"
            aria-describedby={`slot-${index}-bar-history-help`}
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
          <small
            id={`slot-${index}-bar-history-help`}
            className="block mt-1.5 text-[11.5px] text-text-3 leading-snug"
          >
            How many recent bars the agent sees per decision. Lower ={" "}
            cheaper + faster. Higher = more context. Defaults to the
            engine's runtime cap (currently set per-provider). Leave blank
            for the default; integer 1–1000.
          </small>
        </Field>
      </div>

      {/* V2D — cortex-memory mode. `off` keeps the dispatcher's memory
          seam dormant (the default). `global` shares the workspace pool
          across every memory-enabled agent; `agent_scoped` isolates this
          agent's history. See:
          docs/superpowers/plans/2026-05-21-cortex-memory-integration-plan.md */}
      <div className="mt-4">
        <Field label="Memory">
          <select
            value={slot.memory_mode ?? "off"}
            onChange={(e) =>
              patch("memory_mode", e.target.value as AgentSlot["memory_mode"])
            }
            className="w-full px-3 py-2 bg-surface-card border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
          >
            <option value="off">Off</option>
            <option value="global">Global (shared across agents)</option>
            <option value="agent_scoped">Agent-scoped (this agent only)</option>
          </select>
        </Field>
      </div>

      {slot.skill_ids.length > 0 ? (
        <div className="grid grid-cols-2 gap-4 mt-4">
          <Field label="Skills">
            <div className="text-text-3 text-[12px] px-3 py-2">
              {slot.skill_ids.length} skill
              {slot.skill_ids.length === 1 ? "" : "s"} (manage at /agents/skills)
            </div>
          </Field>
        </div>
      ) : null}

      <FiringConditionsAwareness slot={slot} />
    </div>
  );
}

// Phase 1 of the agent-firing-filter operator surface
// (docs/superpowers/specs/2026-05-22-agent-firing-filter-operator-surface.md).
// The card *teaches* — it does not author. Authoring lives at the strategy
// level in Phase 3. Filter-capable slots get no card (the Filter is the gate).
//
// `capabilities` is read defensively: the Rust field landed in Phase A
// (PR #527) but the hand-authored AgentSlot TS type in api/agents.ts does
// not yet carry it. Per the back-compat default in
// crates/xvision-engine/src/agents/model.rs, a slot with no capabilities
// resolves to {Trader} — so undefined here is treated as Trader-capable
// (firing-condition relevant; card shown).
const FIRING_CAPABLE_ROLES = new Set(["trader", "critic", "intern", "router"]);

function FiringConditionsAwareness({ slot }: { slot: AgentSlot }) {
  const rawCaps = (slot as { capabilities?: readonly string[] }).capabilities;
  const caps =
    rawCaps && rawCaps.length > 0
      ? rawCaps.map((c) => c.toLowerCase())
      : ["trader"];
  const isFilterSlot = caps.includes("filter");
  const isFiringCapable = caps.some((c) => FIRING_CAPABLE_ROLES.has(c));
  if (isFilterSlot || !isFiringCapable) {
    return null;
  }

  return (
    <div className="mt-5 px-4 py-3 bg-surface-card border border-border-soft rounded-sm">
      <div className="text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
        Firing conditions
      </div>
      <p className="text-[12.5px] text-text-2 leading-relaxed">
        This agent runs on every bar by default. To gate it on a market
        regime, indicator threshold, or other signal, add a Filter-capable
        agent upstream of this one inside a strategy.{" "}
        <a
          href="/docs?slug=firing-conditions"
          className="text-gold hover:text-gold-soft underline-offset-2 hover:underline"
        >
          Learn more →
        </a>
      </p>
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
