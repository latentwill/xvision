pub mod client;
pub mod errors;
pub mod event_sink;
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
    dispatch as dispatch_notification, mark_runs_interrupted, start_event_sink, EventSinkHandle,
    SidecarFingerprint,
};
pub use protocol::{
    BudgetLimits, EndRunParams, EndRunResult, RunUsage, RuntimeHealthResult, SideEffectLevel, StartRunParams,
    StartRunResult, StepParams, StepResult, ToolDescriptor, ToolRegistryGetResult, ToolRegistrySetResult,
    SUPPORTED_PROTOCOL_VERSION,
};
pub use tool_dispatch::{ToolDispatch, ToolDispatchError};
pub use transport::UdsTransport;
