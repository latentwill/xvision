//! Trajectory recording subsystem (Stage 2 — Cline Runtime Unification).
//!
//! Persists the full agent trajectory (every model frame, tool call, and
//! tool result) for every slot of every recorded run, in a versioned,
//! content-addressed store that a recorded run can be reconstructed from
//! byte-for-byte.
//!
//! # Sub-modules
//!
//! - `key`     — `TrajectoryKey`, `RecordingId`, `TRAJECTORY_SCHEMA_VERSION`
//! - `frame`   — `TrajectoryFrame` enum (Rust mirror of `AgentModelEvent`)
//! - `channel` — lossless backpressured `FrameChannel` (`tokio::mpsc` bounded)
//! - `store`   — `TrajectoryStore` (write/read/validate/purge/reindex)

pub mod channel;
pub mod frame;
pub mod key;
pub mod store;

pub use channel::{
    ChannelStatus, FrameChannel, FrameReceiver, FrameSender, DEFAULT_FRAME_CHANNEL_CAPACITY,
};
pub use frame::TrajectoryFrame;
pub use key::{RecordingId, TrajectoryKey, TrajectoryKeyBuilder, TRAJECTORY_SCHEMA_VERSION};
pub use store::{
    FrameCount, RecordingInfo, StoreError, TrajectoryStore, STATUS_COMPLETE, STATUS_CORRUPT,
    STATUS_INCOMPLETE, STATUS_OPEN,
};
