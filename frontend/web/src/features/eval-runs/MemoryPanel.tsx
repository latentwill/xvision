// `memory-aware-eval-findings` — eval-run surface that pairs the
// per-decision memory recall list with the new memory-aware findings
// inline. Sibling to the V2D `review/MemoryPanel.tsx`, which shows the
// recall timeline inside the review tab. This component is the
// finding-aware superset: it groups recall events by `decision_id` and
// renders any matching `memory_recalled_into_*` findings directly
// underneath the recall rows, so an operator scanning a run never has
// to jump between two surfaces to see "which patterns drove which
// outcome."
//
// Inline-finding pattern: mirrors the amber-warning row treatment in
// `review/MemoryPanel.tsx` (the `memory_disabled_no_embedder` slot)
// and the verbose card body from `review/FindingCard.tsx`. The display
// stays inline — per project rule, no popups / modals.
//
// Inputs are intentionally loose-typed so the host can pass through
// whatever shape the events / findings API returns without coupling
// this component to a specific generated wire type.

import type { FC } from "react";

// ── input shapes ─────────────────────────────────────────────────────────────

export type RecallItem = { id: string; score: number; text_preview: string };
export type RecallPayload = {
  namespace: string;
  items: RecallItem[];
  /**
   * Per-decision identifier the recall fed into. Threaded from the
   * V2D dispatcher (`SlotInput.cycle_idx`) onto the observability
   * `memory_recall` event payload. Optional for back-compat: traces
   * persisted before `memory-provenance-in-decisions-trace` landed
   * don't carry it.
   */
  decision_id?: number;
};

export type MemoryEvent = { kind: "memory_recall"; payload: RecallPayload };

export type MemoryAwareFinding = {
  /** Stable id from the engine. */
  id: string;
  /** `memory_recalled_into_bad_decision` or `memory_recalled_into_good_decision`. */
  kind: string;
  /** Engine emits `warning` for bad outcomes, `info` for opt-in good ones. */
  severity: "info" | "warning" | "critical";
  summary: string;
  description?: string;
  recommendation?: string;
  /**
   * Engine evidence shape — open JSON. We narrow with runtime guards
   * because `Finding.evidence` is `unknown` on the wire.
   */
  evidence?: unknown;
};

export type MemoryPanelProps = {
  events: Array<{ kind: string; payload: unknown }>;
  findings: MemoryAwareFinding[];
};

// ── narrow event shape ───────────────────────────────────────────────────────

function isMemoryRecall(e: {
  kind: string;
  payload: unknown;
}): e is MemoryEvent {
  if (e.kind !== "memory_recall") return false;
  const p = e.payload as Partial<RecallPayload> | undefined;
  return (
    !!p &&
    typeof p.namespace === "string" &&
    Array.isArray(p.items)
  );
}

// ── narrow finding evidence ──────────────────────────────────────────────────

type EvidenceShape = {
  decision_index?: number;
  memory_item_ids?: string[];
};

function evidenceOf(f: MemoryAwareFinding): EvidenceShape {
  const e = f.evidence;
  if (typeof e === "object" && e !== null) {
    return e as EvidenceShape;
  }
  return {};
}

function isMemoryAwareKind(kind: string): boolean {
  return (
    kind === "memory_recalled_into_bad_decision" ||
    kind === "memory_recalled_into_good_decision"
  );
}

// ── presentation helpers ─────────────────────────────────────────────────────

function severityClasses(severity: MemoryAwareFinding["severity"]): string {
  // Inline finding row colours — low-opacity backgrounds + lighter
  // text per the workspace dark-mode rule (no `border-white`,
  // `border-gray-1xx`; always include `dark:` variants).
  switch (severity) {
    case "warning":
      return "border-amber-500/40 bg-amber-500/10 text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300";
    case "info":
      return "border-sky-500/40 bg-sky-500/10 text-sky-700 dark:border-sky-500/30 dark:bg-sky-500/10 dark:text-sky-300";
    case "critical":
      return "border-rose-500/40 bg-rose-500/10 text-rose-700 dark:border-rose-500/30 dark:bg-rose-500/10 dark:text-rose-300";
  }
}

// ── component ────────────────────────────────────────────────────────────────

