-- Down migration for 0001_dashboard_sessions.
DROP TABLE IF EXISTS auth_audit;
DROP TABLE IF EXISTS dashboard_sessions;
