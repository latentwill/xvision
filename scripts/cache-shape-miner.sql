-- Cache Shape Miner — read-only cost and prompt-hash analysis for engine DB.
-- Run against the xvision-engine SQLite database (default: $XVN_HOME/engine.db).

-- ============================================================
-- Pass 1: cost and token distribution by provider + model
-- ============================================================
SELECT
    mc.provider,
    mc.model,
    COUNT(*)                                            AS call_count,
    ROUND(SUM(COALESCE(mc.cost_usd, 0)), 6)            AS total_cost_usd,
    ROUND(AVG(COALESCE(mc.cost_usd, 0)), 8)            AS avg_cost_usd,
    SUM(COALESCE(mc.input_token_count, 0))              AS total_input_tokens,
    SUM(COALESCE(mc.output_token_count, 0))             AS total_output_tokens,
    COUNT(DISTINCT mc.prompt_hash)                      AS distinct_prompts,
    COUNT(*) - COUNT(DISTINCT mc.prompt_hash)           AS duplicate_prompt_calls,
    COUNT(mc.cost_usd)                                  AS calls_with_cost,
    COUNT(*) - COUNT(mc.cost_usd)                       AS calls_without_cost
FROM model_calls mc
JOIN spans s ON s.id = mc.span_id
GROUP BY mc.provider, mc.model
ORDER BY total_cost_usd DESC;

-- ============================================================
-- Pass 2: prompt hash collision candidates
-- (same prompt seen from >1 distinct run — caching opportunity)
-- ============================================================
SELECT
    mc.prompt_hash,
    mc.provider,
    mc.model,
    COUNT(DISTINCT s.run_id) AS distinct_runs,
    COUNT(*)                 AS total_calls,
    ROUND(SUM(COALESCE(mc.cost_usd, 0)), 6) AS total_cost_on_hash
FROM model_calls mc
JOIN spans s ON s.id = mc.span_id
GROUP BY mc.prompt_hash, mc.provider, mc.model
HAVING COUNT(DISTINCT s.run_id) > 1
ORDER BY total_cost_on_hash DESC
LIMIT 50;

-- ============================================================
-- Pass 3: null response_hash rate (incomplete or redacted calls)
-- ============================================================
SELECT
    mc.provider,
    mc.model,
    COUNT(*) AS total,
    COUNT(mc.response_hash) AS with_response_hash,
    COUNT(*) - COUNT(mc.response_hash) AS missing_response_hash
FROM model_calls mc
GROUP BY mc.provider, mc.model
ORDER BY missing_response_hash DESC;

-- ============================================================
-- Pass 4: capability path distribution (structured output usage)
-- ============================================================
SELECT
    COALESCE(mc.capability_path, 'unset') AS capability_path,
    COUNT(*)                               AS call_count
FROM model_calls mc
GROUP BY mc.capability_path
ORDER BY call_count DESC;
