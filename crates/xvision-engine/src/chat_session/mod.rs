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
pub mod rich_blocks;
pub mod store;

pub use context::ContextScope;
pub use rich_blocks::{
    build_inline_chart, ChatActionPayload, ChatRunListItem, ChatRunListPayload,
    ChatStrategyPayload, InlineAction, InlineChartKind, InlineChartPayload, InlineChartSeries,
    InlineChartSource, InlineMetric, InlinePoint, InlineTone, RichBlockError, RichContentBlock,
};
pub use store::{ChatMessage, ChatSessionStore, ChatSessionSummary};
