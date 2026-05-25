pub mod client;
pub mod errors;
pub mod event_sink;
/// Stage-4 sidecar pool: bounded N-client lease/return pool with crash recovery.
pub mod pool;
pub mod protocol;
pub mod provider_map;
pub(crate) mod supervisor;
pub mod tool_dispatch;
/// JSON-RPC over UDS transport. Public for the crate's integration tests
/// (which compile as a separate crate) and for downstream consumers that
/// want raw transport access without spawning a sidecar.
pub mod transport;

pub use client::AgentClient;
pub use errors::{AgentClientError, Result};
pub use event_sink::{
    dispatch as dispatch_notification, mark_runs_interrupted, parse_trajectory_frame_notification,
    start_event_sink, EventSinkHandle, ParsedTrajectoryFrame, SidecarFingerprint, TrajectoryFramePersister,
    TrajectoryFrameSink,
};
pub use pool::{PoolLease, PoolStats, SidecarPool, SlotStatus};
pub use protocol::{
    BudgetLimits, EndRunParams, EndRunResult, ReplayLoadParams, ReplayLoadResult, RunUsage,
    RuntimeHealthResult, SideEffectLevel, StartRunParams, StartRunResult, StepParams, StepResult,
    ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetResult, SUPPORTED_PROTOCOL_VERSION,
};
pub use tool_dispatch::{ToolDispatch, ToolDispatchError};
pub use transport::UdsTransport;
