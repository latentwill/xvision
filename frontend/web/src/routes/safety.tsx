// /safety — global safety gate status + audit log.
//
// Shows the current pause state inline (no popups). Operators can pause or
// resume from here. Below the state card, the last 50 audit rows are listed
// in a table. Everything is inline — no sheets, modals, or popovers.

import { useState } from "react";
import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { Topbar } from "@/components/shell/Topbar";
import { Card } from "@/components/primitives/Card";
import { Pill } from "@/components/primitives/Pill";
import {
  getSafetyAudit,
  getSafetyState,
  pauseSafety,
  resumeSafety,
  safetyKeys,
} from "@/api/safety";

export function SafetyRoute() {
  const qc = useQueryClient();
  const stateQ = useQuery({
    queryKey: safetyKeys.state(),
    queryFn: getSafetyState,
    refetchInterval: 15_000,
  });
  const auditQ = useQuery({
    queryKey: safetyKeys.audit(50),
    queryFn: () => getSafetyAudit(50),
    refetchInterval: 30_000,
  });

  const [reasonInput, setReasonInput] = useState("");

  const pauseMut = useMutation({
    mutationFn: () => pauseSafety({ reason: reasonInput || null }),
    onSuccess: () => {
      setReasonInput("");
      qc.invalidateQueries({ queryKey: safetyKeys.all });
    },
  });
  const resumeMut = useMutation({
    mutationFn: () => resumeSafety({ reason: reasonInput || null }),
    onSuccess: () => {
      setReasonInput("");
      qc.invalidateQueries({ queryKey: safetyKeys.all });
    },
  });

  const paused = stateQ.data?.paused ?? false;

  return (
    <div className="max-w-4xl mx-auto">
      <Topbar title="Safety" sub="Global broker submit gate and audit log." />

      {/* State card */}
      <Card className="mb-6">
        <div className="px-5 py-4 flex flex-col gap-4">
          <div className="flex items-center gap-3">
            <span className="text-[13px] font-medium text-text-2">Status</span>
            {stateQ.isPending ? (
              <Pill>loading…</Pill>
            ) : paused ? (
              <Pill
                tone="danger"
                data-testid="safety-state-paused"
              >
                <span className="w-1.5 h-1.5 rounded-full bg-danger" />
                paused
              </Pill>
            ) : (
              <Pill
                tone="gold"
                data-testid="safety-state-running"
              >
                <span className="w-1.5 h-1.5 rounded-full bg-gold" />
                running
              </Pill>
            )}
          </div>

          {stateQ.data?.paused && stateQ.data.reason ? (
            <p className="text-[13px] text-text-2 m-0">
              <span className="font-medium">Reason:</span>{" "}
              {stateQ.data.reason}
            </p>
          ) : null}

          {stateQ.data?.paused_by ? (
            <p className="text-[12px] text-text-3 m-0">
              Last toggled by {stateQ.data.paused_by}
              {stateQ.data.paused_at
                ? ` at ${new Date(stateQ.data.paused_at).toLocaleString()}`
                : ""}
            </p>
          ) : null}

          {/* Pause / Resume controls */}
          <div className="flex flex-col sm:flex-row gap-2 items-start sm:items-center">
            <input
              type="text"
              value={reasonInput}
              onChange={(e) => setReasonInput(e.target.value)}
              placeholder="Reason (optional)"
              className="px-3 py-1.5 bg-surface-elev border border-border rounded text-[13px] text-text w-full sm:w-64 focus:outline-none focus:border-text-3"
              data-testid="safety-reason-input"
            />
            {paused ? (
              <button
                type="button"
                onClick={() => resumeMut.mutate()}
                disabled={resumeMut.isPending}
                className="px-4 py-1.5 bg-gold text-bg text-[13px] font-medium rounded hover:opacity-90 disabled:opacity-50 transition-opacity motion-safe:active:scale-[0.96]"
                data-testid="safety-resume-btn"
              >
                {resumeMut.isPending ? "Resuming…" : "Resume"}
              </button>
            ) : (
              <button
                type="button"
                onClick={() => pauseMut.mutate()}
                disabled={pauseMut.isPending}
                className="px-4 py-1.5 bg-danger text-bg text-[13px] font-medium rounded hover:opacity-90 disabled:opacity-50 transition-opacity"
                data-testid="safety-pause-btn"
              >
                {pauseMut.isPending ? "Pausing…" : "Pause"}
              </button>
            )}
          </div>

          {(pauseMut.isError || resumeMut.isError) ? (
            <p className="text-[13px] text-danger m-0">
              {String((pauseMut.error ?? resumeMut.error) as Error)}
            </p>
          ) : null}
        </div>
      </Card>

      {/* Audit log */}
      <h2 className="text-[15px] font-medium text-text mb-3">Audit log (last 50)</h2>
      <Card>
        {auditQ.isPending ? (
          <div className="px-5 py-4 text-[13px] text-text-3">Loading…</div>
        ) : auditQ.isError ? (
          <div className="px-5 py-4 text-[13px] text-danger">
            Failed to load audit log.
          </div>
        ) : (auditQ.data?.length ?? 0) === 0 ? (
          <div className="px-5 py-4 text-[13px] text-text-3">No audit rows yet.</div>
        ) : (
          <div className="overflow-x-auto">
            <table className="w-full text-[12px]">
              <thead>
                <tr className="border-b border-border text-text-3">
                  <th className="px-4 py-2 text-left font-medium">Time</th>
                  <th className="px-4 py-2 text-left font-medium">Action</th>
                  <th className="px-4 py-2 text-left font-medium">Result</th>
                  <th className="px-4 py-2 text-left font-medium">User</th>
                  <th className="px-4 py-2 text-left font-medium">Paused?</th>
                </tr>
              </thead>
              <tbody>
                {auditQ.data!.map((row) => (
                  <tr
                    key={row.id}
                    className="border-b border-border last:border-0 hover:bg-surface-elev/40 transition-colors"
                    data-testid="audit-row"
                  >
                    <td className="px-4 py-2 text-text-3 whitespace-nowrap">
                      {new Date(row.timestamp).toLocaleString()}
                    </td>
                    <td className="px-4 py-2 text-text-2 whitespace-nowrap">
                      {row.action_kind}
                    </td>
                    <td className="px-4 py-2 whitespace-nowrap">
                      <ResultPill result={row.result} />
                    </td>
                    <td className="px-4 py-2 text-text-3 whitespace-nowrap">
                      {row.user || "—"}
                    </td>
                    <td className="px-4 py-2 text-text-3 whitespace-nowrap">
                      {row.pause_state_at_time ? "yes" : "no"}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </Card>
    </div>
  );
}

function ResultPill({ result }: { result: string }) {
  if (result === "allowed") {
    return <Pill tone="gold">{result}</Pill>;
  }
  if (result.startsWith("denied")) {
    return <Pill tone="danger">{result}</Pill>;
  }
  return <Pill tone="warn">{result}</Pill>;
}
