use thiserror::Error;

#[derive(Debug, Error)]
pub enum AgentClientError {
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("serde: {0}")]
    Serde(#[from] serde_json::Error),
    #[error("rpc error {code}: {message}")]
    Rpc { code: i64, message: String },
    #[error("malformed response: missing both result and error")]
    MalformedResponse,
    #[error("incompatible version: {0}")]
    IncompatibleVersion(String),
    #[error("sidecar transport closed")]
    TransportClosed,
}

pub type Result<T> = std::result::Result<T, AgentClientError>;
