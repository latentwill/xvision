import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';

import { getScenarioPreview } from '@/api/chart';
import type { ScenarioChartPayload } from '@/api/types.gen/ScenarioChartPayload';
import { useBarsFetchJob } from '@/components/scenario/useBarsFetchJob';

import { ScenarioChart } from './ScenarioChart';

type Props = {
  asset: string;
  from: string;
  to: string;
  granularity: string;
  includeBaseline?: boolean;
};

const DEBOUNCE_MS = 350;

export function WizardPreviewChart({
  asset,
  from,
  to,
  granularity,
  includeBaseline,
}: Props) {
  const [debounced, setDebounced] = useState({
    asset,
    from,
    to,
    granularity,
    baseline: !!includeBaseline,
  });
  // QA22 / `eval-inspector-chart-snap-button`: the preview no longer
  // auto-renders on every form change. Operator reported the prior
  // always-on render produced a "tiny and squished" chart whenever the
  // form had partial values. Gating the render behind an explicit
  // toggle also avoids hammering the preview endpoint while the
  // operator is mid-typing.
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => {
      setDebounced({ asset, from, to, granularity, baseline: !!includeBaseline });
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [asset, from, to, granularity, includeBaseline]);

  const ready = !!debounced.asset && !!debounced.from && !!debounced.to;
  const previewQueryKey = ['scenario-preview', debounced] as const;

  const query = useQuery({
    queryKey: previewQueryKey,
    queryFn: () => getScenarioPreview(debounced),
    enabled: ready && shown,
    staleTime: 30_000,
  });
  const barsFetch = useBarsFetchJob(
    ready && shown
      ? {
          asset: debounced.asset,
          from: debounced.from,
          to: debounced.to,
          granularity: debounced.granularity,
          invalidateQueryKeys: [previewQueryKey],
        }
      : null,
  );

  if (!ready) {
    return (
      <div className="text-text-3 text-[12px]">
        Fill asset + date range to see preview…
      </div>
    );
  }
  if (!shown) {
    return (
      <button
        type="button"
        data-testid="wizard-preview-show"
        onClick={() => setShown(true)}
        className="inline-flex items-center gap-2 px-3.5 py-2 rounded text-[13px] font-medium border border-border text-text hover:border-text-3 transition-colors"
      >
        Show preview chart
      </button>
    );
  }
  if (query.isLoading) {
    return <div className="text-text-3 text-[12px]">Loading preview…</div>;
  }
  if (query.error) {
    return (
      <div className="text-danger text-[12px]">
        Preview failed: {String((query.error as Error).message ?? query.error)}
      </div>
    );
  }
  if (!query.data) return null;

  // Reuse ScenarioChart for visual consistency by synthesising the minimum
  // Scenario shape it needs. The chart only reads scenario.granularity +
  // bar_cache_policy.cache_key + asset[0] + tags.
  const payload: ScenarioChartPayload = {
    scenario: {
      id: 'preview',
      parent_scenario_id: null,
      source: 'User',
      display_name: 'Preview',
      description: '',
      notes: null,
      asset_class: 'Crypto',
      asset: [
        {
          class: 'Crypto',
          symbol: debounced.asset,
          venue_symbol: `${debounced.asset}/USD`,
        },
      ],
      quote_currency: 'Usd',
      time_window: { start: debounced.from, end: debounced.to },
      granularity: previewGranularityToScenario(debounced.granularity),
      timezone: 'UTC',
      calendar: 'Continuous24x7',
      data_source: { type: 'AlpacaHistorical', feed: null, adjustment: 'Raw' },
      venue: {
        venue: 'Alpaca',
        fees: { maker_bps: 10, taker_bps: 25 },
        slippage: { model: 'linear', bps: 5 },
        latency: { decision_to_fill_ms: 500 },
        fill_model: {
          market_order_fill: 'FullAtClose',
          limit_order_fill: 'NeverFills',
          partial_fills: false,
          volume_constraints: null,
        },
      },
      replay_mode: { mode: 'Continuous' },
      bar_cache_policy: {
        cache_key: query.data.cache_key,
        refresh_policy: 'NeverRefresh',
        data_fetched_at: null,
      },
      tags: [],
      created_at: new Date().toISOString(),
      created_by: '',
      archived_at: null,
    } as unknown as ScenarioChartPayload['scenario'],
    bars: query.data.bars,
    indicators: emptyIndicators(),
    cache_status: query.data.cache_status,
  };

  return (
    <div className="border border-border rounded">
      <div className="px-3 py-2 text-[12px] text-text-3 border-b border-border flex items-center justify-between gap-2">
        <span>
          Preview — {debounced.asset} · {debounced.from} → {debounced.to} · {debounced.granularity}
          {includeBaseline && query.data.baseline_equity && (
            <span className="ml-2">· Buy &amp; Hold baseline ({query.data.baseline_equity.length} pts)</span>
          )}
        </span>
        <button
          type="button"
          data-testid="wizard-preview-hide"
          onClick={() => setShown(false)}
          className="text-text-3 hover:text-text transition-colors"
          aria-label="Hide preview chart"
        >
          Hide
        </button>
      </div>
      {/*
        The inner `ScenarioChart` reserves 360px for its price pane plus
        the cache-status header + ChartContainer chrome. The prior
        `maxHeight: 220` cap clipped the bottom of the chart and the
        operator saw a tiny squished render. Let the chart use its
        natural height now that this preview is button-gated.
      */}
      <div>
        <ScenarioChart
          payload={payload}
          onFetch={barsFetch.start}
          fetchStatus={barsFetch.statusText}
          fetchDisabled={!barsFetch.canStart}
        />
      </div>
      {(barsFetch.statusText || barsFetch.outputText || barsFetch.errorText) && (
        <div className="border-t border-border px-3 py-2 text-[12px] text-text-2">
          {barsFetch.statusText && <div>{barsFetch.statusText}</div>}
          {barsFetch.errorText && (
            <div className="text-danger">{barsFetch.errorText}</div>
          )}
          {barsFetch.outputText && (
            <pre className="mt-2 whitespace-pre-wrap font-mono text-[11px] text-text-3">
              {barsFetch.outputText}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

function emptyIndicators(): ScenarioChartPayload['indicators'] {
  const empty: ScenarioChartPayload['indicators']['sma_20'] = [];
  return {
    sma_20: empty, sma_30: empty, sma_50: empty, sma_60: empty, sma_90: empty, sma_200: empty,
    ema_20: empty, ema_30: empty, ema_50: empty, ema_60: empty, ema_90: empty, ema_200: empty,
    bollinger: { upper: empty, middle: empty, lower: empty },
    donchian: { upper: empty, lower: empty },
    rsi_14: empty,
    macd: { line: empty, signal: empty, histogram: empty },
    atr_14: empty,
  };
}

function previewGranularityToScenario(granularity: Props['granularity']) {
  return granularity;
}
