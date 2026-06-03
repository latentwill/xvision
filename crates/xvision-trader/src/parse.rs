//! Parse + validate the Trader's JSON response. Pure function; the backend
//! wire is in `run.rs`.
//!
//! The parser is forgiving in the same ways `xvision_intern::parse_llm_response`
//! is forgiving:
//! 1. `<think>...</think>` blocks are stripped (Qwen-thinking, R1, etc.).
//! 2. The body is trimmed to the substring between the first `{` and last `}`,
//!    handling fenced markdown / leading prose.
//! 3. The decoded shape is validated via garde at the boundary.

use garde::Validate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;
use xvision_core::trading::{Action, AssetSymbol, Direction, TraderDecision};
use xvision_intern::strip_reasoning;

use crate::error::TraderError;

/// What the LLM produces. The runtime fills in `cycle_id`. `asset` is
/// optional on the wire: when present, it must match the briefing's asset
/// (the trader is told what it's trading); when absent, the briefing's
/// asset is used as the authoritative fallback. F18 cascade.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmTraderDecision {
    pub action: Action,
    pub direction: Direction,
    pub size_bps: u32,
    pub stop_loss_pct: f32,
    pub take_profit_pct: f32,
    pub trader_summary: String,
    #[serde(default)]
    pub asset: Option<AssetSymbol>,
}

/// Parse + validate a Trader response. The caller supplies the runtime-owned
/// `cycle_id` and the briefing's `asset` (authoritative — the trader was
/// told what asset to trade in its prompt). If the LLM emits its own
/// `asset` field, it must match.
pub fn parse_trader_response(
    body: &str,
    cycle_id: Uuid,
    briefing_asset: AssetSymbol,
) -> Result<TraderDecision, TraderError> {
    if body.trim().is_empty() {
        return Err(TraderError::Empty);
    }

    let stripped = strip_reasoning(body);
    let trimmed = trim_to_json(&stripped);

    let llm: LlmTraderDecision = serde_json::from_str(&trimmed)
        .map_err(|e| TraderError::Parse(format!("{e}; body[..200]={}", short(&trimmed, 200))))?;

    if let Some(emitted) = llm.asset {
        if emitted != briefing_asset {
            return Err(TraderError::Parse(format!(
                "trader emitted asset={emitted:?} but briefing asset={briefing_asset:?}"
            )));
        }
    }

    let decision = TraderDecision {
        cycle_id,
        action: llm.action,
        size_bps: llm.size_bps,
        direction: llm.direction,
        stop_loss_pct: llm.stop_loss_pct,
        take_profit_pct: llm.take_profit_pct,
        trader_summary: llm.trader_summary,
        asset: briefing_asset,
        trailing_stop_pct: None,
        breakeven_trigger_pct: None,
        breakeven_offset_pct: None,
        fade_sl_bars: None,
        fade_sl_start_pct: None,
        fade_sl_end_pct: None,
        max_bars_held: None,
        sl_atr_mult: None,
        tp_atr_mult: None,
        tp1_pct: None,
        tp1_close_fraction: None,
        tp2_pct: None,
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
        parse_trader_response(body, Uuid::nil(), AssetSymbol::Btc)
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
