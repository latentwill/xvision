-- bead-8wn: persisted operator-set daily spend budget cap.
--
-- A single-row table holding the operator's chosen daily USD budget cap. The
-- cap is intentionally nullable: an UNSET cap means "no denominator" — the
-- dashboard renders an em-dash, never a faked ceiling (HONESTY §8.1/§8.9). A
-- non-positive / NaN cap is rejected at the API boundary (400), so any value
-- that lands here is finite and > 0.
--
-- DB-wipe posture (no users yet): additive, direct, no backfill. The CHECK
-- pins the table to a single logical row (id = 1) so PUT is a deterministic
-- INSERT OR REPLACE on a known key.
CREATE TABLE IF NOT EXISTS cost_budget (
    id            INTEGER PRIMARY KEY CHECK (id = 1),
    daily_cap_usd REAL,
    updated_at    TEXT NOT NULL
);
