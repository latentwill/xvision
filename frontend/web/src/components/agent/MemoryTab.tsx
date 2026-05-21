// MemoryTab — per-agent Memory surface for V2D Observations + Patterns.
//
// Mounted on `/agents/<id>` as a sibling to the AgentForm. Two
// sub-tabs:
//
//   - Patterns      — agent-scoped + global Patterns, with an
//                     "+ Add Pattern" modal. Operator-editable per
//                     V2D intake Decision 6 (add + delete only).
//   - Observations  — agent-scoped Observations, read-only with
//                     scenario_id / run_id filters. Per V2D intake
//                     Decision 5, individual Observations are not
//                     deletable from the UI — bulk-only via "Forget
//                     all memory for this agent".
//
// The "Forget all memory" button at the bottom of the tab triggers a
// hand-rolled AlertDialog (no Radix dependency in this repo); on
// confirm it calls DELETE /api/memory?agent=<id> and invalidates the
// memory list keys so the tab re-renders empty.

import { useEffect, useMemo, useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";

import { ApiError } from "@/api/client";
import {
  agentNamespace,
  createPattern,
  forgetMemory,
  listMemory,
  memoryKeys,
  type MemoryItem,
  type PatternCreateBody,
} from "@/api/memory";
import { Card, CardHeader } from "@/components/primitives/Card";

type SubTab = "patterns" | "observations";

export function MemoryTab({ agentId }: { agentId: string }) {
  const [sub, setSub] = useState<SubTab>("patterns");
  const [forgetOpen, setForgetOpen] = useState(false);

  const allItemsQuery = useQuery({
    queryKey: memoryKeys.list({ agent: agentId }),
    queryFn: () => listMemory({ agent: agentId }),
  });

  return (
    <div className="flex flex-col gap-5">
      <Card>
        <CardHeader title="Memory" />
        <div className="px-5 pb-5">
          <SubTabBar value={sub} onChange={setSub} />
          <div
            role="tabpanel"
            aria-label={sub === "patterns" ? "Patterns" : "Observations"}
          >
            {sub === "patterns" ? (
              <PatternsPanel agentId={agentId} />
            ) : (
              <ObservationsPanel agentId={agentId} />
            )}
          </div>
        </div>
      </Card>

      <div className="flex items-center justify-end">
        <button
          type="button"
          onClick={() => setForgetOpen(true)}
          className="inline-flex items-center gap-1.5 px-3 py-1.5 rounded text-[12px] font-medium border border-danger/40 text-danger hover:bg-danger/10 transition-colors"
        >
          Forget all memory for this agent
        </button>
      </div>

      {forgetOpen ? (
        <ForgetDialog
          agentId={agentId}
          itemCount={allItemsQuery.data?.total ?? 0}
          onClose={() => setForgetOpen(false)}
        />
      ) : null}
    </div>
  );
}

// ── sub-tab bar ─────────────────────────────────────────────────────────────

function SubTabBar({
  value,
  onChange,
}: {
  value: SubTab;
  onChange: (s: SubTab) => void;
}) {
  const tabs: [SubTab, string][] = [
    ["patterns", "Patterns"],
    ["observations", "Observations"],
  ];
  return (
    <div
      role="tablist"
      aria-label="Memory sub-tabs"
      className="flex gap-4 border-b border-border mb-4"
    >
      {tabs.map(([t, label]) => (
        <button
          key={t}
          type="button"
          role="tab"
          aria-selected={value === t}
          aria-label={label}
          onClick={() => onChange(t)}
          className={`pb-2 -mb-px border-b-2 text-[13px] font-medium transition-colors ${
            value === t
              ? "border-text text-text"
              : "border-transparent text-text-3 hover:text-text-2"
          }`}
        >
          {label}
        </button>
      ))}
    </div>
  );
}

// ── patterns panel ──────────────────────────────────────────────────────────

function PatternsPanel({ agentId }: { agentId: string }) {
  const [addOpen, setAddOpen] = useState(false);
  const [scope, setScope] = useState<"agent" | "global">("agent");

  const namespace =
    scope === "agent" ? agentNamespace(agentId) : "global";

  const query = useQuery({
    queryKey: memoryKeys.list({ tier: "pattern", namespace }),
    queryFn: () => listMemory({ tier: "pattern", namespace }),
  });

  const items = query.data?.items ?? [];

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          <label className="text-[12px] text-text-3">
            <span className="mr-2">Namespace</span>
            <select
              value={scope}
              onChange={(e) => setScope(e.target.value as "agent" | "global")}
              className="bg-surface-panel border border-border rounded-sm text-[12.5px] text-text px-2 py-1 focus:outline-none focus:border-gold/40"
            >
              <option value="agent">agent:{agentId}</option>
              <option value="global">global</option>
            </select>
          </label>
        </div>
        <button
          type="button"
          onClick={() => setAddOpen(true)}
          className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded text-[12px] font-medium border border-border text-text-2 hover:text-text hover:border-border-strong transition-colors"
        >
          + Add Pattern
        </button>
      </div>

      {query.isPending ? (
        <div className="text-text-3 text-[13px] py-6">Loading patterns…</div>
      ) : query.isError ? (
        <div className="text-danger text-[13px] py-6">
          Couldn't load patterns: {errorMessage(query.error)}
        </div>
      ) : items.length === 0 ? (
        <div className="text-text-3 text-[13px] py-6">
          No patterns yet for <code className="font-mono">{namespace}</code>.
          Use "+ Add Pattern" to seed one.
        </div>
      ) : (
        <PatternList items={items} agentId={agentId} />
      )}

      {addOpen ? (
        <AddPatternDialog
          agentId={agentId}
          defaultNamespace={namespace}
          onClose={() => setAddOpen(false)}
        />
      ) : null}
    </div>
  );
}

