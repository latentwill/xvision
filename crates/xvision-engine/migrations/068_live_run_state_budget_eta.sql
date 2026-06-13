-- Migration 068: daily-loss budget + stop ETA columns on live_run_state.
--
-- daily_loss_budget_usd: kill_pct * initial capital. Unlocks the strip's
--   buffer %-gradient (remaining / budget). Nullable REAL so pre-068 rows
--   (and live runs with no risk config) stay NULL.
--
-- stop_at: wall-clock deadline (RFC-3339) = started_at + time_limit_secs,
--   only when stop_policy.time_limit_secs is Some. NULL for bar/decision
--   stop policies (they have no wall-clock ETA). Unlocks awm's ETA display.
--
-- Applied via migrate_live_run_state_budget_eta, which guards each column
-- independently via table_has_column so a crash between the two non-atomic
-- ALTERs never strands the DB with one column missing.
ALTER TABLE live_run_state ADD COLUMN daily_loss_budget_usd REAL;
ALTER TABLE live_run_state ADD COLUMN stop_at TEXT;
