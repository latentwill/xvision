-- Review annotations + per-run review auto-fire controls.
--
-- Spec: docs/superpowers/specs/2026-05-23-live-annotation-producer-and-review-autofire.md
--
-- SQLite stores JSON as TEXT. The application validates both
-- `eval_reviews.annotations_json` and `eval_runs.review_model_json`.

ALTER TABLE eval_reviews
    ADD COLUMN annotations_json TEXT NOT NULL DEFAULT '[]';

ALTER TABLE eval_runs
    ADD COLUMN auto_fire_review INTEGER NOT NULL DEFAULT 0;

ALTER TABLE eval_runs
    ADD COLUMN review_model_json TEXT;

ALTER TABLE eval_runs
    ADD COLUMN max_annotations_per_review INTEGER;
