// Renders every chart-v2 primitive in isolation against fixture data so the
// design team can iterate on each without booting an entire surface.

import {
  CacheStatusBadge,
  ChartFrame,
  ConnectionStatus,
  DataTable,
  EmptyState,
  KlineCandlePane,
  LayerPanel,
  Legend,
  MarkerDock,
  PaneStack,
  UplotCompareOverlayPane,
  UplotDrawdownPane,
  UplotEquityPane,
  UplotHistogramPane,
  UplotLinePane,
  UplotOscillatorPane,
} from "@/components/chart/v2/primitives";
import { useChart2Theme } from "@/components/chart/v2/hooks/useChart2Theme";
import { getChart2Fixture } from "@/components/chart/v2/hooks/useChart2Fixture";
import { useState } from "react";

function Card({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="border border-border rounded-card">
      <div className="px-3 py-2 border-b border-border text-[12px] text-text-2 font-medium">
        {title}
      </div>
      <div className="p-3">{children}</div>
    </section>
  );
}

export function ChartLabPrimitives() {
  const theme = useChart2Theme();
  const run = getChart2Fixture("run");
  const compare = getChart2Fixture("compare");
  const [range, setRange] = useState<"1d" | "1w" | "1m" | "3m" | "All">("All");

  return (
    <div className="grid gap-4 grid-cols-1 xl:grid-cols-2">
      <Card title="KlineCandlePane">
        <KlineCandlePane
          candles={run.candles}
          markers={run.markers}
          height={260}
        />
      </Card>

      <Card title="UplotEquityPane">
        <UplotEquityPane points={run.equity} height={160} />
      </Card>

      <Card title="UplotDrawdownPane">
        <UplotDrawdownPane points={run.drawdown} height={140} />
      </Card>

      <Card title="UplotHistogramPane (volume)">
        <UplotHistogramPane candles={run.candles} height={140} />
      </Card>

      <Card title="UplotOscillatorPane (RSI)">
        {run.indicators.rsi ? (
          <UplotOscillatorPane
            kind="rsi"
            series={{ primary: run.indicators.rsi }}
            guides={[30, 70]}
            height={120}
          />
        ) : null}
      </Card>

      <Card title="UplotLinePane">
        {run.indicators.sma20 && run.indicators.sma50 ? (
          <UplotLinePane
            height={120}
            yLabel="Price"
            series={[
              { label: "SMA 20", data: run.indicators.sma20, color: theme.overlay.sma20 },
              { label: "SMA 50", data: run.indicators.sma50, color: theme.overlay.sma50, dashed: true },
            ]}
          />
        ) : null}
      </Card>

      <Card title="UplotCompareOverlayPane">
        <UplotCompareOverlayPane arms={compare.arms} height={180} />
      </Card>

      <Card title="ChartFrame + PaneStack">
        <ChartFrame title="Demo frame" range={range} onRange={setRange}>
          <PaneStack syncKey="lab-demo">
            <UplotEquityPane points={run.equity} height={120} />
            <UplotDrawdownPane points={run.drawdown} height={80} />
          </PaneStack>
        </ChartFrame>
      </Card>

      <Card title="LayerPanel">
        <LayerPanel
          groups={[
            {
              title: "Overlays",
              items: [
                { key: "sma20", label: "SMA 20", on: true },
                { key: "sma50", label: "SMA 50", on: true },
                { key: "sma200", label: "SMA 200", on: false },
                { key: "bollinger", label: "Bollinger", on: false },
              ],
            },
            {
              title: "Markers",
              items: [
                { key: "markerBuy", label: "Buy", on: true },
                { key: "markerSell", label: "Sell", on: true },
                { key: "markerVeto", label: "Veto", on: true },
                { key: "markerHold", label: "Hold", on: false },
              ],
            },
          ]}
          onToggle={() => {}}
        />
      </Card>

      <Card title="MarkerDock">
        <MarkerDock markers={run.markers} />
      </Card>

      <Card title="Legend">
        <Legend
          items={[
            { label: "SMA 20", color: theme.overlay.sma20 },
            { label: "SMA 50", color: theme.overlay.sma50 },
            { label: "SMA 200", color: theme.overlay.sma200 },
            { label: "EMA 20", color: theme.overlay.ema20, dashed: true },
          ]}
        />
      </Card>

      <Card title="ConnectionStatus">
        <div className="flex gap-2">
          <ConnectionStatus state="connected" lastTickMs={Date.now()} />
          <ConnectionStatus state="reconnecting" />
          <ConnectionStatus state="offline" />
        </div>
      </Card>

      <Card title="CacheStatusBadge">
        <div className="flex gap-2">
          <CacheStatusBadge state="fresh" />
          <CacheStatusBadge state="cached" fetchedAt={Date.now()} />
          <CacheStatusBadge state="stale" />
        </div>
      </Card>

      <Card title="EmptyState">
        <EmptyState
          title="No bars yet"
          message="Waiting for the first ETH 1m bar to arrive."
        />
      </Card>

      <Card title="DataTable">
        <DataTable
          columns={[
            { key: "i", header: "#" },
            { key: "time", header: "Time" },
            { key: "close", header: "Close", align: "right" },
          ]}
          rows={run.candles.time.slice(-20).map((t, idx) => ({
            i: idx,
            time: new Date(t * 1000).toISOString().slice(0, 19).replace("T", " "),
            close: run.candles.close[run.candles.close.length - 20 + idx],
          }))}
        />
      </Card>
    </div>
  );
}
