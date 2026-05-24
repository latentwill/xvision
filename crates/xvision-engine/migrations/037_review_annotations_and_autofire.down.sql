ALTER TABLE eval_reviews DROP COLUMN annotations_json;
ALTER TABLE eval_runs DROP COLUMN auto_fire_review;
ALTER TABLE eval_runs DROP COLUMN review_model_json;
ALTER TABLE eval_runs DROP COLUMN max_annotations_per_review;
