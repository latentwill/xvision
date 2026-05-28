use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum GateVerdict {
    Passed,
    Rejected,
}

impl GateVerdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "passed",
            Self::Rejected => "rejected",
        }
    }

    pub fn from_str(s: &str) -> Result<Self> {
        match s {
            "passed" => Ok(Self::Passed),
            "rejected" => Ok(Self::Rejected),
            _ => bail!("unknown GateVerdict: {s}"),
        }
    }
}
