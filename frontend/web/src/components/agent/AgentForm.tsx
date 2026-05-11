// AgentForm — identity + slots + cross-refs editor for an agent.
// Used by both /agents/new (create mode) and /agents/:id (edit mode).
//
// Save flow:
//   1. Operator edits fields → local state changes
//   2. "Save" button POSTs (create) or PUTs (update) via TanStack mutations
//   3. On success, run validate() and surface diagnostics inline
//   4. Errors (Validation/Conflict) surface as inline messages above Save

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useNavigate } from "react-router-dom";

import {
  agentKeys,
  archiveAgent,
  createAgent,
  deployedInStrategies,
  getAgent,
  recentRuns,
  updateAgent,
  validateAgent,
  type AgentSlot,
  type ValidationDiagnostic,
} from "@/api/agents";
import { ApiError } from "@/api/client";
import { Card, CardHeader } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import { Icon } from "@/components/primitives/Icon";
import { SlotForm } from "./SlotForm";

const BLANK_SLOT: AgentSlot = {
  name: "main",
  provider: "",
  model: "",
  system_prompt: "",
  skill_ids: [],
  max_tokens: 4096,
};

const BLANK_AGENT_DRAFT = {
  name: "",
  description: "",
  tags: [] as string[],
  slots: [BLANK_SLOT] as AgentSlot[],
};

type AgentDraft = typeof BLANK_AGENT_DRAFT;

