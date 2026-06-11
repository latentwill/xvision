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
  const rows = events.slice(-maxItems);
  return (
    <ol className="space-y-1" aria-label="Cycle events">
      {rows.map((e, i) => {
        const n = narrateEvent(e);
        const line = (
          <span className="flex gap-3 font-mono text-[12px]">
            <span className="flex-none text-text-4">{fmtTime(e.ts)}</span>
            <span className={toneClass[n.tone]}>{n.sentence}</span>
          </span>
        );
        return (
          <li key={e.ts ? `${e.ts}-${i}` : i}>
            {n.hash ? (
              <ExpandableArtifact hash={n.hash} summary={line} />
            ) : (
              <div className="px-3 py-2">{line}</div>
            )}
          </li>
        );
      })}
    </ol>
  );
}
