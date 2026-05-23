-- 037_review_annotations_and_eval_autofire.sql
--
-- R1 of the live-annotation producer spec:
--   docs/superpowers/specs/2026-05-23-live-annotation-producer-and-review-autofire.md
--
-- Adds two schema atoms:
--
--   1. eval_reviews.annotations — JSON array of ReviewAnnotation objects
--      persisted alongside the review in the same row. Defaults to '[]' so
--      legacy rows (pre-R3 prompt extension) read back as an empty slice
--      without any special-casing in the application layer.
--
--   2. eval_runs.auto_fire_review     — boolean (0/1) flag set at eval-creation
--      time. When 1 the finalize path (R2) enqueues a review automatically.
--      Defaults to 0 (manual-fire only) so existing rows are unaffected.
--
--      eval_runs.review_model_provider — nullable TEXT. The provider the
--      operator chose for the auto-fired (or manually-fired) review. NULL
--      means "prompt at fire time". Stored as a separate column pair instead
--      of a JSON blob so the CLI can filter/display without JSON parsing.
--
--      eval_runs.review_model_name     — nullable TEXT. The model id within
--      review_model_provider.
--
--      eval_runs.max_annotations_per_review — nullable INTEGER. Cap on the
--      number of ReviewAnnotation items the review LLM may return. NULL
--      means "use DEFAULT_MAX_ANNOTATIONS_PER_REVIEW (8)".

ALTER TABLE eval_reviews ADD COLUMN annotations TEXT NOT NULL DEFAULT '[]';

ALTER TABLE eval_runs ADD COLUMN auto_fire_review INTEGER NOT NULL DEFAULT 0;
ALTER TABLE eval_runs ADD COLUMN review_model_provider TEXT;
ALTER TABLE eval_runs ADD COLUMN review_model_name TEXT;
ALTER TABLE eval_runs ADD COLUMN max_annotations_per_review INTEGER;
