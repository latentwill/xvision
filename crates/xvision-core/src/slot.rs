//! `SlotRef` — `<provider>/<model>` reference used to resolve a backend at run
//! time. Provider names are restricted to `[a-z0-9-]+` (Plan #7 Phase 1 garde
//! rule) which keeps the first `/` unambiguous: everything before is the
//! provider, everything after is the model id (model ids may themselves
//! contain `/`).

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SlotRef {
    pub provider: String,
    pub model: String,
}

#[derive(Debug, Error, PartialEq)]
pub enum SlotParseError {
    #[error("slot ref must contain '/': got `{0}`")]
    MissingSlash(String),
    #[error("slot ref provider segment must be non-empty: got `{0}`")]
    EmptyProvider(String),
    #[error("slot ref model segment must be non-empty: got `{0}`")]
    EmptyModel(String),
}

impl SlotRef {
    pub fn new(provider: impl Into<String>, model: impl Into<String>) -> Self {
        Self {
            provider: provider.into(),
            model: model.into(),
        }
    }
}

impl FromStr for SlotRef {
    type Err = SlotParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (provider, model) = s
            .split_once('/')
            .ok_or_else(|| SlotParseError::MissingSlash(s.to_string()))?;
        if provider.is_empty() {
            return Err(SlotParseError::EmptyProvider(s.to_string()));
        }
        if model.is_empty() {
            return Err(SlotParseError::EmptyModel(s.to_string()));
        }
        Ok(Self::new(provider, model))
    }
}

impl fmt::Display for SlotRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}/{}", self.provider, self.model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_simple() {
        let s: SlotRef = "anthropic/claude-opus-4-7".parse().unwrap();
        assert_eq!(s.provider, "anthropic");
        assert_eq!(s.model, "claude-opus-4-7");
    }

    #[test]
    fn model_id_keeps_inner_slashes() {
        let s: SlotRef = "together/meta-llama/Llama-3.3-70B-Instruct-Turbo"
            .parse()
            .unwrap();
        assert_eq!(s.provider, "together");
        assert_eq!(s.model, "meta-llama/Llama-3.3-70B-Instruct-Turbo");
    }

    #[test]
    fn display_round_trips() {
        let s = SlotRef::new("openai", "gpt-4o");
        assert_eq!(s.to_string(), "openai/gpt-4o");
        let back: SlotRef = s.to_string().parse().unwrap();
        assert_eq!(back, s);
    }

    #[test]
    fn rejects_missing_slash() {
        assert_eq!(
            "noslash".parse::<SlotRef>(),
            Err(SlotParseError::MissingSlash("noslash".into()))
        );
    }

    #[test]
    fn rejects_empty_provider() {
        assert_eq!(
            "/model".parse::<SlotRef>(),
            Err(SlotParseError::EmptyProvider("/model".into()))
        );
    }

    #[test]
    fn rejects_empty_model() {
        assert_eq!(
            "provider/".parse::<SlotRef>(),
            Err(SlotParseError::EmptyModel("provider/".into()))
        );
    }
}
