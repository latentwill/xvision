-- Trace Coverage Cartographer — read-only diagnostic queries for the engine DB.
-- Run against the xvision-engine SQLite database (default: $XVN_HOME/engine.db).
--
-- If risk_outcomes is needed, attach the core DB first:
--   ATTACH DATABASE '/path/to/core.db' AS core;
--
-- All queries are SELECT-only. Safe to run on production.

-- ============================================================
-- Pass 1: eval_runs coverage summary
-- ============================================================
SELECT
    er.status,
    COUNT(*)                                            AS run_count,
    COUNT(er.metrics_json)                              AS with_metrics,
    COUNT(er.agents_agent_id)                           AS with_agent_link,
    COUNT(ar.id)                                        AS with_agent_run,
    COUNT(dr.run_id)                                    AS with_det_receipt
FROM eval_runs er
LEFT JOIN agent_runs ar ON ar.eval_run_id = er.id
LEFT JOIN determinism_receipts dr ON dr.run_id = er.id
GROUP BY er.status
ORDER BY run_count DESC;

-- ============================================================
-- Pass 2: agent_runs coverage (spans, model calls, tool calls, checkpoints)
-- ============================================================
SELECT
    ar.status,
    ar.retention_mode,
    COUNT(ar.id)                                        AS run_count,
    SUM(span_counts.n_spans)                            AS total_spans,
    SUM(span_counts.n_model_calls)                      AS total_model_calls,
    SUM(span_counts.n_tool_calls)                       AS total_tool_calls
FROM agent_runs ar
LEFT JOIN (
    SELECT
        s.run_id,
        COUNT(s.id)     AS n_spans,
        COUNT(mc.span_id) AS n_model_calls,
        COUNT(tc.span_id) AS n_tool_calls
    FROM spans s
    LEFT JOIN model_calls mc ON mc.span_id = s.id
    LEFT JOIN tool_calls tc ON tc.span_id = s.id
    GROUP BY s.run_id
) span_counts ON span_counts.run_id = ar.id
GROUP BY ar.status, ar.retention_mode
ORDER BY run_count DESC;

-- ============================================================
-- Pass 3: checkpoint coverage (replay completeness)
-- ============================================================
SELECT
    ar.retention_mode,
    COUNT(c.id)                                         AS checkpoint_count,
    COUNT(c.input_hash)                                 AS with_input_hash,
    COUNT(c.output_hash)                                AS with_output_hash,
    ROUND(100.0 * COUNT(c.output_hash) / MAX(COUNT(c.id), 1), 1) AS output_hash_pct
FROM agent_runs ar
JOIN checkpoints c ON c.run_id = ar.id
GROUP BY ar.retention_mode
ORDER BY checkpoint_count DESC;

-- ============================================================
-- Pass 4: determinism_receipts completeness
-- (runs completed but missing a receipt)
-- ============================================================
SELECT
    COUNT(*) AS completed_runs_without_receipt
FROM eval_runs er
LEFT JOIN determinism_receipts dr ON dr.run_id = er.id
WHERE er.status = 'completed'
  AND dr.run_id IS NULL;

-- ============================================================
-- Pass 5: orphan checkpoints (run_id not in agent_runs)
-- ============================================================
SELECT COUNT(*) AS orphan_checkpoints
FROM checkpoints c
WHERE NOT EXISTS (
    SELECT 1 FROM agent_runs ar WHERE ar.id = c.run_id
);
