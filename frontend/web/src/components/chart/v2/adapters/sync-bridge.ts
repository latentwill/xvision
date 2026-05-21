/**
 * Phase-M0 wiring:
 *   - uplot→uplot crossplot sync via uPlot.sync(key) (native pub/sub).
 *   - kline→uplot direction is stubbed here; real subscribeCrosshair hookup
 *     lands in M1 once KlineCharts exposes a stable crosshair event API.
 */

import uPlot from "uplot";

type KlineCursorHandler = (ts: number | null) => void;

export type SyncBridge = {
  /** The uPlot.sync key shared by all subscribing plots. */
  key: string;
  /** The uPlot SyncPubSub object — uPlot instances self-register via cursor.sync.key. */
  uplotSync: uPlot.SyncPubSub;
  /**
   * No-op pass-through: documents that this uPlot instance participates in the
   * sync group.  uPlot handles actual registration when cursor.sync.key matches.
   */
  register(plot: uPlot): void;
  /**
   * Broadcast a KlineCharts crosshair position to all registered uPlot instances.
   * M0 stub: safely does nothing if no plots are registered or coords are null.
   * Real pixel-mapping wires in M1.
   */
  broadcastFromKline(timestampMs: number | null): void;
  /** Subscribe to KlineCharts crosshair events (kline → uplot direction). */
  subscribeKlineCursor(handler: KlineCursorHandler): () => void;
  /** Notify all KlineCharts cursor subscribers (called from KlineCharts hooks). */
  notifyKlineCursor(ts: number | null): void;
};

/**
 * Create a named sync bridge.  One bridge per chart surface — share the
 * returned object across all panes that should move together.
 */
export function createSyncBridge(key: string): SyncBridge {
  const uplotSync = uPlot.sync(key);
  const klineHandlers = new Set<KlineCursorHandler>();

  const bridge: SyncBridge = {
    key,
    uplotSync,

    register(_plot: uPlot): void {
      // uPlot self-registers when opts.cursor.sync.key === this.key.
      // Nothing to do here; the method exists for documentation and future hooks.
    },

    broadcastFromKline(timestampMs: number | null): void {
      // M0 stub — coordinates require valToPos() which needs a live plot reference.
      // Will be wired to setCursor calls in M1.
      // Guard exists so callers can forward null crosshair-leave events safely.
      if (timestampMs === null || uplotSync.plots.length === 0) return;
    },

    subscribeKlineCursor(handler: KlineCursorHandler): () => void {
      klineHandlers.add(handler);
      return () => {
        klineHandlers.delete(handler);
      };
    },

    notifyKlineCursor(ts: number | null): void {
      for (const handler of klineHandlers) {
        handler(ts);
      }
    },
  };

  return bridge;
}
