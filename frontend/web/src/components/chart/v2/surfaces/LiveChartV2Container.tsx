import { useEffect, useRef, useState } from "react";

import { useRunStream, type LiveStatus } from "@/components/chart/use-run-stream";
import type { RunChartPayload } from "@/api/types.gen";

import { runChartPayloadToV2 } from "../adapters/run-chart-payload";
import { EmptyState } from "../primitives";
import type { LiveChartV2Payload } from "../types";
import { LiveChartV2 } from "./LiveChartV2";

/**
 * Live container for the v2 chart surface.
 *
 * Adapts each streamed RunChartPayload to the columnar v2 payload via
 * `runChartPayloadToV2`, and renders `LiveChartV2` with follow/freeze/resume
 * controls — reproducing the v1 `LiveChart` behavior (Following live / Frozen
 * / Resume live, follow reset on runId change). The surface re-adapts on every
 * stream tick, so the chart updates live without LiveChartV2 needing its own
 * streaming hook.
 *
 * Stream source: when an injected `stream` is supplied (the live page lifts a
 * single `useRunStream(selectedId)` and shares it with the connection dot, the
 * account strip, the positions table, AND this container) the container
 * consumes it and opens NO EventSource of its own. When `stream` is omitted
 * (standalone use, e.g. tests), it falls back to owning the proven
 * `useRunStream(runId)` SSE hook — collapsing the previous duplicate
 * connection per selected run.
 */
export interface LiveChartV2ContainerProps {
  runId: string;
  /** Optional lifted stream; when present the container opens no SSE itself. */
  stream?: { data: RunChartPayload | undefined; status: LiveStatus };
}

export function LiveChartV2Container({ runId, stream }: LiveChartV2ContainerProps) {
  // Hooks must run unconditionally. When a stream is injected we still call the
  // hook but with an empty runId — its effect bails on `!runId` and opens no
  // EventSource, so there is no duplicate connection.
  const own = useRunStream(stream ? "" : runId);
  const { data, status } = stream ?? own;
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
        isEmptyLiveSnapshot(data) ? (
          <EmptyState
            title="No live chart data yet"
            message="This run is wired to the live chart feed, but no bars, equity points, or trade markers have arrived yet."
          />
        ) : (
          <LiveChartV2
            payload={toLivePayload(data, status)}
            follow={effectiveFollow}
          />
        )
      ) : (
        <div className="text-text-3 py-12 text-center">
          Waiting for first event…
        </div>
      )}
    </div>
  );
}

function isEmptyLiveSnapshot(data: RunChartPayload): boolean {
  return (
    data.bars.length === 0 &&
    data.equity.length === 0 &&
    data.markers.trades.length === 0 &&
    data.markers.vetoes.length === 0 &&
    data.markers.holds.length === 0
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