export function AgentForm({ agentId }: { agentId?: string }) {
  const isEdit = Boolean(agentId);
  const navigate = useNavigate();
  const qc = useQueryClient();

  const existing = useQuery({
    queryKey: agentId ? agentKeys.detail(agentId) : ["agents", "noop"],
    queryFn: () => getAgent(agentId!),
    enabled: isEdit,
  });

  const [draft, setDraft] = useState<AgentDraft>(BLANK_AGENT_DRAFT);
  const [diagnostics, setDiagnostics] = useState<ValidationDiagnostic[]>([]);
  const [saveError, setSaveError] = useState<string | null>(null);

  // Load existing into local state once.
  useEffect(() => {
    if (existing.data) {
      const a = existing.data;
      setDraft({
        name: a.name,
        description: a.description,
        tags: a.tags,
        slots: a.slots.length > 0 ? a.slots : [BLANK_SLOT],
      });
    }
  }, [existing.data]);

  const createM = useMutation({
    mutationFn: createAgent,
    onSuccess: async (created) => {
      await qc.invalidateQueries({ queryKey: agentKeys.all });
      await runValidate(created.agent_id);
      navigate(`/agents/${encodeURIComponent(created.agent_id)}`);
    },
    onError: (e) => setSaveError(errorMessage(e)),
  });

  const updateM = useMutation({
    mutationFn: ({ id, body }: { id: string; body: AgentDraft }) =>
      updateAgent(id, body),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: agentKeys.all });
      if (agentId) await runValidate(agentId);
    },
    onError: (e) => setSaveError(errorMessage(e)),
  });

  const archiveM = useMutation({
    mutationFn: () => archiveAgent(agentId!),
    onSuccess: async () => {
      await qc.invalidateQueries({ queryKey: agentKeys.all });
      navigate("/agents");
    },
    onError: (e) => setSaveError(errorMessage(e)),
  });

  async function runValidate(id: string) {
    try {
      const diags = await validateAgent(id);
      setDiagnostics(diags);
    } catch {
      // validation errors are best-effort — don't block save success
    }
  }

  function onSave() {
    setSaveError(null);
    if (isEdit && agentId) {
      updateM.mutate({ id: agentId, body: draft });
    } else {
      createM.mutate({
        name: draft.name,
        description: draft.description,
        tags: draft.tags,
        slots: draft.slots,
      });
    }
  }

  function patchSlot(idx: number, next: AgentSlot) {
    setDraft((d) => ({
      ...d,
      slots: d.slots.map((s, i) => (i === idx ? next : s)),
    }));
  }

  function addSlot() {
    setDraft((d) => ({
      ...d,
      slots: [...d.slots, { ...BLANK_SLOT, name: `slot_${d.slots.length + 1}` }],
    }));
  }

  function removeSlot(idx: number) {
    setDraft((d) => ({
      ...d,
      slots: d.slots.filter((_, i) => i !== idx),
    }));
  }

  function duplicateSlot(idx: number) {
    setDraft((d) => {
      const src = d.slots[idx];
      if (!src) return d;
      return {
        ...d,
        slots: [
          ...d.slots.slice(0, idx + 1),
          { ...src, name: `${src.name}_copy` },
          ...d.slots.slice(idx + 1),
        ],
      };
    });
  }

  const errors = useMemo(
    () => diagnostics.filter((d) => d.severity === "Error"),
    [diagnostics],
  );
  const warnings = useMemo(
    () => diagnostics.filter((d) => d.severity === "Warning"),
    [diagnostics],
  );

  const saving = createM.isPending || updateM.isPending;

  if (isEdit && existing.isPending) {
    return <div className="text-text-3 text-[13px]">Loading…</div>;
  }
  if (isEdit && existing.isError) {
    return (
      <div className="text-danger text-[13px]">
        Couldn't load agent: {errorMessage(existing.error)}
      </div>
    );
  }

  return (
    <div className="flex flex-col gap-5">
      {/* Identity */}
      <Card>
        <CardHeader title="Identity" />
        <div className="px-5 pb-5 grid grid-cols-1 md:grid-cols-2 gap-4">
          <label className="block md:col-span-2">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Name
            </span>
            <input
              type="text"
              value={draft.name}
              onChange={(e) =>
                setDraft((d) => ({ ...d, name: e.target.value }))
              }
              placeholder="e.g. btc-mean-rev-v1"
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
          </label>

          <label className="block md:col-span-2">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Description
            </span>
            <input
              type="text"
              value={draft.description}
              onChange={(e) =>
                setDraft((d) => ({ ...d, description: e.target.value }))
              }
              placeholder="One-line summary of what this agent does"
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
            />
          </label>

          <label className="block md:col-span-2">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Tags (comma-separated)
            </span>
            <input
              type="text"
              value={draft.tags.join(", ")}
              onChange={(e) =>
                setDraft((d) => ({
                  ...d,
                  tags: e.target.value
                    .split(",")
                    .map((t) => t.trim())
                    .filter(Boolean),
                }))
              }
              placeholder="mean-rev, btc, scalper"
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text-2 focus:outline-none focus:border-gold/40"
            />
          </label>
        </div>
      </Card>

      {/* Behavior — slots */}
      <Card>
        <CardHeader
          title="Behavior"
          actions={
            <button
              type="button"
              onClick={addSlot}
              className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:text-text hover:border-border-strong transition-colors"
            >
              <Icon name="plus" size={12} />
              Add slot
            </button>
          }
        />
        <div className="px-5 pb-5">
          {draft.slots.map((slot, i) => (
            <SlotForm
              key={i}
              slot={slot}
              index={i}
              canRemove={draft.slots.length > 1}
              onChange={(s) => patchSlot(i, s)}
              onRemove={() => removeSlot(i)}
              onDuplicate={() => duplicateSlot(i)}
            />
          ))}
        </div>
      </Card>

      {/* Cross-refs (edit mode only — nothing to show until saved) */}
      {isEdit && agentId ? <CrossRefs agentId={agentId} /> : null}

      {/* Validation feedback */}
      {(errors.length > 0 || warnings.length > 0) && (
        <Card>
          <CardHeader title="Validation" />
          <div className="px-5 pb-5">
            <DiagnosticList diagnostics={diagnostics} />
          </div>
        </Card>
      )}

      {/* Save bar */}
      <div className="flex items-center justify-between bg-surface-panel border border-border rounded-card px-5 py-4 sticky bottom-4">
        <div className="text-[12px] text-text-3">
          {saveError ? (
            <span className="text-danger">{saveError}</span>
          ) : isEdit ? (
            "Saving updates the agent in place."
          ) : (
            "Saving creates the agent and routes to its detail page."
          )}
        </div>
        <div className="flex items-center gap-2">
          {isEdit ? (
            <button
              type="button"
              onClick={() => archiveM.mutate()}
              disabled={archiveM.isPending}
              className="px-3 py-2 rounded text-[13px] border border-border text-text-3 hover:text-danger hover:border-danger/40 transition-colors disabled:opacity-50"
            >
              Archive
            </button>
          ) : null}
          <button
            type="button"
            onClick={onSave}
            disabled={saving}
            className="px-4 py-2 rounded text-[13px] font-medium bg-gold text-bg hover:bg-gold-soft transition-colors disabled:opacity-50"
          >
            {saving ? "Saving…" : isEdit ? "Save changes" : "Create agent"}
          </button>
        </div>
      </div>
    </div>
  );
}

