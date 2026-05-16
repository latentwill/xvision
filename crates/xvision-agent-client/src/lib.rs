pub mod client;
pub mod errors;
pub mod protocol;
pub(crate) mod supervisor;
pub mod tool_dispatch;
/// JSON-RPC over UDS transport. Public for the crate's integration tests
/// (which compile as a separate crate) and for downstream consumers that
/// want raw transport access without spawning a sidecar.
pub mod transport;

pub use client::AgentClient;
pub use errors::{AgentClientError, Result};
pub use protocol::{
    RuntimeHealthResult, SideEffectLevel, ToolDescriptor, ToolRegistryGetResult,
    ToolRegistrySetResult, SUPPORTED_PROTOCOL_VERSION,
};
pub use tool_dispatch::{ToolDispatch, ToolDispatchError};
pub use transport::UdsTransport;
