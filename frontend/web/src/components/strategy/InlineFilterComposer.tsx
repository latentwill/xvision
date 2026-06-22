// Inline composer for adding a firing Filter to a non-Filter AgentRef.
// Phase 3 of `agent-firing-filter`. Renders as an in-card accordion
// expansion — NEVER a popup, per /CLAUDE.md.
//
// Flow:
//   1. Operator clicks "Add filter →" on a non-Filter AgentRef row.
//   2. Composer opens inline. Operator picks an existing Filter agent
//      OR authors a new one (provider, model, system_prompt).
//   3. "Save as reusable agent" toggle defaults ON. OFF → the new
//      agent persists with `scope_strategy_id = <this strategy>` and
//      is hidden from the workspace agent list.
//   4. Operator composes a predicate (field, op, value).
//   5. On save: agent is created/looked-up, AgentRef appended with
//      `activates: "filter"`, PipelineEdge added with the predicate.

import { useMemo, useState } from "react";

import { createAgent, type Agent } from "@/api/agents";
import type { ProviderRow } from "@/api/types.gen/ProviderRow";
import {
  addStrategyAgent,
  setStrategyPipeline,
  type AgentRef,
  type EdgePredicate,
  type PipelineDef,
} from "@/api/strategies";
import { ModelPicker } from "@/components/ModelPicker";
import { SignalSearchableSelectMenu, SignalSelectMenu } from "@/components/primitives/SignalMenu";

import {
  buildPredicate,
  describePredicate,
  SCALAR_OPS,
  withAddedEdge,
  type ScalarOp,
} from "./firingPredicate";

export type InlineFilterComposerProps = {
  strategyId: string;
  /// The non-Filter ref this composer will gate. The created Filter
  /// becomes upstream of this ref via a `PipelineEdge`.
  target: AgentRef;
  /// Current pipeline. Used to merge the new edge into the existing
  /// edge list and promote the pipeline kind to `graph`.
  pipeline: PipelineDef;
  /// All existing Filter-capable agents — workspace + this strategy's
  /// own scoped agents. The strategy editor passes the result of
  /// `listAgents({ scope: <strategy_id> })` filtered to those whose
  /// slots include `allowed_tools: ["indicator_panel"]`.
  filterCandidates: Agent[];
  /// Suggested default role for the new Filter ref (eg. "regime_filter").
  defaultRole?: string;
  /// Existing upstream Filter ref when editing an already-configured
  /// firing edge. Edit mode keeps the same ref/role and only rewrites
  /// the predicate.
  existingFilterRef?: AgentRef;
  /// Existing edge condition used to seed the flat predicate editor
  /// when it is scalar.
  initialCondition?: EdgePredicate | null;
  /// Available providers for the inline author-new flow.
  providers: ProviderRow[];
  /// Closes the composer (operator hit Cancel, or the parent decided
  /// the action is done).
  onClose: () => void;
  /// Notifies the parent that the strategy mutated and queries should
  /// be invalidated.
  onSaved: () => void;
};

type Mode = "pick" | "author";

