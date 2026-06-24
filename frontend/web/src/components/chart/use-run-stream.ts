import { useEffect, useRef, useState, useCallback } from "react";
import { useQueryClient } from "@tanstack/react-query";

import { openRunStream, chartKeys, getRunChart } from "@/api/chart";
import {
  createTrace,
  durationSince,
  errorSummary,
} from "@/lib/logger";
import type {
  ChartBar,
  ChartEquityPoint,
  HoldMarker,
  IndicatorPoint,
  Indicators,
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

type WireDecision = {
  decision_index: number;
  timestamp: string;
  asset: string;
  action: string;
  conviction: number | null;
  justification: string | null;
  reasoning: string | null;
  order_size: number | null;
  fill_price: number | null;
  fill_size: number | null;
  fee: number | null;
  pnl_realized: number | null;
};

type WireEvent =
  | { event: "bar"; data: ChartBar }
  | { event: "equity"; data: ChartEquityPoint }
  | { event: "marker"; data: WireMarker }
  | { event: "decision"; data: WireDecision }
  | { event: "status"; data: { phase: string; message: string | null } }
  | {
      event: "indicator_tail";
      data: Record<string, { time: number; value: number }>;
    };

const RECONNECT_DELAY_MS = 1000;
const FLAT_INDICATOR_KEYS = [
  "sma_20",
  "sma_30",
  "sma_50",
  "sma_60",
  "sma_90",
  "sma_200",
  "ema_20",
  "ema_30",
  "ema_50",
  "ema_60",
  "ema_90",
  "ema_200",
  "rsi_14",
  "atr_14",
] as const;

type FlatIndicatorKey = (typeof FLAT_INDICATOR_KEYS)[number];
type IndicatorTail = Record<string, IndicatorPoint>;

function appendIndicatorTail(
  indicators: Indicators,
  tail: IndicatorTail,
): Indicators {
  let next = indicators;

  for (const key of FLAT_INDICATOR_KEYS) {
    const point = tail[key];
    if (!point) continue;
    next = {
      ...next,
      [key]: [...(next[key as FlatIndicatorKey] ?? []), point],
    };
  }

  const appendNested = <
    Section extends "bollinger" | "donchian" | "macd",
    Key extends keyof Indicators[Section] & string,
  >(
    section: Section,
    key: Key,
  ) => {
    const point = tail[`${section}.${key}`] ?? tail[`${section}_${key}`];
    if (!point) return;
    next = {
      ...next,
      [section]: {
        ...next[section],
        [key]: [...(next[section][key] as IndicatorPoint[]), point],
      },
    };
  };

  appendNested("bollinger", "upper");
  appendNested("bollinger", "middle");
  appendNested("bollinger", "lower");
  appendNested("donchian", "upper");
  appendNested("donchian", "lower");
  appendNested("macd", "line");
  appendNested("macd", "signal");
  appendNested("macd", "histogram");

  return next;
}

export function useRunStream(runId: string, initial?: RunChartPayload) {
  const qc = useQueryClient();
  const initialData = initial?.run_id === runId ? initial : undefined;
  const [data, setData] = useState<RunChartPayload | undefined>(initialData);
  const [status, setStatus] = useState<LiveStatus>(
    initialData ? "streaming" : "snapshot",
  );
  const esRef = useRef<EventSource | null>(null);
  const dataRef = useRef<RunChartPayload | undefined>(initialData);
  const runIdRef = useRef(runId);

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

  const mergeDecision = useCallback((d: WireDecision) => {
    setData((prev) => {
      if (!prev) return prev;
      const markers = { ...prev.markers };
      // Only emit trade markers for decisions with fill data.
      // Holds without fills are skipped (the REST reload picks them up).
      if (d.fill_price != null && d.fill_size != null) {
        const action = d.action;
        const side =
          action === "long_open" ? "Buy" as const :
          action === "short_open" || action === "flat" ||
            action === "stop_loss" || action === "take_profit" ||
            action === "max_bars_held" || action === "partial_tp1"
            ? "Sell" as const
            : null;
        if (side) {
          markers.trades = [
            ...markers.trades,
            {
              time: new Date(d.timestamp).getTime() / 1000,
              side,
              price: d.fill_price,
              size: d.fill_size,
              fee: d.fee ?? 0,
              pnl_realized: d.pnl_realized ?? null,
              decision_index: d.decision_index,
              justification: d.justification ?? null,
            },
          ];
        }
      }
      return { ...prev, markers };
    });
  }, []);

  const mergeIndicatorTail = useCallback((tail: IndicatorTail) => {
    setData((prev) =>
      prev
        ? {
            ...prev,
            indicators: appendIndicatorTail(prev.indicators, tail),
          }
        : prev,
    );
  }, []);

  useEffect(() => {
    if (!runId) return;
    if (runIdRef.current !== runId) {
      runIdRef.current = runId;
      dataRef.current = undefined;
      setData(undefined);
      setStatus("snapshot");
    }
    let cancelled = false;
    let reconnectTimer: ReturnType<typeof setTimeout> | undefined;
    let reconnectCount = 0;
    let eventCount = 0;
    const trace = createTrace("stream", { run_id: runId });

    async function snapshot() {
      const started = performance.now();
      trace.info("chart.snapshot.load");
      try {
        const p = await getRunChart(runId);
        if (cancelled) return;
        setData(p);
        setStatus("streaming");
        qc.setQueryData(chartKeys.run(runId), p);
        trace.info("chart.snapshot.ok", {
          duration_ms: durationSince(started),
          bars_count: p.bars.length,
          equity_count: p.equity.length,
          trades_count: p.markers.trades.length,
        });
      } catch {
        if (cancelled) return;
        setStatus("closed");
        trace.error("chart.snapshot.error", {
          duration_ms: durationSince(started),
        });
      }
    }

    function parseEvent(e: Event, expected: WireEvent["event"]): WireEvent | null {
      try {
        const parsed = JSON.parse((e as MessageEvent).data) as WireEvent;
        eventCount += 1;
        trace.debug("stream.event", {
          event_type: parsed.event,
          event_count: eventCount,
        });
        if (parsed.event !== expected) return null;
        return parsed;
      } catch (err) {
        trace.warn("stream.parse_error", {
          expected,
          event_count: eventCount,
          error: errorSummary(err),
        });
        return null;
      }
    }

    function openStream() {
      const es = openRunStream(runId);
      esRef.current = es;

      es.addEventListener("bar", (e) => {
        const parsed = parseEvent(e, "bar");
        if (parsed?.event === "bar") {
          trace.debug("chart.merge.bar", { time: parsed.data.time });
          mergeBar(parsed.data);
        }
      });
      es.addEventListener("equity", (e) => {
        const parsed = parseEvent(e, "equity");
        if (parsed?.event === "equity") {
          trace.debug("chart.merge.equity", { time: parsed.data.time });
          mergeEquity(parsed.data);
        }
      });
      es.addEventListener("marker", (e) => {
        const parsed = parseEvent(e, "marker");
        if (parsed?.event === "marker") {
          trace.debug("chart.merge.marker", { kind: parsed.data.kind });
          mergeMarker(parsed.data);
        }
      });
      es.addEventListener("decision", (e) => {
        const parsed = parseEvent(e, "decision");
        if (parsed?.event === "decision") {
          trace.debug("chart.merge.decision", {
            action: parsed.data.action,
            decision_index: parsed.data.decision_index,
          });
          mergeDecision(parsed.data);
        }
      });
      es.addEventListener("indicator_tail", (e) => {
        const parsed = parseEvent(e, "indicator_tail");
        if (parsed?.event === "indicator_tail") {
          trace.debug("chart.merge.indicator_tail", {
            keys: Object.keys(parsed.data),
          });
          mergeIndicatorTail(parsed.data);
        }
      });
      es.addEventListener("status", (e) => {
        const parsed = parseEvent(e, "status");
        if (parsed?.event === "status") {
          const phase = parsed.data.phase;
          const terminal =
            phase === "completed" || phase === "failed" || phase === "cancelled";
          trace.info(terminal ? "stream.terminal" : "stream.status", {
            phase,
            status_message: parsed.data.message,
            event_count: eventCount,
          });
          if (terminal) {
            es.close();
            esRef.current = null;
            setStatus("closed");
            trace.info("stream.closed", { phase });
          }
        }
      });
      es.onerror = () => {
        if (cancelled) return;
        setStatus("reconnecting");
        es.close();
        esRef.current = null;
        reconnectCount += 1;
        trace.warn("stream.error", { reconnect_count: reconnectCount });
        reconnectTimer = setTimeout(() => {
          if (cancelled) return;
          trace.info("stream.reconnect.snapshot", {
            reconnect_count: reconnectCount,
          });
          snapshot().then(() => {
            if (cancelled) return;
            trace.info("stream.reconnect.schedule", {
              reconnect_count: reconnectCount,
              delay_ms: RECONNECT_DELAY_MS,
            });
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

  return { data: data?.run_id === runId ? data : undefined, status };
}
