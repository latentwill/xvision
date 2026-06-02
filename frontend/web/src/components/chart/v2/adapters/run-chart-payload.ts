import type { RunChartPayload } from "@/api/types.gen";
import type {
  DrawdownPoint,
  IndicatorMap,
  PositionSpan,
  RunChartV2Payload,
  V2Marker,
} from "../types";
import { normalizeEquityToReturnPct } from "./columnar-to-uplot";

export function runChartPayloadToV2(payload: RunChartPayload): RunChartV2Payload {
  const candles = completionAwareCandles(payload);
  return {
    kind: "run",
    asset: payload.asset,
    granularity: payload.granularity,
    candles,
    indicators: indicatorMap(payload),
    equity: normalizeEquityToReturnPct(payload.equity),
    drawdown: payload.drawdown.map(
      (p): DrawdownPoint => ({ time: p.time, value: p.drawdown_pct }),
    ),
    markers: markers(payload),
    positions: positionSpans(payload),
  };
}

function completionAwareCandles(payload: RunChartPayload) {
  const time = payload.bars.map((b) => b.time);
  const open = payload.bars.map((b) => b.open);
  const high = payload.bars.map((b) => b.high);
  const low = payload.bars.map((b) => b.low);
  const close = payload.bars.map((b) => b.close);
  const volume = payload.bars.map((b) => b.volume);
  const lastTime = time.at(-1);
  const finalTime = finalRunTime(payload);
  const lastClose = close.at(-1);

  if (
    lastTime == null ||
    finalTime == null ||
    lastClose == null ||
    finalTime <= lastTime
  ) {
    return { time, open, high, low, close, volume };
  }

  const finalPrices = payload.markers.trades
    .filter((m) => m.time === finalTime && Number.isFinite(m.price))
    .map((m) => m.price);
  const finalHigh = Math.max(lastClose, ...finalPrices);
  const finalLow = Math.min(lastClose, ...finalPrices);

  time.push(finalTime);
  open.push(lastClose);
  high.push(finalHigh);
  low.push(finalLow);
  close.push(lastClose);
  volume.push(0);
  return { time, open, high, low, close, volume };
}

function finalRunTime(payload: RunChartPayload): number | null {
  const times = [
    ...payload.equity.map((p) => p.time),
    ...payload.markers.trades.map((m) => m.time),
  ].filter((time) => Number.isFinite(time));
  return times.length === 0 ? null : Math.max(...times);
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
  }));
  const vetoes: V2Marker[] = payload.markers.vetoes.map((m) => ({
    kind: "veto",
    time: m.time,
    price: m.price,
    decision_index: m.decision_index,
  }));
  const holds: V2Marker[] = payload.markers.holds.map((m) => ({
    kind: "hold",
    time: m.time,
    price: m.price,
    decision_index: m.decision_index,
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