function PatternList({
  items,
  agentId: _agentId,
}: {
  items: MemoryItem[];
  agentId: string;
}) {
  return (
    <ul className="flex flex-col gap-2">
      {items.map((it) => (
        <li
          key={it.id}
          className="border border-border rounded-sm bg-surface-panel px-3 py-2"
        >
          <div className="flex items-start justify-between gap-3">
            <div className="text-[13px] text-text whitespace-pre-wrap">
              {it.text}
            </div>
            <div className="text-[11px] text-text-3 font-mono shrink-0">
              {it.training_window_end
                ? `ends ${it.training_window_end.slice(0, 10)}`
                : "open"}
            </div>
          </div>
          <div className="mt-1 flex items-center gap-2 text-[10.5px] text-text-3 font-mono">
            <span>{it.namespace}</span>
            <span>·</span>
            <span>{it.created_at.slice(0, 10)}</span>
          </div>
        </li>
      ))}
    </ul>
  );
}

// ── add-pattern modal ───────────────────────────────────────────────────────

function AddPatternDialog({
  agentId,
  defaultNamespace,
  onClose,
}: {
  agentId: string;
  defaultNamespace: string;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const [trainingEnd, setTrainingEnd] = useState("");
  const [namespace, setNamespace] = useState(defaultNamespace);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: (body: PatternCreateBody) => createPattern(body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      onClose();
    },
    onError: (err) => setSubmitError(errorMessage(err)),
  });

  function onSubmit(e: React.FormEvent) {
    e.preventDefault();
    if (!text.trim()) {
      setSubmitError("Text is required.");
      return;
    }
    if (!namespace.trim()) {
      setSubmitError("Namespace is required.");
      return;
    }
    // The engine's PatternCreateRequest expects RFC3339; an HTML date
    // input yields YYYY-MM-DD. Append the end-of-day timestamp so the
    // V2D leakage filter compares against the latest moment within the
    // declared window (a Pattern with training_window_end=2025-12-31
    // recalls on scenarios starting 2026-01-01 onward).
    const training_window_end = trainingEnd
      ? `${trainingEnd}T23:59:59Z`
      : undefined;

    m.mutate({
      text: text.trim(),
      namespace: namespace.trim(),
      training_window_end,
    });
  }

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center pt-24 px-4 bg-bg/80 backdrop-blur-sm"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="w-full max-w-md bg-surface-card border border-border rounded-lg shadow-xl"
        onClick={(e) => e.stopPropagation()}
        role="dialog"
        aria-label="Add Pattern"
        aria-modal="true"
      >
        <form onSubmit={onSubmit} className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-serif font-medium text-[20px] tracking-tight text-text">
              Add Pattern
            </h2>
            <p className="m-0 mt-1 text-text-3 text-[12px]">
              Patterns are operator-attested wisdom. The dispatcher recalls
              them on every cycle whose scenario starts after the training
              window end (if set).
            </p>
          </div>

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Text
            </span>
            <textarea
              required
              value={text}
              onChange={(e) => setText(e.target.value)}
              rows={4}
              placeholder="e.g. Mean-revert entries fail on FOMC days; sit out announcement bars."
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text focus:outline-none focus:border-gold/40"
            />
          </label>

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Training data ends
            </span>
            <input
              type="date"
              value={trainingEnd}
              onChange={(e) => setTrainingEnd(e.target.value)}
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
            <span className="block mt-1 text-[11px] text-text-3">
              Optional. Leave blank for operator-attested wisdom recalled in
              every scenario.
            </span>
          </label>

          <label className="block">
            <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1.5">
              Namespace
            </span>
            <select
              value={namespace}
              onChange={(e) => setNamespace(e.target.value)}
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            >
              <option value={agentNamespace(agentId)}>
                agent:{agentId}
              </option>
              <option value="global">global</option>
            </select>
          </label>

          {submitError ? (
            <div className="text-danger text-[12.5px]">{submitError}</div>
          ) : null}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={onClose}
              className="px-3 py-1.5 rounded text-[12.5px] text-text-2 hover:text-text"
            >
              Cancel
            </button>
            <button
              type="submit"
              disabled={m.isPending}
              className="px-3 py-1.5 rounded text-[12.5px] font-medium border border-border text-text hover:border-border-strong disabled:opacity-50"
            >
              {m.isPending ? "Saving…" : "Add Pattern"}
            </button>
          </div>
        </form>
      </div>
    </div>
  );
}

