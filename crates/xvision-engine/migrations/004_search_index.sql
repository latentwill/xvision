-- Command Palette (⌘K) full-text search index — Plan #12 Task 1.
-- See v1-shipping-plan.md §"Migration reservations" — owner: Plan #12.
-- Sibling migration `003_chat_sessions.sql` is owned by the Chat Rail
-- Persistence plan (Plan #11) and lands separately.
--
-- One row per indexed artifact. Per-artifact indexer hooks (bundle save,
-- run finalize, finding record) are deferred to follow-up PRs; this PR
-- ships the schema + the SearchIndex CRUD that those hooks will call.
--
-- FTS5 schema notes:
-- - `artifact_id`, `kind`, `updated_at`, `href` are UNINDEXED (returned
--   verbatim, not tokenized).
-- - `title`, `summary`, `tags` are tokenized with `porter unicode61` so
--   "btc" matches "BTCs" and "running" matches "run".
-- - No FK constraints — FTS5 virtual tables don't support them. Stale rows
--   are pruned by indexer-side `delete()` calls.

CREATE VIRTUAL TABLE IF NOT EXISTS search_index USING fts5(
    artifact_id UNINDEXED,
    kind UNINDEXED,
    title,
    summary,
    tags,
    updated_at UNINDEXED,
    href UNINDEXED,
    tokenize='porter unicode61'
);
