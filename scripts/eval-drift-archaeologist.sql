-- Eval Drift Archaeologist — per-cohort metric variance analysis.
-- Run against the xvision-engine SQLite database (default: $XVN_HOME/engine.db).
--
-- Comparability rule: runs must share (agent_id, scenario_id, bars_content_hash,
-- manifest_canonical). Runs missing manifest_canonical are bucketed as INCOMPARABLE.
-- MetricsSummary JSON keys (from MetricsSummary struct):
--   total_return_pct, sharpe, max_drawdown_pct, win_rate, n_trades, n_decisions
--   inference_cost_quote_total (optional), net_return_pct (optional)

-- ============================================================
-- Pass 1: comparable cohort inventory
-- ============================================================
SELECT
    agent_id,
    scenario_id,
    COALESCE(manifest_canonical, 'INCOMPARABLE') AS cohort_key,
    COUNT(*)       AS run_count,
    MIN(started_at) AS earliest_run,
    MAX(started_at) AS latest_run
FROM eval_runs
WHERE status = 'completed'
GROUP BY agent_id, scenario_id, cohort_key
HAVING COUNT(*) > 1
ORDER BY run_count DESC;

-- ============================================================
-- Pass 2: metric summary per comparable cohort
-- (requires >=2 completed runs with same manifest_canonical)
-- ============================================================
WITH cohorts AS (
    SELECT
        agent_id,
        scenario_id,
        manifest_canonical,
        COUNT(*) AS run_count
    FROM eval_runs
    WHERE status = 'completed'
      AND manifest_canonical IS NOT NULL
    GROUP BY agent_id, scenario_id, manifest_canonical
    HAVING COUNT(*) > 1
),
runs_with_metrics AS (
    SELECT
        er.id,
        er.agent_id,
        er.scenario_id,
        er.manifest_canonical,
        er.started_at,
        CAST(json_extract(er.metrics_json, '$.total_return_pct') AS REAL)  AS total_return_pct,
        CAST(json_extract(er.metrics_json, '$.sharpe')           AS REAL)  AS sharpe,
        CAST(json_extract(er.metrics_json, '$.max_drawdown_pct') AS REAL)  AS max_drawdown_pct,
        CAST(json_extract(er.metrics_json, '$.win_rate')         AS REAL)  AS win_rate,
        CAST(json_extract(er.metrics_json, '$.n_trades')         AS REAL)  AS n_trades
    FROM eval_runs er
    JOIN cohorts c USING (agent_id, scenario_id, manifest_canonical)
    WHERE er.status = 'completed'
)
SELECT
    agent_id,
    scenario_id,
    manifest_canonical,
    COUNT(*)                                                    AS run_count,
    ROUND(AVG(total_return_pct), 4)                             AS avg_return_pct,
    ROUND(MAX(total_return_pct) - MIN(total_return_pct), 4)     AS return_range,
    ROUND(AVG(sharpe), 4)                                       AS avg_sharpe,
    ROUND(MAX(sharpe) - MIN(sharpe), 4)                         AS sharpe_range,
    ROUND(AVG(max_drawdown_pct), 4)                             AS avg_drawdown_pct,
    ROUND(MAX(max_drawdown_pct) - MIN(max_drawdown_pct), 4)     AS drawdown_range
FROM runs_with_metrics
GROUP BY agent_id, scenario_id, manifest_canonical
ORDER BY return_range DESC;

-- ============================================================
-- Pass 3: incomparable runs (missing manifest_canonical)
-- ============================================================
SELECT
    COUNT(*) AS incomparable_run_count
FROM eval_runs
WHERE status = 'completed'
  AND manifest_canonical IS NULL;