// ── observations panel ─────────────────────────────────────────────────────

function ObservationsPanel({ agentId }: { agentId: string }) {
  const [scenarioId, setScenarioId] = useState("");
  const [runId, setRunId] = useState("");

  // Debounce the filter inputs so each keystroke doesn't fire a new
  // query — the user can paste a full UUID in one go and we'll catch
  // the trailing value 250ms later.
  const debouncedScenario = useDebounced(scenarioId, 250);
  const debouncedRun = useDebounced(runId, 250);

  const query = useQuery({
    queryKey: memoryKeys.list({
      tier: "observation",
      agent: agentId,
      scenario_id: debouncedScenario || undefined,
      run_id: debouncedRun || undefined,
    }),
    queryFn: () =>
      listMemory({
        tier: "observation",
        agent: agentId,
        scenario_id: debouncedScenario || undefined,
        run_id: debouncedRun || undefined,
      }),
  });

  const items = query.data?.items ?? [];

  return (
    <div>
      <div className="flex flex-wrap items-end gap-3 mb-3">
        <label className="block">
          <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
            Scenario id
          </span>
          <input
            type="text"
            value={scenarioId}
            onChange={(e) => setScenarioId(e.target.value)}
            placeholder="filter by scenario"
            className="px-2.5 py-1.5 bg-surface-panel border border-border rounded-sm text-[12.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
        </label>
        <label className="block">
          <span className="block text-[11px] uppercase tracking-wide text-text-3 mb-1">
            Run id
          </span>
          <input
            type="text"
            value={runId}
            onChange={(e) => setRunId(e.target.value)}
            placeholder="filter by run"
            className="px-2.5 py-1.5 bg-surface-panel border border-border rounded-sm text-[12.5px] text-text font-mono focus:outline-none focus:border-gold/40"
          />
        </label>
        <p className="m-0 ml-auto text-[11px] text-text-3">
          Observations are read-only. Use "Forget all memory" to clear.
        </p>
      </div>

      {query.isPending ? (
        <div className="text-text-3 text-[13px] py-6">Loading observations…</div>
      ) : query.isError ? (
        <div className="text-danger text-[13px] py-6">
          Couldn't load observations: {errorMessage(query.error)}
        </div>
      ) : items.length === 0 ? (
        <div className="text-text-3 text-[13px] py-6">
          No observations yet for this agent.
        </div>
      ) : (
        <ObservationList items={items} />
      )}
    </div>
  );
}

