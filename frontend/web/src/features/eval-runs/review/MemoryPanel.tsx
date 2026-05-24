// V2D `MemoryPanel`. Surfaces the three new event kinds emitted by the
// dispatcher seam in `crates/xvision-engine/src/agent/execute.rs`:
//
//   - `memory_recall`               — top-k auto-recall hits prepended to
//                                     `system_prompt` as a
//                                     `<prior_observations>` block.
//   - `memory_write`                — post-dispatch decision auto-written
//                                     into the slot's memory namespace.
//   - `memory_disabled_no_embedder` — non-Off slot but no embedder is
//                                     registered for the active provider.
//
// The component is a sibling to whatever already renders per-cycle event
// rows; it filters out everything outside the V2D vocabulary so the host
// can pass the full events array without pre-filtering. Returns `null` if
// no V2D events are present, so the empty-cycle case never paints an
// empty card.
//
// Event payload shapes match the dispatcher tracing lines verbatim;
// `kind`/`payload` is a deliberately loose discriminated shape so the
// panel works against any future cycle-events fetch without coupling to
// a generated wire type that doesn't exist yet.
//
// Phase 4 deep-link: each recall row gets an overflow trigger that
// reveals an "Open Pattern" link. The destination resolves from the
// recall payload's `namespace`:
//
//   - `agent:<id>` → `/agents/<id>?tab=memory&pattern=<id>`
//   - `global`     → `/memory?pattern=<id>`
//
// The destination pages read `?pattern=<id>` and highlight + scroll
// the matching row. "Demote" is deferred to V3 (see TODO below).
//
// `memory-provenance-in-decisions-trace` (2026-05-22): recall payloads
// may now carry a `decision_id` from the dispatcher (the engine's
// `cycle_idx` for the slot invocation). When present, recall rows
// surface a "Decision N" header so operators can attribute the recalled
// items to the specific decision they fed into. Payloads without
// `decision_id` (older traces, non-eval call sites) still render
// cleanly — the header is conditional.

import { useState, type FC } from "react";
import { Link } from "react-router-dom";

type RecallItem = { id: string; score: number; text_preview: string };
type RecallPayload = {
  namespace: string;
  k: number;
  items: RecallItem[];
  /**
   * Per-decision identifier the recall fed into. Threaded from the
   * V2D dispatcher (`SlotInput.cycle_idx`) onto the observability
   * `memory_recall` event payload. Optional for back-compat: older
   * traces persisted before `memory-provenance-in-decisions-trace`
   * landed don't carry it, and CLI/unit-test call sites that don't
   * thread a decision loop also omit it.
   */
  decision_id?: number;
};
type WritePayload = { namespace: string; id: string; text_preview: string };
type DisabledPayload = { namespace: string };

type MemoryEvent =
  | { kind: "memory_recall"; payload: RecallPayload }
  | { kind: "memory_write"; payload: WritePayload }
  | { kind: "memory_disabled_no_embedder"; payload: DisabledPayload };

export type MemoryPanelEvent = { kind: string; payload: unknown };

function isMemoryEvent(e: MemoryPanelEvent): e is MemoryEvent {
  return (
    e.kind === "memory_recall" ||
    e.kind === "memory_write" ||
    e.kind === "memory_disabled_no_embedder"
  );
}

/**
 * Build the deep-link destination for a recalled item. The recall
 * payload's `namespace` is the source of truth; we don't try to peek
 * at the item's stored namespace because recall already pinned it.
 */
export function recallItemHref(namespace: string, itemId: string): string {
  if (namespace.startsWith("agent:")) {
    const agentId = namespace.slice("agent:".length);
    return `/agents/${encodeURIComponent(agentId)}?tab=memory&pattern=${encodeURIComponent(itemId)}`;
  }
  // Default to the workspace page (covers `global` plus any future
  // workspace-scoped namespaces the operator might surface).
  return `/memory?pattern=${encodeURIComponent(itemId)}`;
}

