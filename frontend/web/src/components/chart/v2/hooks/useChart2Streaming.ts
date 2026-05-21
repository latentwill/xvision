// M0 stub — real WebSocket subscriber arrives in M3.
// This module provides a stable, drop-in-compatible API surface so
// surfaces and tests can depend on the hook today.

import { useMemo } from "react";

export type KLineDataLike = {
  timestamp: number;
  open: number;
  high: number;
  low: number;
  close: number;
  volume?: number;
};

export type Chart2StreamingResult = {
  bars: KLineDataLike[];
  connection: "connected";
  lastTickMs: number | null;
  /** M0 stub — no-op. Real push arrives in M3. */
  push: (_bar: KLineDataLike) => void;
};

export type UseChart2StreamingOpts = {
  surface: string;
  initial: KLineDataLike[];
  maxBars?: number;
};

/**
 * M0 stub. Returns the initial bars unchanged, reports connection as
 * "connected", and exposes a no-op `push` function.
 *
 * The `bars` array is memoized on `initial` identity so downstream
 * components do not re-render unless the caller passes a new array.
 *
 * Real WebSocket subscriber arrives in M3.
 */
export function useChart2Streaming(
  opts: UseChart2StreamingOpts,
): Chart2StreamingResult {
  const { initial } = opts;

  // Stable reference — only changes when the caller passes a new `initial`.
  const bars = useMemo(() => initial, [initial]);

  return {
    bars,
    connection: "connected" as const,
    lastTickMs: null,
    push: (_bar: KLineDataLike) => {
      // M0 stub — intentional no-op
    },
  };
}