function ObservationList({ items }: { items: MemoryItem[] }) {
  return (
    <ul className="flex flex-col gap-2">
      {items.map((it) => (
        <li
          key={it.id}
          className="border border-border rounded-sm bg-surface-panel px-3 py-2"
        >
          <div className="text-[13px] text-text whitespace-pre-wrap">
            {it.text}
          </div>
          <div className="mt-1 flex flex-wrap items-center gap-2 text-[10.5px] text-text-3 font-mono">
            <span>{it.created_at.slice(0, 19).replace("T", " ")}</span>
            {it.scenario_id ? (
              <>
                <span>·</span>
                <span>scenario={it.scenario_id}</span>
              </>
            ) : null}
            {it.run_id ? (
              <>
                <span>·</span>
                <span>run={it.run_id.slice(0, 12)}…</span>
              </>
            ) : null}
            {it.cycle_idx != null ? (
              <>
                <span>·</span>
                <span>cycle={it.cycle_idx}</span>
              </>
            ) : null}
          </div>
        </li>
      ))}
    </ul>
  );
}

// ── forget dialog ──────────────────────────────────────────────────────────

function ForgetDialog({
  agentId,
  itemCount,
  onClose,
}: {
  agentId: string;
  itemCount: number;
  onClose: () => void;
}) {
  const qc = useQueryClient();
  const m = useMutation({
    mutationFn: () => forgetMemory({ agent: agentId }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      onClose();
    },
  });

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center pt-32 px-4 bg-bg/80 backdrop-blur-sm"
      onClick={onClose}
      role="presentation"
    >
      <div
        className="w-full max-w-sm bg-surface-card border border-border rounded-lg shadow-xl"
        onClick={(e) => e.stopPropagation()}
        role="alertdialog"
        aria-label="Forget all memory"
        aria-modal="true"
      >
        <div className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-serif font-medium text-[18px] tracking-tight text-text">
              Forget all memory for this agent?
            </h2>
            <p className="m-0 mt-2 text-text-2 text-[13px]">
              This will permanently delete{" "}
              <span className="font-mono text-text">{itemCount}</span>{" "}
              memory item{itemCount === 1 ? "" : "s"} from namespace{" "}
              <code className="font-mono text-text">
                {agentNamespace(agentId)}
              </code>
              . Observations and Patterns alike. This cannot be undone.
            </p>
          </div>

          {m.isError ? (
            <div className="text-danger text-[12.5px]">
              {errorMessage(m.error)}
            </div>
          ) : null}

          <div className="flex items-center justify-end gap-2">
            <button
              type="button"
              onClick={onClose}
              disabled={m.isPending}
              className="px-3 py-1.5 rounded text-[12.5px] text-text-2 hover:text-text disabled:opacity-50"
            >
              Cancel
            </button>
            <button
              type="button"
              onClick={() => m.mutate()}
              disabled={m.isPending}
              className="px-3 py-1.5 rounded text-[12.5px] font-medium border border-danger/40 text-danger hover:bg-danger/10 disabled:opacity-50"
            >
              {m.isPending ? "Forgetting…" : "Confirm forget"}
            </button>
          </div>
        </div>
      </div>
    </div>
  );
}

// ── helpers ────────────────────────────────────────────────────────────────

function errorMessage(err: unknown): string {
  if (err instanceof ApiError) return err.message;
  if (err instanceof Error) return err.message;
  return "Unknown error";
}

function useDebounced<T>(value: T, ms: number): T {
  const [debounced, setDebounced] = useState(value);
  useEffect(() => {
    const t = setTimeout(() => setDebounced(value), ms);
    return () => clearTimeout(t);
  }, [value, ms]);
  return debounced;
}

// Memo'd export shape so dev re-renders of the parent don't churn the
// inner query subscriptions unnecessarily. Not strictly required, but
// matches the AgentForm pattern of stabilising downstream subscribers.
export function useMemoryItemCount(items: MemoryItem[]): number {
  return useMemo(() => items.length, [items]);
}
