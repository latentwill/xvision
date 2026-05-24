-- Live Alpaca launch envelope.
--
-- Adds eval_runs.live_config_json and rebuilds eval_runs so scenario_id can be
-- NULL for scenario-less Live runs. The runtime migrator owns the idempotent
-- rebuild because older operator databases may have a different subset of
-- additive columns depending on when they first opened the app.

ALTER TABLE eval_runs ADD COLUMN live_config_json TEXT;
