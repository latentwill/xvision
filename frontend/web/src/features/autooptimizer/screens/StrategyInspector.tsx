import { useState } from "react";
import { useNavigate, useParams } from "react-router-dom";
import { Topbar } from "@/components/shell/Topbar";
import {
  useBlob,
  useLineageNode,
  promoteStrategy,
  formatGateVerdict,
  type StrategyBlob,
} from "../api";
import { Breadcrumb } from "../ui/Breadcrumb";
import { HashSigil } from "../ui/HashSigil";
import { GateBadge } from "../ui/GateBadge";
import { ParentDiffPanel } from "../panels/ParentDiffPanel";
import { OriginDiffPanel } from "../panels/OriginDiffPanel";

function SectionHeader({ title }: { title: string }) {
  return (
    <h2 className="m-0 text-[15px] font-semibold tracking-tight">{title}</h2>
  );
}

/** Extract a display name from the blob's manifest field if present. */
function blobDisplayName(blob: StrategyBlob | undefined): string | null {
  if (!blob) return null;
  const manifest = blob.manifest;
  if (manifest && typeof manifest === "object" && !Array.isArray(manifest)) {
    const m = manifest as Record<string, unknown>;
    if (typeof m.display_name === "string" && m.display_name.trim()) {
      return m.display_name;
    }
  }
  return null;
}

export function StrategyInspector() {
  const { hash = "" } = useParams<{ hash: string }>();
  const navigate = useNavigate();
  const [promoting, setPromoting] = useState(false);
  const [promoteError, setPromoteError] = useState<string | null>(null);

  const { data: blob, isLoading: blobLoading, isError: blobError } = useBlob(hash);
  const { data: node, isLoading: nodeLoading } = useLineageNode(hash);

  const displayName = blobDisplayName(blob);

  const handlePromote = async () => {
    setPromoting(true);
    setPromoteError(null);
    try {
      await promoteStrategy(hash);
      navigate("/strategies");
    } catch (e) {
      console.error("promote failed", e);
      setPromoteError("Promote failed. Please try again.");
    } finally {
      setPromoting(false);
    }
  };

  const isLoading = blobLoading || nodeLoading;

  return (
    <>
      <Topbar
        title="Optimizer"
        sub="Strategy inspector"
        back={{ to: "/optimizer", label: "Back to Optimizer" }}
      />
      <div className="space-y-5">
        <Breadcrumb
          items={[
            { label: "OPTIMIZER", to: "/optimizer" },
            { label: "strategy" },
            { label: hash.slice(0, 10) },
          ]}
        />

        {isLoading ? (
          <p className="text-[12px] text-text-3">Loading strategy…</p>
        ) : blobError ? (
          <p className="text-[12px] text-danger">Couldn't load this strategy.</p>
        ) : (
          <>
            {/* ── Hero card ──────────────────────────────────────────────────── */}
            <section className="flex items-start gap-4 rounded-md border border-border bg-surface-card p-5">
              <HashSigil hash={hash} size={72} />
              <div className="min-w-0 flex-1">
                <div className="mb-1 flex items-center gap-2">
                  <span className="text-[8.5px] uppercase tracking-widest text-text-3">
                    Optimizer · Strategy
                  </span>
                  {node && (
                    <GateBadge
                      verdict={formatGateVerdict(node.gate_verdict)}
                      status={node.status}
                    />
                  )}
                </div>
                <h1 className="m-0 font-mono text-[22px] tracking-tight">
                  {displayName ?? hash.slice(0, 16)}
                </h1>
                {displayName && (
                  <p className="mt-0.5 font-mono text-[11px] text-text-3">{hash.slice(0, 16)}</p>
                )}
                {node && (
                  <p className="mt-1 font-mono text-[11px] text-text-3">
                    parent {node.parent_hash ? node.parent_hash.slice(0, 10) : "— (root)"}
                    {node.cycle_id ? ` · cycle ${node.cycle_id}` : ""}
                  </p>
                )}
              </div>
            </section>

            {/* ── Raw strategy content ───────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="Strategy content" />
              <pre className="overflow-auto rounded border border-border bg-surface-muted p-3 font-mono text-[11px] text-text-2 max-h-80">
                {blob ? JSON.stringify(blob, null, 2) : "—"}
              </pre>
            </section>

            {/* ── Lineage diffs ──────────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-5">
              <SectionHeader title="Lineage" />
              <div className="space-y-1">
                <h3 className="m-0 text-[13px] font-medium text-text-2">Diff from parent</h3>
                <ParentDiffPanel childHash={hash} parentHash={node?.parent_hash} />
              </div>
              <div className="space-y-1">
                <h3 className="m-0 text-[13px] font-medium text-text-2">
                  Diff from originating strategy
                </h3>
                <OriginDiffPanel hash={hash} />
              </div>
            </section>

            {/* ── Promote action ─────────────────────────────────────────────── */}
            <section className="rounded-md border border-border bg-surface-card p-5 space-y-3">
              <SectionHeader title="Actions" />
              <div className="flex items-center gap-4">
                <button
                  type="button"
                  className="rounded border border-border bg-surface-elev px-3 py-1.5 text-[12px] font-medium text-text-1 hover:bg-surface-muted disabled:opacity-50"
                  onClick={handlePromote}
                  disabled={promoting}
                >
                  {promoting ? "Promoting…" : "Promote to Eval"}
                </button>
                {promoteError && (
                  <p className="text-[12px] text-danger">{promoteError}</p>
                )}
              </div>
              <p className="text-[11px] text-text-3">
                Promote this optimizer-produced strategy to the eval queue for full backtesting.
              </p>
            </section>
          </>
        )}
      </div>
    </>
  );
}
