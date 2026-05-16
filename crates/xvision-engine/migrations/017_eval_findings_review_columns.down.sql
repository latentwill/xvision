DROP INDEX IF EXISTS idx_findings_type;
DROP INDEX IF EXISTS idx_findings_review;

ALTER TABLE eval_findings DROP COLUMN created_at;
ALTER TABLE eval_findings DROP COLUMN recommendation;
ALTER TABLE eval_findings DROP COLUMN description;
ALTER TABLE eval_findings DROP COLUMN title;
ALTER TABLE eval_findings DROP COLUMN confidence;
ALTER TABLE eval_findings DROP COLUMN type;
ALTER TABLE eval_findings DROP COLUMN eval_review_id;
