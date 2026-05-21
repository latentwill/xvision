// MemorySurface — namespace-scoped memory UI shared between the
// per-agent `<MemoryTab>` and the workspace-level `<MemoryPage>`.
//
// V2D v1.1 introduced two operator surfaces over the memory store:
//
//   - `/agents/<id>` Memory tab — scoped to `agent:<id>` (with the
//     option to flip the picker to `global`).
//   - `/memory` workspace page — scoped to `global` only.
//
// Phase 3 hand-rolled the surface inside `MemoryTab.tsx`. Phase 4
// lifts the list / modal / forget logic up here so the workspace page
// can reuse the same Patterns + Observations + AlertDialog shapes
// without duplicating ~500 LOC of TanStack-Query plumbing.
//
// The surface is configured by a discriminated `mode`:
//   - mode="agent" — Patterns sub-tab exposes the agent↔global
//     namespace picker; Observations sub-tab filters by scenario_id /
//     run_id and is scoped to `agent:<id>`; "Forget all memory"
//     deletes by `agent`.
//   - mode="workspace" — Patterns + Observations are both pinned to
//     `namespace=global`; "Forget all global memory" deletes by
//     `namespace=global`.
//
// Optional `highlightPatternId` (sourced from the `?pattern=<id>`
// query param on either route) tags the matching pattern row with
// `data-highlighted="true"` and a gold-tinted background so deep-link
// navigation from the eval-review MemoryPanel scrolls into a visibly
// distinct row.

import { useEffect, useMemo, useRef, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
} from "@tanstack/react-query";

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

export type MemorySurfaceProps =
  | {
      mode: "agent";
      agentId: string;
      highlightPatternId?: string | null;
    }
  | {
      mode: "workspace";
      highlightPatternId?: string | null;
    };

