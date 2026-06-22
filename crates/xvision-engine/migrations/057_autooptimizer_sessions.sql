CREATE TABLE IF NOT EXISTS autooptimizer_session_state (
  session_id        TEXT PRIMARY KEY,
  strategy_id       TEXT NOT NULL,
  config_json       TEXT NOT NULL,
  state             TEXT NOT NULL CHECK(state IN ('queued','running','paused','cancelling','cancelled','finished','failed')),
  mode              TEXT NOT NULL CHECK(mode IN ('once','n_experiments','until_budget')),
  cycles_planned    INTEGER,
  cycles_completed  INTEGER NOT NULL DEFAULT 0,
  kept_count        INTEGER NOT NULL DEFAULT 0,
  suspect_count     INTEGER NOT NULL DEFAULT 0,
  dropped_count     INTEGER NOT NULL DEFAULT 0,
  error             TEXT,
  created_at        TEXT NOT NULL,
  started_at        TEXT,
  finished_at       TEXT
);
CREATE INDEX IF NOT EXISTS idx_aoss_state ON autooptimizer_session_state(state);
CREATE INDEX IF NOT EXISTS idx_aoss_created ON autooptimizer_session_state(created_at);

CREATE TABLE IF NOT EXISTS autooptimizer_events (
  seq         INTEGER PRIMARY KEY AUTOINCREMENT,
  session_id  TEXT NOT NULL,
  cycle_id    TEXT,
  kind        TEXT NOT NULL,
  payload_json TEXT NOT NULL,
  ts          TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_aoe_session ON autooptimizer_events(session_id);
CREATE INDEX IF NOT EXISTS idx_aoe_cycle_kind_seq ON autooptimizer_events(cycle_id, kind, seq);
