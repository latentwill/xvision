import { useEffect, useRef, useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { openRunStream, chartKeys, getRunChart } from "@/api/chart";
import type {
  ChartBar,
  ChartEquityPoint,
  HoldMarker,
  RunChartPayload,
  TradeMarker,
  VetoMarker,
} from "@/api/types.gen";

export type LiveStatus = "snapshot" | "streaming" | "reconnecting" | "closed";

// Wire-format event shapes (no ts-rs equivalent on the Rust side — these
// must stay in lockstep with `RunChartEvent` in
// crates/xvision-engine/src/api/chart.rs).
//
// The `event` tag values are snake_case, matching the Rust `RunChartEvent`
// `rename_all = "snake_case"` and the SSE frame name from `event_name()` in
// eval_runs.rs — so the inner discriminant and the SSE listener name agree.
type WireMarker =
  | ({ kind: "trade" } & TradeMarker)
  | ({ kind: "veto" } & VetoMarker)
  | ({ kind: "hold" } & HoldMarker);

type WireEvent =
  | { event: "bar"; data: ChartBar }
  | { event: "equity"; data: ChartEquityPoint }
  | { event: "marker"; data: WireMarker }
  | { event: "status"; data: { phase: string; message: string | null } }
  | {
      event: "indicator_tail";
      data: Record<string, { time: number; value: number }>;
    };

const RECONNECT_DELAY_MS = 1000;

export function useRunStream(runId: string, initial?: RunChartPayload) {
  const qc = useQueryClient();
  const [data, setData] = useState<RunChartPayload | undefined>(initial);
  const [status, setStatus] = useState<LiveStatus>(
    initial ? "streaming" : "snapshot",
  );
  const esRef = useRef<EventSource | null>(null);
  const dataRef = useRef<RunChartPayload | undefined>(initial);

  // Keep ref in sync with state so the SSE handlers can read the latest
  // payload without forcing a re-subscribe.
  useEffect(() => {
    dataRef.current = data;
  }, [data]);

  const mergeBar = useCallback((bar: ChartBar) => {
    setData((prev) => (prev ? { ...prev, bars: [...prev.bars, bar] } : prev));
  }, []);

  const mergeEquity = useCallback((point: ChartEquityPoint) => {
    setData((prev) =>
      prev ? { ...prev, equity: [...prev.equity, point] } : prev,
    );
  }, []);

  const mergeMarker = useCallback((m: WireMarker) => {
    setData((prev) => {
      if (!prev) return prev;
      const markers = { ...prev.markers };
      if (m.kind === "trade") {
        const { kind: _trade, ...trade } = m;
        markers.trades = [...markers.trades, trade];
      } else if (m.kind === "veto") {
        const { kind: _veto, ...veto } = m;
        markers.vetoes = [...markers.vetoes, veto];
      } else if (m.kind === "hold") {
        const { kind: _hold, ...hold } = m;
        markers.holds = [...markers.holds, hold];
      }
      return { ...prev, markers };
    });
  }, []);

  useEffect(() => {
    if (!runId) return;
    let cancelled = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;

    async function snapshot() {
      try {
        const p = await getRunChart(runId);
        if (cancelled) return;
        setData(p);
        setStatus("streaming");
        qc.setQueryData(chartKeys.run(runId), p);
      } catch {
        if (cancelled) return;
        setStatus("closed");
      }
    }

    function openStream() {
      const es = openRunStream(runId);
      esRef.current = es;

      es.addEventListener("bar", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "bar") mergeBar(parsed.data);
      });
      es.addEventListener("equity", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "equity") mergeEquity(parsed.data);
      });
      es.addEventListener("marker", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "marker") mergeMarker(parsed.data);
      });
      es.addEventListener("status", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "status") {
          const phase = parsed.data.phase;
          if (
            phase === "completed" ||
            phase === "failed" ||
            phase === "cancelled"
          ) {
            es.close();
            esRef.current = null;
            setStatus("closed");
          }
        }
      });
      es.onerror = () => {
        if (cancelled) return;
        setStatus("reconnecting");
        es.close();
        esRef.current = null;
        reconnectTimer = setTimeout(() => {
          if (cancelled) return;
          snapshot().then(() => {
            if (cancelled) return;
            openStream();
          });
        }, RECONNECT_DELAY_MS);
      };
    }

    if (!dataRef.current) {
      snapshot().then(() => {
        if (cancelled) return;
        openStream();
      });
    } else {
      openStream();
    }

    return () => {
      cancelled = true;
      if (reconnectTimer) clearTimeout(reconnectTimer);
      esRef.current?.close();
      esRef.current = null;
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [runId]);

  return { data, status };
}
