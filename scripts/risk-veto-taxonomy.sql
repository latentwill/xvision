-- Risk Veto Taxonomist — read-only frequency report on RiskDecision outcomes.
-- Run against the xvision-core SQLite database (default: $XVN_HOME/core.db).
--
-- VetoReason is serde(rename_all = "snake_case"). Unit variants serialize as
-- plain strings ("position_too_large"); Custom(String) serializes as {"custom": "..."}.
-- RiskDecision uses serde(tag = "verdict") so $.verdict = "vetoed"|"modified"|"approved".

-- ============================================================
-- Pass 1: top-level verdict distribution
-- ============================================================
SELECT
    json_extract(risk_decision_json, '$.verdict') AS verdict,
    COUNT(*)                                       AS count
FROM risk_outcomes
GROUP BY verdict
ORDER BY count DESC;

-- ============================================================
-- Pass 2: veto reason frequency (vetoed rows only)
-- ============================================================
WITH vetoed AS (
    SELECT
        json_extract(risk_decision_json, '$.verdict') AS verdict,
        json_extract(risk_decision_json, '$.reason')  AS reason_raw
    FROM risk_outcomes
    WHERE json_extract(risk_decision_json, '$.verdict') IN ('vetoed', 'modified')
)
SELECT
    verdict,
    CASE
        WHEN json_type(reason_raw) = 'text' THEN reason_raw
        WHEN json_type(reason_raw) = 'object'
             THEN 'custom:' || COALESCE(json_extract(reason_raw, '$.custom'), '?')
        ELSE 'unknown'
    END AS reason,
    COUNT(*) AS count
FROM vetoed
GROUP BY verdict, reason
ORDER BY count DESC;

-- ============================================================
-- Pass 3: custom veto text sample (up to 20 rows)
-- ============================================================
SELECT
    cycle_id,
    arm_name,
    json_extract(risk_decision_json, '$.reason.custom') AS custom_text,
    created_at
FROM risk_outcomes
WHERE json_extract(risk_decision_json, '$.verdict') IN ('vetoed', 'modified')
  AND json_type(json_extract(risk_decision_json, '$.reason')) = 'object'
ORDER BY created_at DESC
LIMIT 20;
