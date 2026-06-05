import type { LineageStatus } from "../api";

type Bucket = "Kept" | "Suspect" | "Dropped" | "Pending";

function bucketOf(verdict: string, status?: LineageStatus): Bucket {
  if (status === "quarantined" || verdict === "Suspect") return "Suspect";
  if (status === "active" || verdict === "Accepted") return "Kept";
  if (status === "rejected" || verdict === "Rejected") return "Dropped";
  return "Pending";
}

const STYLE: Record<Bucket, string> = {
  Kept: "text-gold border-gold/40 bg-gold/[0.10]",
  Suspect: "text-warn border-warn/40 bg-warn/[0.10]",
  Dropped: "text-danger border-danger/40 bg-danger/[0.10]",
  Pending: "text-text-3 border-border bg-surface-elev",
};

export function GateBadge({
  verdict,
  status,
}: {
  verdict: string;
  status?: LineageStatus;
}) {
  const bucket = bucketOf(verdict, status);
  return (
    <span
      className={`inline-flex items-center rounded px-1.5 py-0.5 font-mono text-[10px] font-semibold uppercase tracking-wide border ${STYLE[bucket]}`}
    >
      {bucket}
    </span>
  );
}
