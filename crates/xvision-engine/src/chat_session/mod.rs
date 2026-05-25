//! Chat-rail persistence backbone — Plan #11.
//!
//! Owns migration `003_chat_sessions.sql` per the v1 migration registry.
//! This module exposes the storage primitives the rail's backend needs:
//! `ChatSessionStore` (sqlx-backed CRUD over `chat_sessions` +
//! `chat_messages`) and `ContextScope` (the per-route context discriminator
//! used for chip sets + composer placeholders).
//!
//! The WizardLoop refactor (Phase B), the `/api/chat-rail/*` endpoints
//! (Phase C), and the frontend rail (Phase D) all build on top of this and
//! are deferred to follow-up PRs.

pub mod context;
pub mod event_log;
pub mod rich_blocks;
pub mod store;
pub mod tool_policy;

pub use context::ContextScope;
pub use event_log::SessionEventLog;
pub use rich_blocks::{
    action_confirmation_card, build_inline_chart, inline_compare_chart_from_report,
    inline_equity_chart_from_run_detail, inline_returns_histogram_from_runs,
    inline_strategy_card_from_summary, run_list_card_from_summaries, ChatActionPayload, ChatRunListItem,
    ChatRunListPayload, ChatStrategyPayload, InlineAction, InlineChartKind, InlineChartPayload,
    InlineChartSeries, InlineChartSource, InlineMetric, InlinePoint, InlineTone, RichBlockError,
    RichContentBlock,
};
pub use store::{ChatMessage, ChatSessionRailState, ChatSessionStore, ChatSessionSummary};
pub use tool_policy::{
    classify as classify_tool, decide as decide_tool_policy, effective_policy, ToolClass, ToolPolicy,
    ToolPolicyRow, ToolPolicyStore, GLOBAL_SCOPE,
};
