-- Add allowed_tools_json to agent_slots: the slot-level tool assignment
-- that replaces capability-typed dispatch. Each entry is a tool name
-- string from the tool registry (e.g. "ohlcv", "submit_decision").
-- DEFAULT '[]' preserves existing rows — the dispatcher falls back to
-- strategy.required_tools when this is empty (backward compat).
-- See docs/plans/2026-06-06-002-feat-tool-based-agent-capabilities-plan.md U3.
ALTER TABLE agent_slots ADD COLUMN allowed_tools_json TEXT NOT NULL DEFAULT '[]';
