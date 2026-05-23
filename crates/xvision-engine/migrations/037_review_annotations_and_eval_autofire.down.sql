-- 037_review_annotations_and_eval_autofire.down.sql
--
-- Reverses 037_review_annotations_and_eval_autofire.sql.
--
-- DROP COLUMN requires SQLite ≥ 3.35.0 (released 2021-03-12).
-- The workspace pins rusqlite/sqlx features that bundle SQLite 3.45+
-- (verify: `grep -r "bundled\|sqlcipher" Cargo.toml`), so this form
-- is safe. If for any reason the host SQLite is older, replace each
-- DROP COLUMN with a table-recreate workaround.

ALTER TABLE eval_runs DROP COLUMN max_annotations_per_review;
ALTER TABLE eval_runs DROP COLUMN review_model_name;
ALTER TABLE eval_runs DROP COLUMN review_model_provider;
ALTER TABLE eval_runs DROP COLUMN auto_fire_review;
ALTER TABLE eval_reviews DROP COLUMN annotations;
