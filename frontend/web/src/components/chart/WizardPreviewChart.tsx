import { useEffect, useState } from 'react';
import { useQuery } from '@tanstack/react-query';

import { getScenarioPreview } from '@/api/chart';
import type { ScenarioChartPayload } from '@/api/types.gen/ScenarioChartPayload';

import { ScenarioChart } from './ScenarioChart';

type Props = {
  asset: string;
  from: string;
  to: string;
  granularity: '1h' | '1d';
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

  useEffect(() => {
    const t = setTimeout(() => {
      setDebounced({ asset, from, to, granularity, baseline: !!includeBaseline });
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [asset, from, to, granularity, includeBaseline]);

  const ready = !!debounced.asset && !!debounced.from && !!debounced.to;

  const query = useQuery({
    queryKey: ['scenario-preview', debounced],
    queryFn: () => getScenarioPreview(debounced),
    enabled: ready,
    staleTime: 30_000,
  });

  if (!ready) {
    return (
      <div className="text-text-3 text-[12px]">
        Fill asset + date range to see preview…
      </div>
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
      granularity: debounced.granularity === '1h' ? 'Hour1' : 'Day1',
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
    cache_status: query.data.cache_status,
  };

  return (
    <div className="border border-border rounded">
      <div className="px-3 py-2 text-[12px] text-text-3 border-b border-border">
        Preview — {debounced.asset} · {debounced.from} → {debounced.to} · {debounced.granularity}
        {includeBaseline && query.data.baseline_equity && (
          <span className="ml-2">· Buy &amp; Hold baseline ({query.data.baseline_equity.length} pts)</span>
        )}
      </div>
      <div style={{ maxHeight: 220, overflow: 'hidden' }}>
        <ScenarioChart payload={payload} />
      </div>
    </div>
  );
}
