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
// RunChartEvent uses #[serde(tag = "event", content = "data")] with no
// rename_all, so tag values are PascalCase. The SSE frame names (from
// event_name() in eval_runs.rs) are lowercase. The browser fires
// addEventListener('bar', ...) on `event: bar` SSE frames, and e.data
// contains the full serialised RunChartEvent: {"event":"Bar","data":{...}}.
type WireMarker =
  | ({ kind: "trade" } & TradeMarker)
  | ({ kind: "veto" } & VetoMarker)
  | ({ kind: "hold" } & HoldMarker);

type WireEvent =
  | { event: "Bar"; data: ChartBar }
  | { event: "Equity"; data: ChartEquityPoint }
  | { event: "Marker"; data: WireMarker }
  | { event: "Status"; data: { phase: string; message: string | null } }
  | {
      event: "IndicatorTail";
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
        const { kind: _tradeKind, ...trade } = m;
        void _tradeKind;
        markers.trades = [...markers.trades, trade];
      } else if (m.kind === "veto") {
        const { kind: _vetoKind, ...veto } = m;
        void _vetoKind;
        markers.vetoes = [...markers.vetoes, veto];
      } else if (m.kind === "hold") {
        const { kind: _holdKind, ...hold } = m;
        void _holdKind;
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
        if (parsed.event === "Bar") mergeBar(parsed.data);
      });
      es.addEventListener("equity", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "Equity") mergeEquity(parsed.data);
      });
      es.addEventListener("marker", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "Marker") mergeMarker(parsed.data);
      });
      es.addEventListener("status", (e) => {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        if (parsed.event === "Status") {
          const phase = parsed.data.phase;
          if (phase === "completed" || phase === "failed") {
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
