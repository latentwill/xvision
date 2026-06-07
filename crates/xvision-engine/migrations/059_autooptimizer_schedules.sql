CREATE TABLE IF NOT EXISTS autooptimizer_schedules (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  enabled      INTEGER NOT NULL DEFAULT 1,
  time_local   TEXT NOT NULL,
  strategy_id  TEXT NOT NULL,
  config_json  TEXT NOT NULL,
  last_run_at  TEXT,
  next_run_at  TEXT
);
