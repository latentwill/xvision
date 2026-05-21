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

import type { FC } from "react";

type RecallItem = { id: string; score: number; text_preview: string };
type RecallPayload = { namespace: string; k: number; items: RecallItem[] };
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

export const MemoryPanel: FC<{ events: MemoryPanelEvent[] }> = ({ events }) => {
  const memEvents = events.filter(isMemoryEvent);
  if (memEvents.length === 0) return null;

  return (
    <section className="rounded-card border border-border p-3 mt-3">
      <h4 className="mb-2 font-serif italic text-[14px] text-text">Memory</h4>
      <ul className="space-y-2">
        {memEvents.map((e, i) => {
          if (e.kind === "memory_recall") {
            return (
              <li key={i} className="text-[12px]">
                <div className="text-text-3">
                  recall · <code className="font-mono">{e.payload.namespace}</code> ·
                  {" "}k={e.payload.k}
                </div>
                <ul className="mt-1 space-y-1">
                  {e.payload.items.map((it) => (
                    <li key={it.id} className="flex gap-2">
                      <span className="tabular-nums text-text-3">
                        {it.score.toFixed(2)}
                      </span>
                      <span className="text-text-2">{it.text_preview}</span>
                    </li>
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