export function InlineFilterComposer({
  strategyId,
  target,
  pipeline,
  filterCandidates,
  defaultRole = "filter",
  existingFilterRef,
  initialCondition,
  providers,
  onClose,
  onSaved,
}: InlineFilterComposerProps) {
  const editing = existingFilterRef !== undefined;
  const initialPredicate = initialCondition
    ? describePredicate(initialCondition)
    : null;
  const [mode, setMode] = useState<Mode>(
    editing || filterCandidates.length > 0 ? "pick" : "author",
  );
  const [pickedAgentId, setPickedAgentId] = useState<string>(
    existingFilterRef?.agent_id ?? filterCandidates[0]?.agent_id ?? "",
  );
  const [filterRole, setFilterRole] = useState(
    existingFilterRef?.role ?? defaultRole,
  );
  const [signalField, setSignalField] = useState(
    initialPredicate?.signalField ?? "regime",
  );
  const [op, setOp] = useState<ScalarOp>(initialPredicate?.op ?? "eq");
  const [rawValue, setRawValue] = useState(
    initialPredicate ? formatRawValue(initialPredicate.value) : "",
  );

  const [newName, setNewName] = useState("");
  const [newProvider, setNewProvider] = useState<string | null>(null);
  const [newModel, setNewModel] = useState("");
  const [newPrompt, setNewPrompt] = useState("");
  const [saveAsReusable, setSaveAsReusable] = useState(true);

  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  const candidateById = useMemo(
    () => new Map(filterCandidates.map((a) => [a.agent_id, a])),
    [filterCandidates],
  );

  function canSubmit(): boolean {
    if (filterRole.trim() === "") return false;
    if (signalField.trim() === "") return false;
    if (rawValue.trim() === "" && (op === "eq" || op === "neq")) {
      // empty value would author `eq <field> ""` — surprising; require
      // an explicit value.
      return false;
    }
    if (editing) return existingFilterRef?.agent_id === pickedAgentId;
    if (mode === "pick") return pickedAgentId !== "";
    return (
      newName.trim() !== "" &&
      newProvider !== null &&
      newModel.trim() !== "" &&
      newPrompt.trim() !== ""
    );
  }

  async function submit() {
    if (!canSubmit() || busy) return;
    setBusy(true);
    setErr(null);
    try {
      let agentId: string;
      if (mode === "pick") {
        agentId = pickedAgentId;
      } else {
        const created = await createAgent({
          name: newName.trim(),
          description: `Filter agent for strategy ${strategyId}`,
          tags: ["filter"],
          slots: [
            {
              name: "main",
              provider: newProvider!,
              model: newModel.trim(),
              system_prompt: newPrompt.trim(),
              skill_ids: [],
              max_tokens: null,
              // Inline-authored agents are Filter agents by intent —
              // the Phase B dispatcher reads this when resolving the
              // (Agent, AgentRef.activates="filter") pair at run time.
              allowed_tools: ["indicator_panel"],
            },
          ],
          // Toggle ON (default) → workspace agent (scope undefined).
          // Toggle OFF → scoped to this strategy.
          scope_strategy_id: saveAsReusable ? undefined : strategyId,
        });
        agentId = created.agent_id;
      }
      if (!editing) {
        await addStrategyAgent(strategyId, {
          agent_id: agentId,
          role: filterRole.trim(),
          // Phase A `activates`: this position plays the Filter
          // capability of the referenced agent, even if the agent also
          // advertises a Trader capability.
          activates: "filter",
        });
      }
      const newPipeline = withAddedEdge(pipeline, {
        from_role: filterRole.trim(),
        to_role: target.role,
        condition: buildPredicate(op, signalField.trim(), rawValue),
      });
      await setStrategyPipeline(strategyId, {
        kind: newPipeline.kind,
        edges: newPipeline.edges,
      });
      onSaved();
      onClose();
    } catch (e) {
      setErr(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  }

  return (
    <div
      data-testid={`inline-filter-composer-${target.role}`}
      className="border border-border-soft rounded p-3 space-y-3 bg-surface-elev"
    >
      <div className="text-[12px] uppercase tracking-wide text-text-3">
        {editing ? "Edit filter" : "Add filter"} for {target.role}
      </div>

      <div className="flex gap-2 text-[12px]">
        <button
          type="button"
          className={modeButtonClass(mode === "pick")}
          onClick={() => setMode("pick")}
          disabled={editing || filterCandidates.length === 0}
        >
          Pick existing
        </button>
        <button
          type="button"
          className={modeButtonClass(mode === "author")}
          onClick={() => setMode("author")}
          disabled={editing}
        >
          Author new agent
        </button>
      </div>

      {mode === "pick" ? (
        <div className="space-y-2">
          <FieldLabel>Filter agent</FieldLabel>
          {filterCandidates.length === 0 ? (
            <div className="text-[12px] text-text-3">
              No Filter-capable agents in the workspace yet. Switch to
              "Author new agent" to create one inline.
            </div>
          ) : (
            <SignalSearchableSelectMenu
              ariaLabel="Filter agent"
              value={pickedAgentId}
              onChange={setPickedAgentId}
              placeholder="Select filter agent…"
              searchPlaceholder="Search filter agents…"
              emptyHint="No filter agents match"
              disabled={editing}
              className="w-full justify-between"
              options={filterCandidates.map((agent) => ({
                value: agent.agent_id,
                label: agent.name,
                meta: `${agent.agent_id}${agent.scope_strategy_id ? " · scoped" : ""}`,
                searchText: `${agent.name} ${agent.agent_id} ${agent.description ?? ""} ${agent.scope_strategy_id ?? ""}`,
              }))}
            />
          )}
          {pickedAgentId && candidateById.get(pickedAgentId)?.description ? (
            <div className="text-[12px] text-text-3">
              {candidateById.get(pickedAgentId)!.description}
            </div>
          ) : null}
        </div>
      ) : (
        <div className="space-y-2">
          <FieldLabel>Filter agent name</FieldLabel>
          <input
            data-testid="inline-filter-composer-new-name"
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            value={newName}
            onChange={(e) => setNewName(e.target.value)}
            placeholder="regime-detector-v1"
          />
          <FieldLabel>Provider / model</FieldLabel>
          <ModelPicker
            rows={providers}
            loading={false}
            provider={newProvider}
            model={newModel}
            onChange={(p, m) => {
              setNewProvider(p);
              setNewModel(m);
            }}
            className="w-full"
          />
          <FieldLabel>System prompt</FieldLabel>
          <textarea
            className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono min-h-[120px]"
            value={newPrompt}
            onChange={(e) => setNewPrompt(e.target.value)}
            placeholder="You are a regime classifier. Read the bar history and emit { regime: 'trend' | 'chop' | 'high_vol', confidence: 0..1 }."
          />
          <label className="flex items-center gap-2 text-[12px] text-text-2 pt-1">
            <input
              type="checkbox"
              data-testid="inline-filter-composer-save-as-reusable"
              checked={saveAsReusable}
              onChange={(e) => setSaveAsReusable(e.target.checked)}
            />
            <span>
              Save as reusable agent
              <span className="text-text-3"> · </span>
              <span className="text-text-3">
                {saveAsReusable
                  ? "appears in /agents alongside the workspace agents"
                  : "scoped to this strategy only; hidden from /agents"}
              </span>
            </span>
          </label>
        </div>
      )}

      <div className="border-t border-border-soft pt-3 space-y-2">
        <FieldLabel>Pipeline role for this filter</FieldLabel>
        <input
          className="w-full bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono disabled:opacity-70"
          value={filterRole}
          onChange={(e) => setFilterRole(e.target.value)}
          disabled={editing}
        />

        <FieldLabel>Fires when</FieldLabel>
        <div className="grid grid-cols-[1fr_auto_1fr] gap-2 items-start">
          <input
            className="bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            value={signalField}
            onChange={(e) => setSignalField(e.target.value)}
            placeholder="regime"
            aria-label="signal field"
          />
          <SignalSelectMenu
            ariaLabel="operator"
            value={op}
            options={SCALAR_OPS.map((entry) => ({
              value: entry.op,
              label: entry.label,
            }))}
            onChange={(next) => setOp(next as ScalarOp)}
            className="justify-between bg-surface-elev font-mono"
            minWidth={120}
          />
          <input
            className="bg-surface-elev border border-border rounded px-3 py-2 text-[13px] text-text font-mono"
            value={rawValue}
            onChange={(e) => setRawValue(e.target.value)}
            placeholder="trend"
            aria-label="value"
          />
        </div>
        <div className="text-[11px] text-text-3">
          Reads <code className="font-mono">{signalField || "<field>"}</code>{" "}
          from the filter agent's most-recent signal payload.
        </div>
      </div>

      {err ? <div className="text-[12px] text-danger">{err}</div> : null}

      <div className="flex gap-2 pt-1">
        <button
          type="button"
          data-testid="inline-filter-composer-save"
          className="px-3 py-1.5 rounded text-[12px] border border-border disabled:opacity-50"
          disabled={!canSubmit() || busy}
          onClick={submit}
        >
          {busy ? "Saving…" : "Save filter"}
        </button>
        <button
          type="button"
          className="px-3 py-1.5 rounded text-[12px] border border-border-soft text-text-2"
          onClick={onClose}
          disabled={busy}
        >
          Cancel
        </button>
      </div>
    </div>
  );
}

function FieldLabel({ children }: { children: React.ReactNode }) {
  return (
    <div className="text-[11px] uppercase tracking-wide text-text-3">
      {children}
    </div>
  );
}

function modeButtonClass(active: boolean): string {
  return [
    "px-2.5 py-1 rounded border text-[12px]",
    active
      ? "border-border bg-surface-elev text-text"
      : "border-border-soft text-text-2 hover:text-text",
  ].join(" ");
}

function formatRawValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "number" || typeof value === "boolean") {
    return String(value);
  }
  if (value === null) return "null";
  return JSON.stringify(value);
}
