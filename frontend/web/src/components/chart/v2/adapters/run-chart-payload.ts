import type { RunChartPayload } from "@/api/types.gen";
import type {
  DrawdownPoint,
  EquityPoint,
  IndicatorMap,
  PositionSpan,
  RunChartV2Payload,
  V2Marker,
} from "../types";

export function runChartPayloadToV2(payload: RunChartPayload): RunChartV2Payload {
  return {
    kind: "run",
    asset: payload.asset,
    granularity: payload.granularity,
    candles: {
      time: payload.bars.map((b) => b.time),
      open: payload.bars.map((b) => b.open),
      high: payload.bars.map((b) => b.high),
      low: payload.bars.map((b) => b.low),
      close: payload.bars.map((b) => b.close),
      volume: payload.bars.map((b) => b.volume),
    },
    indicators: indicatorMap(payload),
    equity: payload.equity.map(
      (p): EquityPoint => ({ time: p.time, value: p.equity_usd }),
    ),
    drawdown: payload.drawdown.map(
      (p): DrawdownPoint => ({ time: p.time, value: p.drawdown_pct }),
    ),
    markers: markers(payload),
    positions: positionSpans(payload),
  };
}

function indicatorMap(payload: RunChartPayload): IndicatorMap {
  const i = payload.indicators;
  return {
    sma20: line(i.sma_20),
    sma30: line(i.sma_30),
    sma50: line(i.sma_50),
    sma60: line(i.sma_60),
    sma90: line(i.sma_90),
    sma200: line(i.sma_200),
    ema20: line(i.ema_20),
    ema30: line(i.ema_30),
    ema50: line(i.ema_50),
    ema60: line(i.ema_60),
    ema90: line(i.ema_90),
    ema200: line(i.ema_200),
    bollUpper: line(i.bollinger.upper),
    bollMiddle: line(i.bollinger.middle),
    bollLower: line(i.bollinger.lower),
    donchianUpper: line(i.donchian.upper),
    donchianLower: line(i.donchian.lower),
    rsi: line(i.rsi_14),
    macdLine: line(i.macd.line),
    macdSignal: line(i.macd.signal),
    macdHist: line(i.macd.histogram),
    atr: line(i.atr_14),
  };
}

function line(points: Array<{ time: number; value: number }> | undefined) {
  const rows = points ?? [];
  return {
    time: rows.map((p) => p.time),
    value: rows.map((p) => p.value),
  };
}

function markers(payload: RunChartPayload): V2Marker[] {
  const trades: V2Marker[] = payload.markers.trades.map((m) => ({
    kind: m.side === "Buy" ? "buy" : "sell",
    time: m.time,
    price: m.price,
    decision_index: m.decision_index,
    text: m.justification ?? undefined,
  }));
  const vetoes: V2Marker[] = payload.markers.vetoes.map((m) => ({
    kind: "veto",
    time: m.time,
    price: m.price,
    decision_index: m.decision_index,
    text: m.reason,
  }));
  const holds: V2Marker[] = payload.markers.holds.map((m) => ({
    kind: "hold",
    time: m.time,
    price: m.price,
    decision_index: m.decision_index,
    text: m.conviction == null ? undefined : `conviction ${m.conviction.toFixed(2)}`,
  }));
  return [...trades, ...vetoes, ...holds].sort((a, b) => a.time - b.time);
}

function positionSpans(payload: RunChartPayload): PositionSpan[] {
  const spans: PositionSpan[] = [];
  let open: PositionSpan | null = null;
  for (let i = 0; i < payload.position.length; i++) {
    const p = payload.position[i];
    const side =
      p.side === "Long" ? "long" : p.side === "Short" ? "short" : null;
    if (side == null || p.size === 0) {
      if (open) {
        open.end = p.time;
        spans.push(open);
        open = null;
      }
      continue;
    }
    if (!open) {
      open = { side, start: p.time, end: p.time };
      continue;
    }
    if (open.side !== side) {
      open.end = p.time;
      spans.push(open);
      open = { side, start: p.time, end: p.time };
    } else {
      open.end = p.time;
    }
  }
  if (open) spans.push(open);
  return spans;
}
