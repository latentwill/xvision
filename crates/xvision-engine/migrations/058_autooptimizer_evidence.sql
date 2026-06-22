CREATE TABLE IF NOT EXISTS autooptimizer_findings (
  id           INTEGER PRIMARY KEY AUTOINCREMENT,
  bundle_hash  TEXT NOT NULL,
  severity     TEXT NOT NULL,
  code         TEXT NOT NULL,
  summary      TEXT NOT NULL,
  detail       TEXT,
  model        TEXT,
  created_at   TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_aof_hash ON autooptimizer_findings(bundle_hash);

CREATE TABLE IF NOT EXISTS autooptimizer_gate_records (
  bundle_hash           TEXT PRIMARY KEY,
  parent_day_score      REAL, child_day_score      REAL,
  parent_holdout_score  REAL, child_holdout_score  REAL,
  gate_epsilon          REAL,
  holdout_epsilon       REAL,
  delta_day             REAL, delta_holdout        REAL,
  drawdown_ratio        REAL,
  verdict               TEXT NOT NULL,
  reason                TEXT,
  rationale             TEXT,
  created_at            TEXT NOT NULL
);
