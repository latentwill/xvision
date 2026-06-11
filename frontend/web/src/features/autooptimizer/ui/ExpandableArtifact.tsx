import { useState, type ReactNode } from "react";
import { Link } from "react-router-dom";
import { useExperimentDetail } from "../api";
import { GateScorecard } from "../panels/GateScorecard";
import { FindingsList } from "../panels/FindingsList";
import { ParentDiffPanel } from "../panels/ParentDiffPanel";
import { RegimeCards } from "../panels/RegimeCards";

export function ExpandableArtifact({
  hash,
  summary,
  defaultOpen = false,
  writerModel,
  open: controlledOpen,
  onToggle,
}: {
  hash: string;
  summary: ReactNode;
  defaultOpen?: boolean;
  writerModel?: string | null;
  /** Controlled open state. When provided (with onToggle), the internal
   *  useState path is bypassed — used by ExperimentBoard to lift state. */
  open?: boolean;
  onToggle?: () => void;
}) {
  const [uncontrolledOpen, setUncontrolledOpen] = useState(defaultOpen);
  const isControlled = controlledOpen !== undefined;
  const open = isControlled ? controlledOpen : uncontrolledOpen;
  const toggle = isControlled
    ? () => onToggle?.()
    : () => setUncontrolledOpen((v) => !v);

  return (
    <div className="rounded-sm border border-border bg-surface-card">
      <button
        type="button"
        aria-expanded={open}
        onClick={toggle}
        className="flex w-full items-center justify-between gap-3 px-3 py-2 text-left text-[12px] hover:bg-surface-elev"
      >
        <span className="min-w-0 truncate">{summary}</span>
        <span className="text-text-4">{open ? "−" : "+"}</span>
      </button>
      {open && <ArtifactBody hash={hash} writerModel={writerModel} />}
    </div>
  );
}

function ArtifactBody({
  hash,
  writerModel,
}: {
  hash: string;
  writerModel?: string | null;
}) {
  const detail = useExperimentDetail(hash);

  if (detail.isLoading) {
    return (
      <div className="px-3 py-3 text-[12px] text-text-3">
        Loading experiment…
      </div>
    );
  }

  if (detail.isError || !detail.data) {
    return (
      <div className="px-3 py-3 text-[12px] text-text-3">
        Artifact not available on this backend.
      </div>
    );
  }

  const d = detail.data;
  const parentHash = d.lineage_node.parent_hash ?? null;

  return (
    <div className="space-y-3 border-t border-border px-3 py-3">
      {writerModel && (
        <div className="font-mono text-[11px] text-text-3">
          Writer: <span className="text-text-2">{writerModel}</span>
        </div>
      )}

      {d.rationale && (
        <div>
          <div className="text-[10px] uppercase tracking-widest text-text-4">
            Why tested
          </div>
          <p className="mt-1 text-[12.5px] text-text-2">{d.rationale}</p>
        </div>
      )}

      {d.gate_record && <GateScorecard gate_record={d.gate_record} />}

      <ParentDiffPanel childHash={hash} parentHash={parentHash} />

      <RegimeCards results={d.regime_results} />

      {d.findings.length > 0 && <FindingsList findings={d.findings} />}

      <p className="text-[10.5px] text-text-4">
        Full prompt/response transcripts aren't persisted yet.
      </p>

      <Link
        to={`/optimizer/strategy/${hash}`}
        className="inline-block text-[11px] text-gold hover:underline"
      >
        Open strategy →
      </Link>
    </div>
  );
}
