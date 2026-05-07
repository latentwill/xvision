//! Parse + validate the Trader's JSON response. Pure function; the backend
//! wire is in `run.rs`.
//!
//! The parser is forgiving in the same ways `xianvec_intern::parse_llm_response`
//! is forgiving:
//! 1. `<think>...</think>` blocks are stripped (Qwen-thinking, R1, etc.).
//! 2. The body is trimmed to the substring between the first `{` and last `}`,
//!    handling fenced markdown / leading prose.
//! 3. The decoded shape is validated via garde at the boundary.

use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use xianvec_core::trading::{Action, Direction, TraderDecision};
use xianvec_intern::strip_reasoning;

use crate::error::TraderError;

/// What the LLM produces. The runtime fills in `setup_id`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTraderDecision {
    pub action: Action,
    pub direction: Direction,
    pub size_bps: u32,
    pub stop_loss_pct: f32,
    pub take_profit_pct: f32,
    pub trader_summary: String,
}

/// Parse + validate a Trader response. The caller supplies the runtime-owned
/// `setup_id`.
pub fn parse_trader_response(body: &str, setup_id: Uuid) -> Result<TraderDecision, TraderError> {
    if body.trim().is_empty() {
        return Err(TraderError::Empty);
    }

    let stripped = strip_reasoning(body);
    let trimmed = trim_to_json(&stripped);

    let llm: LlmTraderDecision = serde_json::from_str(&trimmed)
        .map_err(|e| TraderError::Parse(format!("{e}; body[..200]={}", short(&trimmed, 200))))?;

    let decision = TraderDecision {
        setup_id,
        action: llm.action,
        size_bps: llm.size_bps,
        direction: llm.direction,
        stop_loss_pct: llm.stop_loss_pct,
        take_profit_pct: llm.take_profit_pct,
        trader_summary: llm.trader_summary,
    };
    decision.validate().map_err(TraderError::Validation)?;
    Ok(decision)
}

fn trim_to_json(s: &str) -> String {
    if let (Some(start), Some(end)) = (s.find('{'), s.rfind('}')) {
        if start < end {
            return s[start..=end].to_string();
        }
    }
    s.to_string()
}

fn short(s: &str, n: usize) -> &str {
    if s.len() <= n {
        s
    } else {
        &s[..n]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(body: &str) -> Result<TraderDecision, TraderError> {
        parse_trader_response(body, Uuid::nil())
    }

    const GOLDEN_BUY: &str = r#"{
        "action": "buy",
        "direction": "long",
        "size_bps": 800,
        "stop_loss_pct": 2.0,
        "take_profit_pct": 5.0,
        "trader_summary": "Long entry on confirmed trend with 2.5:1 R:R."
    }"#;

    #[test]
    fn parses_clean_buy() {
        let d = parse(GOLDEN_BUY).expect("clean buy must parse");
        assert_eq!(d.action, Action::Buy);
        assert_eq!(d.direction, Direction::Long);
        assert_eq!(d.size_bps, 800);
    }

    #[test]
    fn parses_flat() {
        let body = r#"{"action":"flat","direction":"flat","size_bps":0,"stop_loss_pct":0.1,"take_profit_pct":0.1,"trader_summary":"No edge in chop; stand aside until break."}"#;
        let d = parse(body).expect("flat must parse");
        assert_eq!(d.action, Action::Flat);
        assert_eq!(d.size_bps, 0);
    }

    #[test]
    fn strips_think_block() {
        let body = format!("<think>let me reason...</think>\n{GOLDEN_BUY}");
        parse(&body).expect("think prefix must be stripped");
    }

    #[test]
    fn trims_markdown_fence() {
        let body = format!("```json\n{GOLDEN_BUY}\n```");
        parse(&body).expect("markdown fence must be trimmed");
    }

    #[test]
    fn trims_leading_prose() {
        let body = format!("Here is my decision:\n{GOLDEN_BUY}\nThank you.");
        parse(&body).expect("leading prose must be trimmed");
    }

    #[test]
    fn rejects_empty_body() {
        let err = parse("").expect_err("empty body must fail");
        assert!(matches!(err, TraderError::Empty));
    }

    #[test]
    fn rejects_oversize() {
        let body = r#"{"action":"buy","direction":"long","size_bps":3000,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"Way too big position size."}"#;
        let err = parse(body).expect_err("size_bps>2000 must fail");
        assert!(matches!(err, TraderError::Validation(_)));
    }

    #[test]
    fn rejects_short_summary() {
        let body = r#"{"action":"buy","direction":"long","size_bps":500,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"too short"}"#;
        let err = parse(body).expect_err("short summary must fail");
        assert!(matches!(err, TraderError::Validation(_)));
    }

    #[test]
    fn rejects_zero_stop_loss() {
        let body = r#"{"action":"buy","direction":"long","size_bps":500,"stop_loss_pct":0.0,"take_profit_pct":5.0,"trader_summary":"Missing stop loss is unsafe."}"#;
        let err = parse(body).expect_err("stop_loss_pct=0 must fail");
        assert!(matches!(err, TraderError::Validation(_)));
    }

    #[test]
    fn rejects_invalid_action() {
        let body = r#"{"action":"explode","direction":"long","size_bps":500,"stop_loss_pct":2.0,"take_profit_pct":5.0,"trader_summary":"Invalid action keyword."}"#;
        let err = parse(body).expect_err("invalid action must fail");
        assert!(matches!(err, TraderError::Parse(_)));
    }
}