export const MemoryPanel: FC<MemoryPanelProps> = ({ events, findings }) => {
  const recalls = events.filter(isMemoryRecall);
  const memoryFindings = (findings ?? []).filter((f) => isMemoryAwareKind(f.kind));

  // Group findings by decision_index so each recall block can render
  // its associated finding(s) inline.
  const findingsByDecision = new Map<number, MemoryAwareFinding[]>();
  for (const f of memoryFindings) {
    const idx = evidenceOf(f).decision_index;
    if (typeof idx !== "number") continue;
    const list = findingsByDecision.get(idx) ?? [];
    list.push(f);
    findingsByDecision.set(idx, list);
  }

  // Surface any orphaned findings (no matching recall in `events`) at
  // the bottom of the panel so they're still discoverable. This can
  // happen when the panel is rendered with a partial events slice but
  // a full findings list (e.g. a paginated trace view).
  const seenDecisions = new Set(
    recalls
      .map((r) => r.payload.decision_id)
      .filter((d): d is number => typeof d === "number"),
  );
  const orphanedFindings = memoryFindings.filter((f) => {
    const idx = evidenceOf(f).decision_index;
    if (typeof idx !== "number") return true;
    return !seenDecisions.has(idx);
  });

  if (recalls.length === 0 && memoryFindings.length === 0) {
    return null;
  }

  return (
    <section
      className="rounded-card border border-border p-3 mt-3"
      data-testid="memory-panel"
    >
      <h4 className="mb-2 font-serif italic text-[14px] text-text">Memory</h4>

      {recalls.length > 0 ? (
        <ul className="space-y-3">
          {recalls.map((e, i) => {
            const decisionId = e.payload.decision_id;
            const matched =
              typeof decisionId === "number"
                ? (findingsByDecision.get(decisionId) ?? [])
                : [];
            return (
              <li key={i} className="text-[12px]">
                <div className="text-text-3">
                  {typeof decisionId === "number" ? (
                    <>Decision {decisionId} · </>
                  ) : null}
                  recall ·{" "}
                  <code className="font-mono">{e.payload.namespace}</code>
                </div>
                <ul className="mt-1 space-y-1">
                  {e.payload.items.map((it) => (
                    <li
                      key={it.id}
                      className="flex gap-2 items-start"
                      title={it.text_preview}
                    >
                      <span className="tabular-nums text-text-3">
                        {it.score.toFixed(2)}
                      </span>
                      <span className="text-text-2 flex-1">
                        {it.text_preview}
                      </span>
                      <code className="text-[11px] text-text-3 font-mono shrink-0">
                        {it.id}
                      </code>
                    </li>
                  ))}
                </ul>
                {matched.length > 0 ? (
                  <ul className="mt-2 space-y-1.5">
                    {matched.map((f) => (
                      <FindingRow key={f.id} finding={f} />
                    ))}
                  </ul>
                ) : null}
              </li>
            );
          })}
        </ul>
      ) : null}

      {orphanedFindings.length > 0 ? (
        <ul className="mt-3 space-y-1.5" data-testid="memory-orphan-findings">
          {orphanedFindings.map((f) => (
            <FindingRow key={f.id} finding={f} />
          ))}
        </ul>
      ) : null}
    </section>
  );
};

const FindingRow: FC<{ finding: MemoryAwareFinding }> = ({ finding }) => {
  const cls = severityClasses(finding.severity);
  const ids = evidenceOf(finding).memory_item_ids ?? [];
  const role = finding.severity === "info" ? "status" : "alert";
  return (
    <li
      role={role}
      data-finding-kind={finding.kind}
      data-finding-severity={finding.severity}
      className={`rounded-sm border px-2 py-1.5 text-[12px] ${cls}`}
    >
      <div className="font-medium">{finding.summary}</div>
      {finding.description ? (
        <div className="mt-1 opacity-90 whitespace-pre-line">
          {finding.description}
        </div>
      ) : null}
      {finding.recommendation ? (
        <div className="mt-1 opacity-90">
          <span className="mr-1">→</span>
          {finding.recommendation}
        </div>
      ) : null}
      {ids.length > 0 ? (
        <div className="mt-1 flex flex-wrap gap-1">
          {ids.map((id) => (
            <code
              key={id}
              className="text-[11px] px-1 py-0.5 rounded-sm bg-black/10 dark:bg-white/10 font-mono"
              data-testid="memory-finding-item-id"
            >
              {id}
            </code>
          ))}
        </div>
      ) : null}
    </li>
  );
};
