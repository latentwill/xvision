-- 069_tool_http_cache.sql — cache external-tool (Nansen/Elfa) HTTP responses
-- for deterministic backtest re-runs. Keyed by (recording_id, tool_name,
-- input_hash); the input hash includes the injected as_of_date so historical
-- anchors are frozen at record time.
CREATE TABLE tool_http_cache (
  recording_id  TEXT NOT NULL REFERENCES trajectory_recordings(recording_id) ON DELETE CASCADE,
  tool_name     TEXT NOT NULL,
  input_hash    TEXT NOT NULL,
  as_of_date    TEXT,
  response_json TEXT NOT NULL,
  created_at    INTEGER NOT NULL,
  PRIMARY KEY (recording_id, tool_name, input_hash)
);
