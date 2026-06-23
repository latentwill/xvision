-- Live-run bar storage so the chart endpoint can serve OHLCV candles
-- independently of decision state (warmup, filter-gated, pre-first-decision).
CREATE TABLE IF NOT EXISTS eval_run_bars (
    run_id       TEXT    NOT NULL REFERENCES eval_runs(id) ON DELETE CASCADE,
    bar_index    INTEGER NOT NULL,
    timestamp    TEXT    NOT NULL,   -- ISO-8601
    open         REAL    NOT NULL,
    high         REAL    NOT NULL,
    low          REAL    NOT NULL,
    close        REAL    NOT NULL,
    volume       REAL    NOT NULL,   -- may be zero for synthetic/derived bars
    PRIMARY KEY (run_id, bar_index)
);
CREATE INDEX IF NOT EXISTS idx_eval_run_bars_ts ON eval_run_bars(run_id, timestamp);