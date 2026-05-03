//! Reasoning-token stripper. Reasoning models (o-series, DeepSeek-R1,
//! Qwen-thinking, gpt-oss) emit thinking content before the JSON answer.
//! Two shapes seen in the wild:
//!
//! 1. Provider-native fields — already split out by the SDK
//!    (`response.choices[].message.reasoning_content` for OpenAI-compat
//!    reasoning, `thinking` blocks for Anthropic). Handled at the backend
//!    level by reading the right field.
//! 2. Inline `<think>...</think>` blocks in the user-visible content.
//!    `strip_reasoning` removes these so the JSON body parses cleanly.

use regex::Regex;
use std::sync::OnceLock;

static THINK_RE: OnceLock<Regex> = OnceLock::new();

fn think_re() -> &'static Regex {
    THINK_RE.get_or_init(|| Regex::new(r"(?is)<think>.*?</think>").expect("static regex"))
}

/// Remove `<think>...</think>` blocks (case-insensitive, multi-line, lazy).
/// Trailing whitespace from the strip is normalized so downstream JSON parse
/// doesn't trip on a leading newline.
pub fn strip_reasoning(text: &str) -> String {
    let stripped = think_re().replace_all(text, "");
    stripped.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_simple_block() {
        let s = "<think>I should think...</think>\n{\"x\": 1}";
        assert_eq!(strip_reasoning(s), r#"{"x": 1}"#);
    }

    #[test]
    fn strips_multiline_block() {
        let s = "<think>\nstep 1\nstep 2\n</think>\n\n{\"y\": 2}";
        assert_eq!(strip_reasoning(s), r#"{"y": 2}"#);
    }

    #[test]
    fn strips_multiple_blocks() {
        let s = "<think>a</think>middle<think>b</think>\nend";
        // Whitespace between stripped blocks is preserved (the second block
        // strip leaves the surrounding `\n`); the trim() at the end only
        // touches leading/trailing whitespace of the whole string.
        assert_eq!(strip_reasoning(s), "middle\nend");
    }

    #[test]
    fn passes_through_when_no_block() {
        let s = r#"{"clean": true}"#;
        assert_eq!(strip_reasoning(s), s);
    }

    #[test]
    fn case_insensitive() {
        let s = "<Think>x</Think>{\"y\":1}";
        assert_eq!(strip_reasoning(s), r#"{"y":1}"#);
    }
}
