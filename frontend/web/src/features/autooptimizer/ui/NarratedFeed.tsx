import type { CycleProgressEvent } from "../api";
import { narrateEvent, type NarrationTone } from "../selectors/narrateEvent";
import { ExpandableArtifact } from "./ExpandableArtifact";

const toneClass: Record<NarrationTone, string> = {
  kept: "text-gold",
  rejected: "text-danger",
  suspect: "text-warn",
  warn: "text-warn",
  neutral: "text-text-2",
};

/** Marker dot colour per tone — the timeline spine reads at a glance. */
const dotClass: Record<NarrationTone, string> = {
  kept: "bg-gold",
  rejected: "bg-danger",
  suspect: "bg-warn",
  warn: "bg-warn",
  neutral: "bg-text-4",
};

function fmtTime(ts?: string) {
  if (!ts) return "";
  try {
    return new Date(ts).toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
  } catch {
    return "";
  }
}

export function NarratedFeed({
  events,
  maxItems = 100,
}: {
  events: CycleProgressEvent[];
  maxItems?: number;
}) {
  const rows = events.slice(-maxItems).reverse();
  return (
    <ol className="space-y-0" aria-label="Cycle events">
      {rows.map((e, i) => {
        const n = narrateEvent(e);
        const isLast = i === rows.length - 1;
        const line = (
          <span className="flex min-w-0 items-baseline gap-3 font-mono text-[12px]">
            <span className="flex-none tabular-nums text-text-4">{fmtTime(e.ts)}</span>
            <span className={`min-w-0 ${toneClass[n.tone]}`}>{n.sentence}</span>
          </span>
        );
        return (
          <li key={e.ts ? `${e.ts}-${i}` : i} className="relative flex gap-3">
            {/* Timeline spine: a faint connector with a tone-coloured node. */}
            <span className="relative flex w-2 flex-none justify-center" aria-hidden>
              {!isLast && (
                <span className="absolute top-3 bottom-0 w-px bg-border-soft" />
              )}
              <span
                className={`relative z-10 mt-[7px] h-1.5 w-1.5 rounded-full ${dotClass[n.tone]}`}
              />
            </span>
            <div className="min-w-0 flex-1">
              {n.hash ? (
                <ExpandableArtifact hash={n.hash} summary={line} />
              ) : (
                <div className="py-1">{line}</div>
              )}
            </div>
          </li>
        );
      })}
    </ol>
  );
}
