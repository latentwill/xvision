-- Phase 2.3 (chat-rail SAFETY CORE): three-state tool policy persistence.
--
-- A `tool_policies` row records the operator's persisted decision for one
-- chat authoring tool, scoped either globally ('global') or per user id.
--   enabled      — 0 ⇒ the tool is hidden from the model entirely (Denied).
--   auto_approve — 1 ⇒ writes run without an approval round-trip; 0 ⇒
--                  NeedsApproval. Read tools auto-approve regardless.
--
-- Defaults (Read → enabled+auto_approve, Write → enabled+needs-approval,
-- Dangerous → disabled) live in the classifier, not the schema: a row exists
-- only when an operator has overridden the default for that tool.
--
-- Wired at runtime via `migrate_tool_policies` in `ApiContext::open` (the
-- hand-maintained registry; this repo does NOT apply migrations through
-- `sqlx::migrate!`). Without that wiring the table never exists at runtime.
CREATE TABLE IF NOT EXISTS tool_policies (
    user_scope   TEXT NOT NULL,      -- 'global' or a user id
    tool_name    TEXT NOT NULL,
    enabled      INTEGER NOT NULL DEFAULT 1,
    auto_approve INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (user_scope, tool_name)
);