export const MemoryPanel: FC<{ events: MemoryPanelEvent[] }> = ({ events }) => {
  const memEvents = events.filter(isMemoryEvent);
  if (memEvents.length === 0) return null;

  return (
    <section className="rounded-card border border-border p-3 mt-3">
      <h4 className="mb-2 font-sans font-semibold text-[14px] text-text">Memory</h4>
      <ul className="space-y-2">
        {memEvents.map((e, i) => {
          if (e.kind === "memory_recall") {
            // memory-provenance-in-decisions-trace: when the payload
            // carries a decision_id, surface a "Decision N" prefix so
            // operators can attribute recalled items to the specific
            // decision they fed into. Older traces (and CLI/unit-test
            // call sites) emit recall events without decision_id; in
            // that case the prefix is suppressed.
            const decisionLabel =
              typeof e.payload.decision_id === "number"
                ? `Decision ${e.payload.decision_id} · `
                : "";
            return (
              <li key={i} className="text-[12px]">
                <div className="text-text-3">
                  {decisionLabel}recall · <code className="font-mono">{e.payload.namespace}</code> ·
                  {" "}k={e.payload.k}
                </div>
                <ul className="mt-1 space-y-1">
                  {e.payload.items.map((it) => (
                    <RecallRow
                      key={it.id}
                      item={it}
                      namespace={e.payload.namespace}
                    />
                  ))}
                </ul>
              </li>
            );
          }
          if (e.kind === "memory_write") {
            return (
              <li key={i} className="text-[12px]">
                <div className="text-text-3">
                  write · <code className="font-mono">{e.payload.namespace}</code>
                </div>
                <div className="text-text-2">{e.payload.text_preview}</div>
              </li>
            );
          }
          // memory_disabled_no_embedder — amber warning row. Low-opacity
          // backgrounds + lighter text for dark-mode contrast per the
          // workspace dark-border rule (no `border-white` /
          // `border-gray-1xx`); the explicit `dark:` variants keep the
          // row legible on both themes.
          return (
            <li
              key={i}
              className="text-[12px] rounded-sm border border-amber-500/40 bg-amber-500/10 text-amber-700 dark:border-amber-500/30 dark:bg-amber-500/10 dark:text-amber-300 px-2 py-1"
            >
              No embedder configured · <code className="font-mono">{e.payload.namespace}</code>
            </li>
          );
        })}
      </ul>
    </section>
  );
};

// ── recall row with overflow menu ───────────────────────────────────────────

const RecallRow: FC<{ item: RecallItem; namespace: string }> = ({
  item,
  namespace,
}) => {
  const [open, setOpen] = useState(false);

  return (
    <li
      className="flex gap-2 items-start relative group"
      title={item.text_preview}
    >
      <span className="tabular-nums text-text-3">{item.score.toFixed(2)}</span>
      <span className="text-text-2 flex-1" title={item.text_preview}>
        {item.text_preview}
      </span>
      <div className="relative shrink-0">
        <button
          type="button"
          aria-label="Open recall actions"
          aria-haspopup="menu"
          aria-expanded={open}
          onClick={() => setOpen((o) => !o)}
          className="px-1.5 py-0.5 rounded text-text-3 hover:text-text hover:bg-surface-elev/60 transition-colors"
        >
          ⋯
        </button>
        {open ? (
          <>
            {/* Click-away catch — clicking outside the menu closes it. */}
            <div
              className="fixed inset-0 z-30"
              onClick={() => setOpen(false)}
              role="presentation"
            />
            <div
              role="menu"
              className="absolute right-0 top-full mt-1 z-40 min-w-[160px] rounded-sm border border-border bg-surface-card shadow-md py-1"
            >
              <Link
                to={recallItemHref(namespace, item.id)}
                onClick={() => setOpen(false)}
                className="block px-3 py-1.5 text-[12px] text-text-2 hover:text-text hover:bg-surface-elev/60"
              >
                Open Pattern
              </Link>
              {/* TODO(V3): "Demote" — convert a recalled item back to */}
              {/* an Observation or delete it from Patterns. Deferred */}
              {/* to V3 supersede/replace semantics; until then, the */}
              {/* per-agent + workspace Patterns sub-tabs already */}
              {/* expose deletion via the standard list affordances. */}
            </div>
          </>
        ) : null}
      </div>
    </li>
  );
};
