/**
 * InsightLog — collapsible right rail listing annotations as cards.
 *
 * Expanded: 280px column with one card per annotation. Each card has a
 * 2px left accent bar (gold or red), Cormorant title, mono timestamp,
 * 11.5px body, category pill + confidence footer.
 *
 * Collapsed: 36px rail showing vertical "Insight Log · N events" text
 * + a column of N small accent-colored dots. Click `›` to expand, `‹`
 * to collapse. State is controlled by the parent.
 */
import type { ReactElement } from "react";

import type { Annotation } from "../types";

export interface InsightLogProps {
  annotations: Annotation[];
  /** Filter applied to the visible list; same set as AnnotationOverlay. */
  visibleTypes?: ReadonlySet<Annotation["type"]>;
  /** Controlled expand/collapse. */
  open: boolean;
  onToggle: () => void;
}

function fmtTimestamp(ts: number | undefined): string {
  if (!ts) return "—";
  const d = new Date(ts * 1000);
  const hh = String(d.getUTCHours()).padStart(2, "0");
  const mm = String(d.getUTCMinutes()).padStart(2, "0");
  return `${hh}:${mm}`;
}

function fmtConfPct(conf: number): string {
  return `${Math.round(conf * 100)}%`;
}

export function InsightLog({
  annotations,
  visibleTypes,
  open,
  onToggle,
}: InsightLogProps): ReactElement {
  const visible = visibleTypes
    ? annotations.filter((a) => visibleTypes.has(a.type))
    : annotations;

  if (!open) {
    return (
      <aside
        className="border-l border-border bg-surface-card flex flex-col items-center justify-between py-3 px-1"
        style={{ width: 36 }}
        data-testid="insight-log-collapsed"
      >
        <button
          type="button"
          className="text-text-3 hover:text-text px-1 py-0.5"
          onClick={onToggle}
          aria-label="Open insight log"
        >
          ‹
        </button>
        <div
          className="caps"
          style={{
            writingMode: "vertical-rl",
            transform: "rotate(180deg)",
            letterSpacing: "0.12em",
          }}
        >
          Insight Log · {visible.length} events
        </div>
        <div className="flex flex-col items-center gap-1.5 pb-2">
          {visible.slice(0, 6).map((a) => (
            <span
              key={a.idx}
              className="block w-1.5 h-1.5 rounded-full"
              style={{
                backgroundColor: a.danger
                  ? "var(--danger)"
                  : "var(--gold)",
              }}
              aria-hidden="true"
            />
          ))}
        </div>
      </aside>
    );
  }

  return (
    <aside
      className="border-l border-border bg-surface-card flex flex-col"
      style={{ width: 280 }}
      data-testid="insight-log-open"
    >
      <header className="flex items-center justify-between px-3 py-2 border-b border-border-soft">
        <div className="caps">Insight Log · {visible.length} events</div>
        <button
          type="button"
          className="text-text-3 hover:text-text px-1 py-0.5"
          onClick={onToggle}
          aria-label="Close insight log"
        >
          ›
        </button>
      </header>
      <ul className="flex-1 overflow-auto divide-y divide-border-soft">
        {visible.map((a) => {
          const accent = a.danger ? "var(--danger)" : "var(--gold)";
          return (
            <li
              key={a.idx}
              className="relative pl-4 pr-3 py-3 hover:bg-surface-hover transition-colors"
            >
              <span
                aria-hidden="true"
                className="absolute left-0 top-2 bottom-2 w-[2px] rounded-r"
                style={{ backgroundColor: accent }}
              />
              <div className="flex items-baseline justify-between gap-2">
                <div
                  className="text-[14px] leading-tight text-text"
                  style={{ fontFamily: '"Cormorant Garamond", serif' }}
                >
                  {a.title}
                </div>
                <span
                  className="text-[10.5px] text-text-3 shrink-0"
                  style={{ fontFamily: '"JetBrains Mono", monospace' }}
                >
                  {fmtTimestamp(a.ts)}
                </span>
              </div>
              <p className="mt-1 text-[11.5px] leading-snug text-text-2">{a.body}</p>
              <div className="mt-1 flex items-center justify-between text-[10.5px] text-text-3">
                <span className="caps" style={{ color: accent }}>
                  {a.type}
                </span>
                <span style={{ fontFamily: '"JetBrains Mono", monospace' }}>
                  conf {fmtConfPct(a.conf)}
                </span>
              </div>
            </li>
          );
        })}
        {visible.length === 0 && (
          <li className="px-3 py-6 text-[12px] text-text-3 text-center">
            No annotations match the current filter.
          </li>
        )}
      </ul>
    </aside>
  );
}
