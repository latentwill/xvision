/**
 * M0 stub — pure in-memory streaming buffer for live candle updates.
 * The WebSocket hookup and real-time feed integration arrive in M3.
 */

import { type KLineData } from "klinecharts";

export type StreamingBuffer = {
  /**
   * Push a new bar into the buffer.
   * If the bar's timestamp matches the last bar, updates it in-place (tick merge).
   * Otherwise appends.  Trims the oldest bar when buffer exceeds maxBars.
   */
  push(bar: KLineData): void;
  /**
   * Return the current buffer contents and mark it as clean.
   * Does NOT clear the buffer — use size() to track changes.
   */
  flush(): KLineData[];
  /** Timestamp (ms) of the most recent bar, or null if the buffer is empty. */
  lastTimestampMs(): number | null;
  /** Current number of bars in the buffer. */
  size(): number;
};

/**
 * Create a streaming bar buffer with a maximum capacity of `maxBars`.
 */
export function createStreamingBuffer(maxBars: number): StreamingBuffer {
  const bars: KLineData[] = [];

  return {
    push(bar: KLineData): void {
      if (bars.length > 0 && bars[bars.length - 1].timestamp === bar.timestamp) {
        // Same timestamp — update last bar in-place (real-time tick merge).
        bars[bars.length - 1] = bar;
      } else {
        bars.push(bar);
        if (bars.length > maxBars) {
          bars.shift();
        }
      }
    },

    flush(): KLineData[] {
      // Returns a shallow copy so callers cannot mutate the internal buffer.
      // Dirty-flag tracking will wire in M3 when the WS hookup lands.
      return bars.slice();
    },

    lastTimestampMs(): number | null {
      return bars.length > 0 ? bars[bars.length - 1].timestamp : null;
    },

    size(): number {
      return bars.length;
    },
  };
}