export function MemorySurface(props: MemorySurfaceProps) {
  const [sub, setSub] = useState<SubTab>("patterns");
  const [forgetOpen, setForgetOpen] = useState(false);

  // Item count for the forget-dialog summary. The query shape mirrors
  // Phase 3's MemoryTab so the cache key matches across surfaces.
  const allItemsQuery = useQuery({
    queryKey:
      props.mode === "agent"
        ? memoryKeys.list({ agent: props.agentId })
        : memoryKeys.list({ namespace: "global" }),
    queryFn: () =>
      props.mode === "agent"
        ? listMemory({ agent: props.agentId })
        : listMemory({ namespace: "global" }),
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
              <PatternsPanel
                {...props}
                highlightPatternId={props.highlightPatternId ?? null}
              />
            ) : (
              <ObservationsPanel {...props} />
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
          {props.mode === "agent"
            ? "Forget all memory for this agent"
            : "Forget all global memory"}
        </button>
      </div>

      {forgetOpen ? (
        <ForgetDialog
          {...props}
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

type PatternsPanelProps = MemorySurfaceProps & {
  highlightPatternId: string | null;
};

function PatternsPanel(props: PatternsPanelProps) {
  const [addOpen, setAddOpen] = useState(false);
  // Agent mode lets the operator toggle between the agent-scoped
  // namespace and the shared `global` shelf. Workspace mode pins to
  // `global` — no toggle, since the per-agent page already owns the
  // agent-scoped view.
  const [scope, setScope] = useState<"agent" | "global">(
    props.mode === "agent" ? "agent" : "global",
  );

  const namespace =
    props.mode === "agent"
      ? scope === "agent"
        ? agentNamespace(props.agentId)
        : "global"
      : "global";

  const query = useQuery({
    queryKey: memoryKeys.list({ tier: "pattern", namespace }),
    queryFn: () => listMemory({ tier: "pattern", namespace }),
  });

  const items = query.data?.items ?? [];

  return (
    <div>
      <div className="flex items-center justify-between mb-3">
        <div className="flex items-center gap-3">
          {props.mode === "agent" ? (
            <label className="text-[12px] text-text-3">
              <span className="mr-2">Namespace</span>
              <select
                value={scope}
                onChange={(e) =>
                  setScope(e.target.value as "agent" | "global")
                }
                className="bg-surface-panel border border-border rounded-sm text-[12.5px] text-text px-2 py-1 focus:outline-none focus:border-gold/40"
              >
                <option value="agent">agent:{props.agentId}</option>
                <option value="global">global</option>
              </select>
            </label>
          ) : (
            <span className="text-[12px] text-text-3">
              Namespace <code className="font-mono text-text-2">global</code>
            </span>
          )}
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
        <PatternList
          items={items}
          highlightPatternId={props.highlightPatternId}
        />
      )}

      {addOpen ? (
        <AddPatternDialog
          {...props}
          defaultNamespace={namespace}
          onClose={() => setAddOpen(false)}
        />
      ) : null}
    </div>
  );
}

function PatternList({
  items,
  highlightPatternId,
}: {
  items: MemoryItem[];
  highlightPatternId: string | null;
}) {
  const highlightRef = useRef<HTMLLIElement | null>(null);

  // Scroll the deep-linked row into view once it mounts. We don't
  // animate; a soft jump keeps the page reload-shareable without
  // confusing the operator about where focus moved.
  useEffect(() => {
    // `scrollIntoView` is unavailable in jsdom (vitest); guard so the
    // highlight effect doesn't crash unit tests. Production browsers
    // always have it.
    if (
      highlightPatternId &&
      highlightRef.current &&
      typeof highlightRef.current.scrollIntoView === "function"
    ) {
      highlightRef.current.scrollIntoView({
        block: "center",
        behavior: "auto",
      });
    }
  }, [highlightPatternId, items.length]);

  return (
    <ul className="flex flex-col gap-2">
      {items.map((it) => {
        const highlighted = highlightPatternId === it.id;
        return (
          <li
            key={it.id}
            ref={highlighted ? highlightRef : null}
            data-highlighted={highlighted ? "true" : undefined}
            className={
              "border rounded-sm px-3 py-2 transition-colors " +
              (highlighted
                ? "border-gold/60 bg-gold/[0.08]"
                : "border-border bg-surface-panel")
            }
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
        );
      })}
    </ul>
  );
}

// ── add-pattern modal ───────────────────────────────────────────────────────

type AddPatternDialogProps = MemorySurfaceProps & {
  defaultNamespace: string;
  onClose: () => void;
};

function AddPatternDialog(props: AddPatternDialogProps) {
  const qc = useQueryClient();
  const [text, setText] = useState("");
  const [trainingEnd, setTrainingEnd] = useState("");
  const [namespace, setNamespace] = useState(props.defaultNamespace);
  const [submitError, setSubmitError] = useState<string | null>(null);

  const m = useMutation({
    mutationFn: (body: PatternCreateBody) => createPattern(body),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      props.onClose();
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
      onClick={props.onClose}
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

          <div
            role="note"
            aria-label="Embedder requirement"
            className="px-3 py-2 rounded-sm border border-amber-500/40 bg-amber-500/5 text-[11.5px] text-amber-900 dark:text-amber-200 leading-snug"
          >
            <strong className="font-medium">Requires an embedder.</strong>{" "}
            Patterns are matched to decision context via vector similarity, so
            an agent's provider (or a configured default) must support
            embeddings. Without one, this Pattern is stored but never recalled —
            check Settings → Providers, or watch eval-review for a{" "}
            <code className="font-mono">memory_disabled_no_embedder</code>{" "}
            event after the next run.
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
              title="The latest date your training data covers. Normalized to end-of-day UTC at submit. Scenarios with start_date <= this date will EXCLUDE this Pattern (look-ahead protection); scenarios starting after will recall it."
              className="w-full px-3 py-2 bg-surface-panel border border-border rounded-sm text-[13.5px] text-text font-mono focus:outline-none focus:border-gold/40"
            />
            <div className="mt-1 space-y-0.5 text-[11px] text-text-3 leading-snug">
              <p className="m-0">
                Optional. The date your training data ends (end-of-day UTC at
                submit).
              </p>
              <p className="m-0">
                Scenarios starting <em>after</em> this date will recall this
                Pattern; scenarios overlapping or earlier exclude it
                (look-ahead protection).
              </p>
              <p className="m-0">
                Leave blank for operator wisdom recalled in <em>every</em>{" "}
                scenario.
              </p>
            </div>
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
              {props.mode === "agent" ? (
                <>
                  <option value={agentNamespace(props.agentId)}>
                    agent:{props.agentId}
                  </option>
                  <option value="global">global</option>
                </>
              ) : (
                <option value="global">global</option>
              )}
            </select>
          </label>

          {submitError ? (
            <div className="text-danger text-[12.5px]">{submitError}</div>
          ) : null}

          <div className="flex items-center justify-end gap-2 pt-1">
            <button
              type="button"
              onClick={props.onClose}
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

function ObservationsPanel(props: MemorySurfaceProps) {
  const [scenarioId, setScenarioId] = useState("");
  const [runId, setRunId] = useState("");

  const debouncedScenario = useDebounced(scenarioId, 250);
  const debouncedRun = useDebounced(runId, 250);

  const query = useQuery({
    queryKey: memoryKeys.list(
      props.mode === "agent"
        ? {
            tier: "observation",
            agent: props.agentId,
            scenario_id: debouncedScenario || undefined,
            run_id: debouncedRun || undefined,
          }
        : {
            tier: "observation",
            namespace: "global",
            scenario_id: debouncedScenario || undefined,
            run_id: debouncedRun || undefined,
          },
    ),
    queryFn: () =>
      listMemory(
        props.mode === "agent"
          ? {
              tier: "observation",
              agent: props.agentId,
              scenario_id: debouncedScenario || undefined,
              run_id: debouncedRun || undefined,
            }
          : {
              tier: "observation",
              namespace: "global",
              scenario_id: debouncedScenario || undefined,
              run_id: debouncedRun || undefined,
            },
      ),
  });

  const items = query.data?.items ?? [];

  const emptyCopy =
    props.mode === "agent"
      ? "No observations yet for this agent."
      : "No observations yet for the global namespace.";

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
        <div className="text-text-3 text-[13px] py-6">{emptyCopy}</div>
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

type ForgetDialogProps = MemorySurfaceProps & {
  itemCount: number;
  onClose: () => void;
};

function ForgetDialog(props: ForgetDialogProps) {
  const qc = useQueryClient();
  const m = useMutation({
    mutationFn: () =>
      props.mode === "agent"
        ? forgetMemory({ agent: props.agentId })
        : forgetMemory({ namespace: "global" }),
    onSuccess: () => {
      qc.invalidateQueries({ queryKey: memoryKeys.all });
      props.onClose();
    },
  });

  const title =
    props.mode === "agent"
      ? "Forget all memory for this agent?"
      : "Forget all global memory?";
  const aria =
    props.mode === "agent" ? "Forget all memory" : "Forget all global memory";
  const namespaceCode =
    props.mode === "agent" ? agentNamespace(props.agentId) : "global";

  return (
    <div
      className="fixed inset-0 z-40 flex items-start justify-center pt-32 px-4 bg-bg/80 backdrop-blur-sm"
      onClick={props.onClose}
      role="presentation"
    >
      <div
        className="w-full max-w-sm bg-surface-card border border-border rounded-lg shadow-xl"
        onClick={(e) => e.stopPropagation()}
        role="alertdialog"
        aria-label={aria}
        aria-modal="true"
      >
        <div className="p-5 space-y-4">
          <div>
            <h2 className="m-0 font-serif font-medium text-[18px] tracking-tight text-text">
              {title}
            </h2>
            <p className="m-0 mt-2 text-text-2 text-[13px]">
              This will permanently delete{" "}
              <span className="font-mono text-text">{props.itemCount}</span>{" "}
              memory item{props.itemCount === 1 ? "" : "s"} from namespace{" "}
              <code className="font-mono text-text">{namespaceCode}</code>.
              Observations and Patterns alike. This cannot be undone.
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
              onClick={props.onClose}
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

// Re-export for back-compat with Phase 3 callers that imported the
// item-count helper. Kept here so external consumers don't import from
// the per-agent component file directly.
export function useMemoryItemCount(items: MemoryItem[]): number {
  return useMemo(() => items.length, [items]);
}
