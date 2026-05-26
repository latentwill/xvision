import { useEffect, useRef, useState } from "react";

import { useRunStream } from "@/components/chart/use-run-stream";

import { runChartPayloadToV2 } from "../adapters/run-chart-payload";
import type { LiveChartV2Payload } from "../types";
import { LiveChartV2 } from "./LiveChartV2";

/**
 * Live container for the v2 chart surface.
 *
 * Owns the proven `useRunStream(runId)` SSE hook (snapshot + merge-on-event,
 * reconnect, terminal close), adapts each streamed RunChartPayload to the
 * columnar v2 payload via `runChartPayloadToV2`, and renders `LiveChartV2`
 * with follow/freeze/resume controls — reproducing the v1 `LiveChart`
 * behavior (Following live / Frozen / Resume live, follow reset on runId
 * change). The surface re-adapts on every stream tick, so the chart updates
 * live without LiveChartV2 needing its own streaming hook.
 */
export function LiveChartV2Container({ runId }: { runId: string }) {
  const { data, status } = useRunStream(runId);
  const [follow, setFollow] = useState(true);
  const prevRunId = useRef(runId);
  // Until the runId-change effect runs, treat a changed runId as following so
  // a stale `follow=false` from the previous run never sticks to a new run.
  const effectiveFollow = prevRunId.current === runId ? follow : true;

  useEffect(() => {
    if (prevRunId.current === runId) return;
    prevRunId.current = runId;
    setFollow(true);
  }, [runId]);

  return (
    <div>
      <label className="flex items-center gap-2 mb-2 text-[12px] text-text-2">
        <input
          type="checkbox"
          checked={effectiveFollow}
          onChange={(e) => setFollow(e.target.checked)}
        />
        {effectiveFollow ? "Following live" : "Frozen"}
        {!effectiveFollow && (
          <button
            type="button"
            onClick={() => setFollow(true)}
            className="ml-2 underline"
          >
            Resume live
          </button>
        )}
      </label>
      {data ? (
        <LiveChartV2 payload={toLivePayload(data, status)} follow={effectiveFollow} />
      ) : (
        <div className="text-text-3 py-12 text-center">
          Waiting for first event…
        </div>
      )}
    </div>
  );
}

function connectionFor(status: string): LiveChartV2Payload["connection"] {
  if (status === "streaming") return "connected";
  if (status === "closed") return "offline";
  return "reconnecting"; // snapshot | reconnecting
}

function toLivePayload(
  data: Parameters<typeof runChartPayloadToV2>[0],
  status: string,
): LiveChartV2Payload {
  const v2 = runChartPayloadToV2(data);
  return {
    kind: "live",
    asset: v2.asset,
    granularity: v2.granularity,
    candles: v2.candles,
    equity: v2.equity,
    markers: v2.markers,
    live_index: Math.max(0, v2.candles.time.length - 1),
    connection: connectionFor(status),
    cache: "fresh",
  };
}
