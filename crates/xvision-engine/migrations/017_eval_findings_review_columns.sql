-- Extend `eval_findings` with the review-linked v2 shape from
-- `docs/superpowers/specs/2026-05-15-eval-review-agent.md`. Existing
-- extractor callers keep writing `kind` / `severity` / `summary` /
-- `evidence_json` / `extracted_at` / `schema_version`; review findings
-- add the columns below.
--
-- All new columns are nullable so:
--   1. Legacy extractor rows (no review parent) round-trip unchanged.
--   2. New review rows can leave the legacy columns empty when callers
--      have moved fully to v2.
--
-- Apply via the loader's column-presence guard — SQLite does not
-- support `ALTER TABLE ADD COLUMN IF NOT EXISTS`.

ALTER TABLE eval_findings ADD COLUMN eval_review_id TEXT;
ALTER TABLE eval_findings ADD COLUMN type TEXT;
ALTER TABLE eval_findings ADD COLUMN confidence REAL;
ALTER TABLE eval_findings ADD COLUMN title TEXT;
ALTER TABLE eval_findings ADD COLUMN description TEXT;
ALTER TABLE eval_findings ADD COLUMN recommendation TEXT;
ALTER TABLE eval_findings ADD COLUMN created_at TEXT;

CREATE INDEX IF NOT EXISTS idx_findings_review
    ON eval_findings(eval_review_id);
CREATE INDEX IF NOT EXISTS idx_findings_type
    ON eval_findings(type);
