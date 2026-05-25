// "Improve this agent" affordance (Phase 3.7).
//
// Renders on the agent detail surface. Lists optimization runs that target
// this agent and links each to its routed run-detail view. Operator-friendly
// wording ("Improve this agent") — the optimizer name (MIPRO/GEPA) is only
// shown as a small muted tag per row, with the full internals living behind
// the "Advanced detail" toggle on the run-detail view itself.
//
// No popups: this is an inline card with router <Link>s.

import { useQuery } from "@tanstack/react-query";
import { Link } from "react-router-dom";

import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import {
  listOptimizations,
  optimizationKeys,
  type OptimizationRun,
} from "@/api/optimizations";

const INFLIGHT = new Set(["pending", "running"]);

function tone(status: string): "info" | "warn" | "danger" | "default" {
  if (status === "completed") return "info";
  if (status === "failed") return "danger";
  if (INFLIGHT.has(status)) return "warn";
  return "default";
}

export function ImproveAgentPanel({ agentId }: { agentId: string }) {
  const q = useQuery<OptimizationRun[]>({
    queryKey: optimizationKeys.list(agentId),
    queryFn: () => listOptimizations(agentId),
    enabled: agentId.length > 0,
  });

  const runs = q.data ?? [];

  return (
    <Card className="mt-6 mb-10" data-testid="improve-agent-panel">
      <div className="px-5 pt-4 pb-2 flex items-center justify-between">
        <h2 className="m-0 text-[15px] font-medium">Improve this agent</h2>
        <span className="text-[12px] text-text-3">
          prompt optimization runs
        </span>
      </div>

      {q.isLoading ? (
        <div className="px-5 pb-4 text-[13px] text-text-3">
          Loading optimization runs…
        </div>
      ) : runs.length === 0 ? (
        <div className="px-5 pb-5 text-[13px] text-text-2">
          No optimization runs yet for this agent. Kick one off from the CLI
          (<code className="font-mono text-text-3">xvn optimize</code>) to
          improve a slot&apos;s prompt against a scenario corpus; results show
          up here for review.
        </div>
      ) : (
        <ul className="px-5 pb-4">
          {runs.map((r) => (
            <li
              key={r.id}
              className="flex items-center gap-3 py-2 border-b border-border last:border-b-0"
              data-testid={`improve-run-${r.id}`}
            >
              <Pill tone={tone(r.status)}>{r.status}</Pill>
              <span className="text-[13px] text-text">
                slot <span className="font-medium">{r.slot_name}</span>
              </span>
              <span className="text-[11px] text-text-3 uppercase tracking-wide">
                {r.optimizer}
              </span>
              <Link
                to={`/agents/${encodeURIComponent(agentId)}/optimizations/${encodeURIComponent(r.id)}`}
                className="ml-auto text-[13px] text-accent hover:underline"
                data-testid={`improve-run-link-${r.id}`}
              >
                Review →
              </Link>
            </li>
          ))}
        </ul>
      )}
    </Card>
  );
}
