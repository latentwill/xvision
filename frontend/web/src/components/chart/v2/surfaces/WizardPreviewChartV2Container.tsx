import { useEffect, useState } from "react";
import { useQuery } from "@tanstack/react-query";

import { getScenarioPreview } from "@/api/chart";
import { useBarsFetchJob } from "@/components/scenario/useBarsFetchJob";

import { scenarioPreviewToWizardV2 } from "../adapters/scenario-preview-payload";
import { WizardPreviewChartV2 } from "./WizardPreviewChartV2";

type Props = {
  asset: string;
  from: string;
  to: string;
  granularity: string;
  includeBaseline?: boolean;
};

const DEBOUNCE_MS = 350;

/**
 * Fetching wrapper that reproduces the v1 `WizardPreviewChart` behavior
 * (debounce, button gate, preview query, bars-fetch job) but renders the
 * v2 `WizardPreviewChartV2` surface instead of v1 `ScenarioChart`.
 *
 * The v1 component passed the bars-fetch controls (onFetch / fetchStatus /
 * fetchDisabled) INTO `ScenarioChart`'s cache-status badge. The v2 surface
 * has no cache badge, so we surface the "Fetch bars" action + status block
 * at the container level — inline, no popup — mirroring v1's status block.
 */
export function WizardPreviewChartV2Container({
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
  // auto-renders on every form change. Gating the render behind an
  // explicit toggle avoids hammering the preview endpoint while the
  // operator is mid-typing and prevents the "tiny and squished" chart
  // from partial form values.
  //
  // PR #341 code review followup: `shown` is also reset whenever the
  // operator changes any input. Otherwise the gate only matters the
  // first time the chart is shown — subsequent input changes would
  // silently refetch via `enabled: ready && shown`.
  const [shown, setShown] = useState(false);

  useEffect(() => {
    const t = setTimeout(() => {
      setDebounced({ asset, from, to, granularity, baseline: !!includeBaseline });
    }, DEBOUNCE_MS);
    return () => clearTimeout(t);
  }, [asset, from, to, granularity, includeBaseline]);

  // Reset the button gate whenever inputs change. The dependency list
  // mirrors the debounce useEffect above so the chart only stays
  // visible while inputs are stable.
  useEffect(() => {
    setShown(false);
  }, [asset, from, to, granularity, includeBaseline]);

  const ready = !!debounced.asset && !!debounced.from && !!debounced.to;
  const previewQueryKey = ["scenario-preview", debounced] as const;

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
      <div>
        <WizardPreviewChartV2 payload={scenarioPreviewToWizardV2(query.data)} />
      </div>
      {(barsFetch.canStart ||
        barsFetch.statusText ||
        barsFetch.outputText ||
        barsFetch.errorText) && (
        <div className="border-t border-border px-3 py-2 text-[12px] text-text-2">
          {barsFetch.canStart && (
            <button
              type="button"
              data-testid="wizard-preview-fetch-bars"
              onClick={barsFetch.start}
              disabled={!barsFetch.canStart}
              className="inline-flex items-center gap-2 px-3 py-1.5 rounded text-[12px] font-medium border border-border text-text hover:border-text-3 transition-colors disabled:opacity-50"
            >
              Fetch bars
            </button>
          )}
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
