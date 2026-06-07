import { useState } from "react";
import type { ExperimentFinding } from "../api";

const DETAIL_CHAR_LIMIT = 180;

const SEVERITY_BADGE: Record<ExperimentFinding["severity"], string> = {
  info: "text-blue-400 border-blue-400/40 bg-blue-400/[0.10]",
  warn: "text-amber-400 border-amber-400/40 bg-amber-400/[0.10]",
  risk: "text-red-400 border-red-400/40 bg-red-400/[0.10]",
};

function FindingCard({ finding }: { finding: ExperimentFinding }) {
  const [expanded, setExpanded] = useState(false);
  const { severity, code, summary, detail, model } = finding;

  const isLong = detail != null && detail.length > DETAIL_CHAR_LIMIT;
  const visibleDetail =
    detail == null
      ? null
      : isLong && !expanded
        ? detail.slice(0, DETAIL_CHAR_LIMIT) + "…"
        : detail;

  return (
    <div className="rounded-md border border-border bg-surface-card p-4 space-y-2">
      <div className="flex items-start gap-2">
        <span
          className={`inline-flex shrink-0 items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-wide border ${SEVERITY_BADGE[severity]}`}
        >
          {severity}
        </span>
        <div className="min-w-0 flex-1">
          <span className="font-mono text-[12px] font-semibold text-text-1">{code}</span>
          <span className="mx-1.5 text-text-3">·</span>
          <span className="text-[12px] text-text-2">{summary}</span>
        </div>
      </div>

      {visibleDetail && (
        <p className="text-[12px] text-text-3 leading-relaxed">{visibleDetail}</p>
      )}

      {isLong && (
        <button
          type="button"
          onClick={() => setExpanded((v) => !v)}
          className="text-[11px] text-text-3 hover:text-text-2 transition-colors"
          aria-label={expanded ? "Collapse" : "Show more"}
        >
          {expanded ? "Show less" : "Show more"}
        </button>
      )}

      {model && (
        <p className="text-[10px] text-text-3 font-mono">{model}</p>
      )}
    </div>
  );
}

export function FindingsList({ findings }: { findings: ExperimentFinding[] }) {
  if (findings.length === 0) {
    return (
      <p className="text-[12px] text-text-3">No reviewer notes for this experiment</p>
    );
  }

  return (
    <div className="space-y-2">
      {findings.map((f) => (
        <FindingCard key={f.id} finding={f} />
      ))}
    </div>
  );
}
