//! OSShip-style markdown skills for xvn.
//!
//! A skill is a single markdown file with YAML frontmatter:
//!
//! ```text
//! ---
//! name: crypto-trader-base
//! display_name: "Generalist crypto trader"
//! description: "Default trader prompt for any crypto strategy"
//! version: 1.0.0
//! allowed_tools: [ohlcv, indicator_panel]
//! model_requirement: "anthropic.claude-sonnet-4.6+"
//! ---
//!
//! You are a crypto trader. ...
//! ```
//!
//! Plan 2b ships parser + filesystem store + attach-to-agent helper.
//! Marketplace discovery + content-addressed publishing wait for
//! Plan 5 (blockchain integration).

pub mod attach;
pub mod frontmatter;
pub mod store;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Skill {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub allowed_tools: Vec<String>,
    pub model_requirement: String,
    pub body: String,
    pub content_hash: String,
}

#[derive(Debug, thiserror::Error)]
pub enum SkillError {
    #[error("missing frontmatter delimiters")]
    MissingFrontmatter,
    #[error("YAML parse error: {0}")]
    YamlParse(#[from] serde_yaml::Error),
    #[error("required field missing: {0}")]
    MissingField(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("utf8: {0}")]
    Utf8(#[from] std::str::Utf8Error),
}

pub fn parse(markdown: &str) -> Result<Skill, SkillError> {
    let (frontmatter_yaml, body) = frontmatter::split(markdown)?;
    let parsed: serde_yaml::Value = serde_yaml::from_str(frontmatter_yaml)?;
    let get_str = |key: &str| -> Result<String, SkillError> {
        parsed
            .get(key)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| SkillError::MissingField(key.to_string()))
    };
    let allowed_tools: Vec<String> = parsed
        .get("allowed_tools")
        .and_then(|v| v.as_sequence())
        .map(|seq| {
            seq.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();
    Ok(Skill {
        name: get_str("name")?,
        display_name: get_str("display_name")?,
        description: get_str("description")?,
        version: get_str("version")?,
        allowed_tools,
        model_requirement: get_str("model_requirement")?,
        body: body.to_string(),
        content_hash: sha256_hex(markdown.as_bytes()),
    })
}

fn sha256_hex(input: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(input);
    hasher.finalize().iter().map(|b| format!("{b:02x}")).collect()
}
