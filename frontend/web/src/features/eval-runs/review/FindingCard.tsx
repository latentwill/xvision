import { Pill } from "@/components/primitives/Pill";
import type { ReviewFinding } from "@/api/eval-review";

/// Map a review-finding severity to the existing Pill palette. Review
/// findings use four severities (`low`, `medium`, `high`, `critical`)
/// while the v1 extractor used three; we collapse the legacy v1
/// `info` / `warning` rows onto the same toned pills so the panel
/// renders both shapes consistently.
function severityTone(severity: string): "default" | "info" | "warn" | "danger" {
  switch (severity) {
    case "critical":
      return "danger";
    case "high":
      return "danger";
    case "medium":
      return "warn";
    case "low":
      return "info";
    // v1 fallthroughs
    case "warning":
      return "warn";
    case "info":
      return "info";
    default:
      return "default";
  }
}

export function FindingCard({ finding }: { finding: ReviewFinding }) {
  // v2 review findings store `type` + `title` + `description` +
  // `recommendation`; v1 extractor rows only have `kind` + `summary` +
  // `evidence`. Fall through to the v1 fields when v2 is absent.
  const kind = finding.type ?? finding.kind;
  const title = finding.title ?? finding.summary;
  const description = finding.description;
  const recommendation = finding.recommendation;
  const confidence = finding.confidence;
  const evidence = finding.evidence as
    | Array<{ kind?: string; reference?: string }>
    | undefined;

  return (
    <div className="border border-border rounded-card p-4 bg-surface-card">
      <div className="flex items-start justify-between gap-3 mb-2">
        <div className="flex flex-wrap items-center gap-2">
          <Pill tone={severityTone(finding.severity as string)}>{kind}</Pill>
          <Pill>{finding.severity}</Pill>
          {typeof confidence === "number" && (
            <span className="text-text-3 text-[11px]">
              confidence {confidence.toFixed(2)}
            </span>
          )}
        </div>
      </div>
      <div className="text-text text-[14px] font-medium mb-1">{title}</div>
      {description && (
        <div className="text-text-2 text-[13px] mb-2 whitespace-pre-line">
          {description}
        </div>
      )}
      {recommendation && (
        <div className="text-text-2 text-[13px] mb-2">
          <span className="text-gold mr-1">→</span>
          {recommendation}
        </div>
      )}
      {Array.isArray(evidence) && evidence.length > 0 && (
        <div className="flex flex-wrap gap-1.5 mt-2">
          {evidence.map((e, i) => (
            <code
              key={i}
              className="text-[11px] px-1.5 py-0.5 rounded-sm border border-border text-text-3 font-mono"
            >
              {e.reference ?? "?"}
            </code>
          ))}
        </div>
      )}
    </div>
  );
}