function CrossRefs({ agentId }: { agentId: string }) {
  const deployedQ = useQuery({
    queryKey: agentKeys.deployedIn(agentId),
    queryFn: () => deployedInStrategies(agentId),
  });
  const runsQ = useQuery({
    queryKey: agentKeys.recentRuns(agentId),
    queryFn: () => recentRuns(agentId, 5),
  });

  return (
    <Card>
      <CardHeader title="Where this agent is used" />
      <div className="px-5 pb-5 grid grid-cols-1 md:grid-cols-2 gap-6">
        <div>
          <h4 className="text-[12px] uppercase tracking-wide text-text-3 mb-2 font-medium">
            Deployed in strategies
          </h4>
          {deployedQ.data && deployedQ.data.length > 0 ? (
            <ul className="space-y-1.5">
              {deployedQ.data.map((s) => (
                <li key={s.strategy_id} className="text-[13px] text-text-2">
                  {s.name}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-text-3 text-[12.5px] m-0 leading-snug">
              Not deployed in any strategy yet. Reference this agent from a
              strategy's authoring page to link it.
            </p>
          )}
        </div>

        <div>
          <h4 className="text-[12px] uppercase tracking-wide text-text-3 mb-2 font-medium">
            Recent runs
          </h4>
          {runsQ.data && runsQ.data.length > 0 ? (
            <ul className="space-y-1.5">
              {runsQ.data.map((r) => (
                <li
                  key={r.run_id}
                  className="text-[13px] text-text-2 font-mono"
                >
                  {r.run_id.slice(0, 12)}… — {r.status}
                </li>
              ))}
            </ul>
          ) : (
            <p className="text-text-3 text-[12.5px] m-0 leading-snug">
              No runs yet. Eval-run attribution lands when strategies start
              referencing agents.
            </p>
          )}
        </div>
      </div>
    </Card>
  );
}

function DiagnosticList({
  diagnostics,
}: {
  diagnostics: ValidationDiagnostic[];
}) {
  return (
    <ul className="space-y-2">
      {diagnostics.map((d, i) => (
        <li key={i} className="flex items-start gap-2.5 text-[13px]">
          <Pill
            tone={
              d.severity === "Error"
                ? "danger"
                : d.severity === "Warning"
                  ? "warn"
                  : "info"
            }
            className="mt-0.5"
          >
            {d.severity}
          </Pill>
          <div className="flex-1">
            <div className="text-text">{d.message}</div>
            {d.field ? (
              <div className="text-text-3 text-[11px] font-mono mt-0.5">
                {d.field}
              </div>
            ) : null}
          </div>
        </li>
      ))}
    </ul>
  );
}

function errorMessage(e: unknown): string {
  if (e instanceof ApiError) return `${e.code}: ${e.message}`;
  if (e instanceof Error) return e.message;
  return String(e);
}
